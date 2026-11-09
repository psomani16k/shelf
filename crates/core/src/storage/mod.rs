pub mod local_storage;
#[cfg(feature = "s3")]
pub mod s3_storage;

use std::{
    collections::HashSet,
    future::Future,
    path::{Path, PathBuf},
};

use anyhow::Result;
use bytes::Bytes;
use serde_json::{Map, Value};
use shelf_proto::{
    bookkeeping::{BookMetadata, BookProgress},
    config::Config,
    state::State,
};
use shelf_utility::timeutil::Time;
use tracing::warn;

pub use local_storage::LocalLibraryStorage;

const STATE_FILE: &str = "state.json";
const CONFIG_FILE: &str = "config.json";

pub fn is_epub(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("epub"))
}

/// Where a stored file's bytes are: a local file callers can stream, or bytes already fetched.
#[derive(Debug)]
pub enum FileContent {
    Local(PathBuf),
    Bytes(Bytes),
}

/// Where a library's book files and `.shelf/` data live. Book paths are relative to the library
/// root, with `/` separators. The non-async methods block.
pub trait LibraryStorage: Sync {
    /// Every `.epub` file in the library, at any depth, skipping dot-files and dot-folders.
    fn all_book_file_paths(&self) -> Result<HashSet<PathBuf>>;

    /// Every `.epub` file under `dir`, with the same rules as `all_book_file_paths`.
    fn book_file_paths_under(&self, dir: &Path) -> Result<HashSet<PathBuf>>;

    fn is_folder(&self, path: &Path) -> Result<bool>;

    /// `None` if the book is gone.
    fn book_modified(&self, path: &Path) -> Result<Option<Time>>;

    /// `None` if the book is gone.
    fn read_book(&self, path: &Path) -> Result<Option<Vec<u8>>>;

    /// `None` if the book is gone (a local file isn't checked).
    fn book_file(&self, path: &Path) -> impl Future<Output = Result<Option<FileContent>>> + Send;

    fn all_book_hashes_in_books_dir(&self) -> Result<HashSet<String>>;

    fn book_metadata(&self, hash: &str) -> Result<Option<BookMetadata>>;

    /// Reads every book's metadata, skipping unreadable ones with a warning.
    fn all_book_metadata(&self) -> Result<Vec<BookMetadata>> {
        Ok(self
            .all_book_hashes_in_books_dir()?
            .into_iter()
            .filter_map(|hash| match self.book_metadata(&hash) {
                Ok(meta) => meta,
                Err(e) => {
                    warn!(id = %hash, error = %e, "skipped unreadable metadata");
                    None
                }
            })
            .collect())
    }

    fn set_book_metadata(&self, hash: &str, metadata: &BookMetadata) -> Result<()>;

    fn create_book_dir(&self, hash: &str) -> Result<()>;

    fn book_progress(&self, hash: &str) -> Result<Option<BookProgress>>;

    fn set_book_progress(&self, hash: &str, progress: BookProgress) -> Result<()>;

    /// `None` if the book has no thumbnail.
    fn book_thumbnail(&self, hash: &str)
    -> impl Future<Output = Result<Option<FileContent>>> + Send;

    fn set_book_thumbnail(&self, hash: &str, thumbnail: &[u8]) -> Result<()>;

    fn clean_up_hashes(&self, hashes: HashSet<String>) -> Result<()>;

    /// A file directly in `.shelf/`; `None` if it's missing.
    fn read_shelf_file(&self, name: &str) -> impl Future<Output = Result<Option<String>>> + Send;

    /// Replaces a file directly in `.shelf/` all at once: readers never see half of it.
    fn write_shelf_file(&self, name: &str, bytes: &[u8]) -> impl Future<Output = Result<()>> + Send;

    /// Deletes temp files that a crash mid-write left behind.
    fn clean_up_temp_files(&self) -> impl Future<Output = Result<()>> + Send;

    /// A missing or unparseable state gives the default state.
    fn read_state(&self) -> impl Future<Output = Result<State>> + Send {
        async {
            let Some(json) = self.read_shelf_file(STATE_FILE).await? else {
                return Ok(State::default());
            };
            Ok(serde_json::from_str(&json).unwrap_or_else(|e| {
                warn!(file = STATE_FILE, error = %e, "unparseable state, using the default");
                State::default()
            }))
        }
    }

    fn write_state(&self, state: &State) -> impl Future<Output = Result<()>> + Send {
        async move {
            let json = serde_json::to_string_pretty(state)?;
            self.write_shelf_file(STATE_FILE, json.as_bytes()).await
        }
    }

    /// A missing or unparseable config gives the default config.
    fn read_config(&self) -> impl Future<Output = Result<Config>> + Send {
        async {
            let Some(json) = self.read_shelf_file(CONFIG_FILE).await? else {
                return Ok(Config::default());
            };
            Ok(serde_json::from_str(&json).unwrap_or_else(|e| {
                warn!(file = CONFIG_FILE, error = %e, "unparseable config, using the default");
                Config::default()
            }))
        }
    }

    /// Merges the fields set in `patch` into the stored config and returns the full config.
    /// The merge works on the raw JSON, so fields this build doesn't know about survive.
    fn update_config(&self, patch: &Config) -> impl Future<Output = Result<Config>> + Send {
        async move {
            let mut stored = match self.read_shelf_file(CONFIG_FILE).await? {
                Some(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
                    warn!(file = CONFIG_FILE, error = %e, "unparseable config, starting from empty");
                    Value::Object(Map::new())
                }),
                None => Value::Object(Map::new()),
            };
            if !stored.is_object() {
                warn!(file = CONFIG_FILE, "config isn't a JSON object, starting from empty");
                stored = Value::Object(Map::new());
            }

            merge(&mut stored, serde_json::to_value(patch)?);
            let config = serde_json::from_value(stored.clone())?;
            let json = serde_json::to_string_pretty(&stored)?;
            self.write_shelf_file(CONFIG_FILE, json.as_bytes()).await?;
            Ok(config)
        }
    }
}

/// Objects merge key by key; anything else, lists included, replaces what's stored.
fn merge(base: &mut Value, patch: Value) {
    match (base, patch) {
        (Value::Object(base), Value::Object(patch)) => {
            for (key, value) in patch {
                merge(base.entry(key).or_insert(Value::Null), value);
            }
        }
        (base, patch) => *base = patch,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn merge_changes_only_patched_fields() {
        let mut base = json!({"a": 1, "b": {"c": 2, "d": 3}, "e": [1, 2]});
        merge(&mut base, json!({"b": {"c": 5}, "e": [9]}));
        assert_eq!(base, json!({"a": 1, "b": {"c": 5, "d": 3}, "e": [9]}));
    }
}
