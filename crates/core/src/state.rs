use std::{io::ErrorKind, path::Path};

use anyhow::Context;
use shelf_proto::state::State;
use tokio::{fs, io::AsyncWriteExt};
use tracing::{debug, warn};

use crate::timeutil::Time;

const SHELF_DIR: &str = ".shelf";
const STATE_FILE: &str = ".shelf/state.json";
/// Marks a temp file from `write_atomic`: `.<target name>.tmp-<pid>-<nanos>`.
const TMP_MARKER: &str = ".tmp-";

/// A missing or unparseable `state.json` gives the default state.
pub async fn read_state(root: &Path) -> anyhow::Result<State> {
    let path = root.join(STATE_FILE);
    let json = match fs::read_to_string(&path).await {
        Ok(json) => json,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(State::default()),
        Err(e) => return Err(e.into()),
    };
    Ok(serde_json::from_str(&json).unwrap_or_else(|e| {
        warn!(path = ?path, error = %e, "unparseable state, using the default");
        State::default()
    }))
}

pub async fn write_state(root: &Path, state: &State) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(state)?;
    write_atomic(&root.join(STATE_FILE), json.as_bytes()).await
}

/// Writes a hidden temp file beside `path` and renames it over `path`, so a crash or a sync tool
/// never sees half a file. The temp file must be on the same filesystem for the rename to be
/// atomic, which is why it can't go in the OS temp dir.
pub(crate) async fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let dir = path.parent().context("no parent folder")?;
    let name = path.file_name().context("no file name")?.to_string_lossy();
    let tmp = dir.join(format!(
        ".{name}{TMP_MARKER}{}-{}",
        std::process::id(),
        Time::now().unix_nanos()
    ));

    let result = async {
        let mut file = fs::File::create(&tmp).await?;
        file.write_all(bytes).await?;
        file.sync_all().await?;
        fs::rename(&tmp, path).await
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&tmp).await;
    }
    Ok(result?)
}

/// Deletes temp files that a crash mid-write left in `.shelf/`, and the `.shelf/tmp/` folder
/// that earlier builds wrote them to.
pub async fn clean_up_temp_files(root: &Path) -> anyhow::Result<()> {
    let dir = root.join(SHELF_DIR);
    let mut entries = match fs::read_dir(&dir).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') && name.contains(TMP_MARKER) {
            debug!(file = %name, "removing leftover temp file");
            fs::remove_file(entry.path()).await?;
        }
    }
    match fs::remove_dir_all(dir.join("tmp")).await {
        Err(e) if e.kind() != ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_leaves_no_temp_files() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(SHELF_DIR)).await.unwrap();
        let state = State {
            last_book_id: Some("abc".into()),
            ..Default::default()
        };

        write_state(root.path(), &state).await.unwrap();

        assert_eq!(read_state(root.path()).await.unwrap(), state);
        let mut names = Vec::new();
        let mut entries = fs::read_dir(root.path().join(SHELF_DIR)).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            names.push(entry.file_name().into_string().unwrap());
        }
        assert_eq!(names, ["state.json"]);
    }

    #[tokio::test]
    async fn clean_up_removes_only_temp_files() {
        let root = tempfile::tempdir().unwrap();
        let shelf = root.path().join(SHELF_DIR);
        fs::create_dir_all(shelf.join("tmp")).await.unwrap();
        fs::create_dir(shelf.join("books")).await.unwrap();
        fs::write(shelf.join("state.json"), "{}").await.unwrap();
        fs::write(shelf.join(".state.json.tmp-1-2"), "{")
            .await
            .unwrap();

        clean_up_temp_files(root.path()).await.unwrap();

        assert!(shelf.join("state.json").exists());
        assert!(shelf.join("books").exists());
        assert!(!shelf.join(".state.json.tmp-1-2").exists());
        assert!(!shelf.join("tmp").exists());
    }
}
