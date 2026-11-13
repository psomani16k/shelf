use std::{
    collections::HashMap,
    convert::Infallible,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use anyhow::{Context, Result};
use shelf_core::library::{scope::Scope, user::validate_user};
use shelf_proto::server::ServerData;
use shelf_utility::timeutil::Time;
use tokio::{fs, io::AsyncWriteExt};
use tracing::{error, info, warn};

const FILE_NAME: &str = "data.json";

/// `<dir>/data.json`, held in memory. `refresh` picks up hand edits, and `update` re-reads the
/// file before changing it, so the server never writes over an edit it hasn't seen.
pub struct DataFile {
    dir: PathBuf,
    data: ServerData,
    /// Each user's `library_root` and `books`, built whenever `data` is.
    scopes: Scopes,
    /// The mtime of the version in `data`, or of a broken version already reported.
    mtime: Option<SystemTime>,
}

impl DataFile {
    /// Creates the file with no users if it's missing. A file that can't be parsed, or names an
    /// invalid user id, library root or book pattern, stops the server rather than being
    /// overwritten.
    pub async fn load(dir: &Path, now: Time) -> Result<Self> {
        let path = dir.join(FILE_NAME);
        let (data, scopes, mtime) = match read(&path, now).await? {
            Some(Loaded { data, scopes, mtime }) => (data, scopes, Some(mtime)),
            None => {
                let data = ServerData::default();
                write_atomic(dir, &path, &data).await?;
                info!(path = %path.display(), "created server data");
                (data, Scopes::new(), modified(&path).await.ok())
            }
        };
        Ok(Self {
            dir: dir.to_owned(),
            data,
            scopes,
            mtime,
        })
    }

    pub fn data(&self) -> &ServerData {
        &self.data
    }

    /// The part of the library the user sees; `None` for an unknown user.
    pub fn scope(&self, user: &str) -> Option<Arc<Scope>> {
        self.scopes.get(user).cloned()
    }

    /// Reloads the file if it changed on disk. A broken edit is logged once and the last good
    /// copy stays in use.
    pub async fn refresh(&mut self, now: Time) {
        let path = self.path();
        let mtime = match modified(&path).await {
            Ok(mtime) => mtime,
            Err(e) => {
                if self.mtime.take().is_some() {
                    warn!(error = %e, "can't see the server data file, keeping the copy in memory");
                }
                return;
            }
        };
        if self.mtime == Some(mtime) {
            return;
        }
        self.mtime = Some(mtime);
        match read(&path, now).await {
            Ok(Some(Loaded { data, scopes, mtime })) => {
                self.data = data;
                self.scopes = scopes;
                self.mtime = Some(mtime);
                info!(users = self.data.users.len(), "server data reloaded");
            }
            Ok(None) => warn!("the server data file is gone, keeping the copy in memory"),
            Err(e) => error!(
                error = %format!("{e:#}"),
                "couldn't reload the server data, keeping the last good copy"
            ),
        }
    }

    /// Applies `change` to the file as it is on disk now, and writes it back. Fails without
    /// writing if the file can't be read or parsed.
    pub async fn update<T>(
        &mut self,
        now: Time,
        change: impl FnOnce(&mut ServerData) -> T,
    ) -> Result<T> {
        let out = self
            .try_update(now, |data| Ok::<_, Infallible>(change(data)))
            .await?;
        Ok(out.unwrap_or_else(|never| match never {}))
    }

    /// Like `update`, but nothing is written when `change` fails; its error is the inner one.
    pub async fn try_update<T, E>(
        &mut self,
        now: Time,
        change: impl FnOnce(&mut ServerData) -> Result<T, E>,
    ) -> Result<Result<T, E>> {
        let path = self.path();
        let Loaded { mut data, .. } = read(&path, now)
            .await?
            .with_context(|| format!("{} is gone", path.display()))?;
        let out = match change(&mut data) {
            Ok(out) => out,
            Err(e) => return Ok(Err(e)),
        };
        drop_expired_sessions(&mut data, now);
        let scopes = build_scopes(&data)?;
        write_atomic(&self.dir, &path, &data).await?;
        self.data = data;
        self.scopes = scopes;
        self.mtime = modified(&path).await.ok();
        Ok(Ok(out))
    }

    fn path(&self) -> PathBuf {
        self.dir.join(FILE_NAME)
    }
}

async fn modified(path: &Path) -> std::io::Result<SystemTime> {
    fs::metadata(path).await?.modified()
}

type Scopes = HashMap<String, Arc<Scope>>;

struct Loaded {
    data: ServerData,
    scopes: Scopes,
    mtime: SystemTime,
}

/// The file and its mtime, read before the contents so a later edit is never missed; `None` if
/// it's missing.
async fn read(path: &Path, now: Time) -> Result<Option<Loaded>> {
    let mtime = match modified(path).await {
        Ok(mtime) => mtime,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("couldn't read {}", path.display())),
    };
    let json = fs::read_to_string(path)
        .await
        .with_context(|| format!("couldn't read {}", path.display()))?;
    let mut data: ServerData = serde_json::from_str(&json)
        .with_context(|| format!("{} isn't valid server data", path.display()))?;
    for id in data.users.keys() {
        validate_user(id).with_context(|| format!("bad user id in {}", path.display()))?;
    }
    let scopes = build_scopes(&data).with_context(|| format!("in {}", path.display()))?;
    drop_expired_sessions(&mut data, now);
    Ok(Some(Loaded {
        data,
        scopes,
        mtime,
    }))
}

