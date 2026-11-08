use std::{io::ErrorKind, path::Path};

use anyhow::Context;
use shelf_proto::state::State;
use tokio::{fs, io::AsyncWriteExt};
use tracing::warn;

use shelf_utility::timeutil::Time;

const SHELF_DIR: &str = ".shelf";
const STATE_FILE: &str = ".shelf/state.json";
/// Marks a temp file from `write_atomic`: `.<target name>.tmp-<pid>-<nanos>`.
pub(crate) const TMP_MARKER: &str = ".tmp-";

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