fn build_scopes(data: &ServerData) -> Result<Scopes> {
    data.users
        .iter()
        .map(|(id, user)| {
            let scope = Scope::new(&user.library_root, &user.books)
                .with_context(|| format!("user {id}"))?;
            Ok((id.clone(), Arc::new(scope)))
        })
        .collect()
}

/// Also drops sessions whose expiry isn't a timestamp, which only a hand edit can cause.
fn drop_expired_sessions(data: &mut ServerData, now: Time) {
    for user in data.users.values_mut() {
        user.sessions
            .retain(|_, s| s.expires.parse::<Time>().is_ok_and(|expires| expires > now));
    }
}

/// A temp file in the same folder, then a rename, so a crash never leaves half a file.
async fn write_atomic(dir: &Path, path: &Path, data: &ServerData) -> Result<()> {
    let tmp = dir.join(format!(".{FILE_NAME}.tmp"));
    let json = serde_json::to_string_pretty(data)? + "\n";
    let write = async {
        // Only the server's user can read the passwords.
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .await?;
        file.write_all(json.as_bytes()).await?;
        file.sync_all().await?;
        fs::rename(&tmp, path).await
    };
    if let Err(e) = write.await {
        let _ = fs::remove_file(&tmp).await;
        return Err(e).with_context(|| format!("couldn't write {}", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{os::unix::fs::PermissionsExt, time::Duration};

    use shelf_proto::{api::role, server::Session};

    use super::*;

    fn now() -> Time {
        "2026-11-13T10:00:00Z".parse().unwrap()
    }

    fn write(dir: &Path, json: &str) {
        std::fs::write(dir.join(FILE_NAME), json).unwrap();
    }

    fn contents(dir: &Path) -> String {
        std::fs::read_to_string(dir.join(FILE_NAME)).unwrap()
    }

    /// A different mtime than the last write, even on a coarse clock.
    fn edit(dir: &Path, json: &str) {
        let file = dir.join(FILE_NAME);
        let before = std::fs::metadata(&file).unwrap().modified().unwrap();
        write(dir, json);
        let after = std::fs::File::options().write(true).open(&file).unwrap();
        after.set_modified(before + Duration::from_secs(1)).unwrap();
    }

    #[tokio::test]
    async fn missing_file_is_created_empty_and_private() {
        let dir = tempfile::tempdir().unwrap();

        let file = DataFile::load(dir.path(), now()).await.unwrap();

        assert!(file.data().users.is_empty());
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [FILE_NAME]);
        let mode = std::fs::metadata(dir.path().join(FILE_NAME)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[tokio::test]
    async fn existing_users_load_and_the_file_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let json = r#"{ "users": { "parth": { "password": "hunter2" } } }"#;
        write(dir.path(), json);

        let file = DataFile::load(dir.path(), now()).await.unwrap();

        assert_eq!(file.data().users["parth"].password, "hunter2");
        assert_eq!(contents(dir.path()), json);
    }

    #[tokio::test]
    async fn roles_load_by_name_and_default_to_banned() {
        let dir = tempfile::tempdir().unwrap();
        let json = r#"{ "users": { "a": { "role": "OWNER" }, "b": { "role": "EDITOR" }, "c": {} } }"#;
        write(dir.path(), json);

        let file = DataFile::load(dir.path(), now()).await.unwrap();

        let users = &file.data().users;
        assert_eq!(users["a"].role(), role::Value::Owner);
        assert_eq!(users["b"].role(), role::Value::Editor);
        assert_eq!(users["c"].role(), role::Value::Banned);
    }

    #[tokio::test]
    async fn bad_files_stop_the_server_and_are_kept() {
        let bad = [
            r#"{ "users": { "Not OK": {} } }"#,
            r#"{ "users": "#,
            // A typo must not quietly give fewer (or more) rights.
            r#"{ "users": { "a": { "role": "ADMN" } } }"#,
            // Nor a typo in what they may see.
            r#"{ "users": { "a": { "library_root": "../a" } } }"#,
            r#"{ "users": { "a": { "books": ["a["] } } }"#,
        ];
        for json in bad {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), json);

            assert!(DataFile::load(dir.path(), now()).await.is_err(), "{json}");
            assert_eq!(contents(dir.path()), json);
        }
    }

    #[tokio::test]
    async fn sessions_round_trip_and_expired_ones_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let json = r#"{ "users": { "a": { "sessions": {
            "live": { "created": "2026-11-01T00:00:00Z", "expires": "2027-01-01T00:00:00Z" },
            "old": { "created": "2026-01-01T00:00:00Z", "expires": "2026-02-01T00:00:00Z" },
            "junk": { "expires": "soon" }
        } } } }"#;
        write(dir.path(), json);
        let mut file = DataFile::load(dir.path(), now()).await.unwrap();
        assert_eq!(file.data().users["a"].sessions.len(), 1);

        file.update(now(), |data| {
            let session = Session {
                created: now().to_string(),
                expires: (now() + Duration::from_secs(60)).to_string(),
            };
            data.users.get_mut("a").unwrap().sessions.insert("new".into(), session);
        })
        .await
        .unwrap();

        let reloaded = DataFile::load(dir.path(), now()).await.unwrap();
        let sessions = &reloaded.data().users["a"].sessions;
        assert_eq!(sessions.keys().collect::<Vec<_>>(), ["live", "new"]);
        assert_eq!(sessions["new"].expires, "2026-11-13T10:01:00Z");
    }

    #[tokio::test]
    async fn update_keeps_a_hand_edit_made_after_load() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), r#"{ "users": { "a": {} } }"#);
        let mut file = DataFile::load(dir.path(), now()).await.unwrap();
        edit(dir.path(), r#"{ "users": { "a": {}, "b": { "password": "x" } } }"#);

        file.update(now(), |data| data.users.get_mut("a").unwrap().password = "new".into())
            .await
            .unwrap();

        let reloaded = DataFile::load(dir.path(), now()).await.unwrap();
        let users = &reloaded.data().users;
        assert_eq!(users["a"].password, "new");
        assert_eq!(users["b"].password, "x");
    }

    #[tokio::test]
    async fn a_broken_edit_is_never_written_over() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), r#"{ "users": { "a": { "password": "x" } } }"#);
        let mut file = DataFile::load(dir.path(), now()).await.unwrap();
        let broken = r#"{ "users": "#;
        edit(dir.path(), broken);

        file.refresh(now()).await;
        assert_eq!(file.data().users["a"].password, "x");
        assert!(file.update(now(), |_| ()).await.is_err());
        assert_eq!(contents(dir.path()), broken);
    }

    #[tokio::test]
    async fn refresh_picks_up_a_hand_edit() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), r#"{ "users": { "a": { "role": "OWNER" } } }"#);
        let mut file = DataFile::load(dir.path(), now()).await.unwrap();

        edit(dir.path(), r#"{ "users": { "a": { "role": "READER" } } }"#);
        file.refresh(now()).await;

        assert_eq!(file.data().users["a"].role(), role::Value::Reader);
    }
}
