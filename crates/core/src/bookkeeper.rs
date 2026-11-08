use std::{
    collections::HashSet,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde_json::{Map, Value};
use shelf_proto::{
    bookkeeping::{BookMetadata, BookProgress},
    config::Config,
};
use std::fs::{self};
use tracing::{debug, warn};

use crate::state::{TMP_MARKER, write_atomic};

pub fn is_epub(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("epub"))
}

#[derive(Debug, Clone)]
pub struct BookKeeper {
    root: Arc<Path>,
}

impl BookKeeper {
    pub fn new(root: &Path) -> anyhow::Result<Self> {
        let root = root.canonicalize()?.into();
        Ok(Self { root })
    }

    pub fn library_root(&self) -> &Path {
        &self.root
    }

    pub fn books_dir(&self) -> PathBuf {
        self.root.join(".shelf/books")
    }

    pub fn book_dir_by_id(&self, hash: &str) -> PathBuf {
        self.root.join(format!(".shelf/books/{}", hash))
    }

    pub fn all_book_hashes_in_books_dir(&self) -> anyhow::Result<HashSet<String>> {
        let book_dir = self.books_dir();
        if !book_dir.exists() {
            return Ok(HashSet::new());
        }
        let contents = fs::read_dir(book_dir)?;
        let mut res = HashSet::new();
        for entry in contents {
            let entry = entry?;
            res.insert(entry.file_name().to_string_lossy().to_string());
        }
        Ok(res)
    }

    pub fn all_book_file_paths(&self) -> anyhow::Result<HashSet<PathBuf>> {
        Self::book_file_paths_under(&self.root)
    }

    /// Every `.epub` file under `dir`, at any depth, skipping dot-files and dot-folders.
    pub fn book_file_paths_under(dir: &Path) -> anyhow::Result<HashSet<PathBuf>> {
        let mut res = HashSet::new();
        let content = std::fs::read_dir(dir)?;
        for p in content {
            let p = p?;
            if p.file_name().to_str().is_some_and(|f| f.starts_with(".")) {
                continue;
            }
            let t = p.file_type()?;
            if t.is_dir() {
                let e = Self::book_file_paths_under(&p.path())?;
                res.extend(e);
            }
            if t.is_file() && is_epub(&p.path()) {
                res.insert(p.path());
            }
        }
        return Ok(res);
    }

    pub fn book_metadata(&self, hash: &str) -> anyhow::Result<Option<BookMetadata>> {
        let book_dir = self.book_dir_by_id(hash);
        let meta_file = book_dir.join("metadata.json");
        if meta_file.exists() {
            let meta = fs::read_to_string(&meta_file)?;
            let meta = serde_json::from_str(&meta)?;
            return Ok(Some(meta));
        }
        Ok(None)
    }

    /// Reads every `.shelf/books/*/metadata.json`, skipping unreadable ones with a warning.
    pub fn all_book_metadata(&self) -> anyhow::Result<Vec<BookMetadata>> {
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

    pub fn set_book_metadata(&self, hash: &str, metadata: &BookMetadata) -> anyhow::Result<()> {
        let book_dir = self.book_dir_by_id(hash);
        let meta_file = book_dir.join("metadata.json");
        fs::write(meta_file, serde_json::to_string_pretty(metadata)?)?;
        Ok(())
    }

    pub fn book_progress(&self, hash: &str) -> anyhow::Result<Option<BookProgress>> {
        let book_dir = self.book_dir_by_id(hash);
        let progress_file = book_dir.join("progress.json");
        if progress_file.exists() {
            let p = fs::read_to_string(&progress_file)?;
            let p = serde_json::from_str(&p)?;
            return Ok(p);
        }
        Ok(None)
    }

    pub fn set_book_progress(&self, hash: &str, progress: BookProgress) -> anyhow::Result<()> {
        let book_dir = self.book_dir_by_id(hash);
        let progress_file = book_dir.join("progress.json");
        fs::write(progress_file, serde_json::to_string_pretty(&progress)?)?;
        Ok(())
    }

    pub fn book_thumbnail_path(&self, hash: &str) -> PathBuf {
        self.book_dir_by_id(hash).join("thumbnail.jpeg")
    }

    pub fn set_book_thumbnail(&self, hash: &str, thumbnail: &[u8]) -> anyhow::Result<()> {
        let book_dir = self.book_dir_by_id(hash);
        let thumbnail_file = book_dir.join("thumbnail.jpeg");
        fs::write(thumbnail_file, thumbnail)?;
        Ok(())
    }

    pub fn book_cleaned_path(&self, book: &Path) -> anyhow::Result<PathBuf> {
        let mut book = book.canonicalize()?;
        if book.starts_with(&self.root) {
            book = book.strip_prefix(&self.root)?.to_path_buf();
        }
        Ok(book)
    }

    pub fn clean_up_hashes(&self, hashes: HashSet<String>) -> anyhow::Result<()> {
        for hash in hashes {
            let _ = fs::remove_dir_all(self.book_dir_by_id(&hash));
        }
        Ok(())
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join(".shelf/config.json")
    }

    /// A missing or unparseable `config.json` gives the default config.
    pub async fn read_config(&self) -> anyhow::Result<Config> {
        let path = self.config_path();
        let json = match tokio::fs::read_to_string(&path).await {
            Ok(json) => json,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Config::default()),
            Err(e) => return Err(e.into()),
        };
        Ok(serde_json::from_str(&json).unwrap_or_else(|e| {
            warn!(path = ?path, error = %e, "unparseable config, using the default");
            Config::default()
        }))
    }

    /// Merges the fields set in `patch` into the stored file and returns the full config.
    /// The merge works on raw JSON, so fields this build doesn't know about survive.
    pub async fn update_config(&self, patch: &Config) -> anyhow::Result<Config> {
        let path = self.config_path();
        let mut stored = match tokio::fs::read_to_string(&path).await {
            Ok(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
                warn!(path = ?path, error = %e, "unparseable config, starting from empty");
                Value::Object(Map::new())
            }),
            Err(e) if e.kind() == ErrorKind::NotFound => Value::Object(Map::new()),
            Err(e) => return Err(e.into()),
        };
        if !stored.is_object() {
            warn!(path = ?path, "config isn't a JSON object, starting from empty");
            stored = Value::Object(Map::new());
        }

        merge(&mut stored, serde_json::to_value(patch)?);
        let config = serde_json::from_value(stored.clone())?;
        write_atomic(&path, serde_json::to_string_pretty(&stored)?.as_bytes()).await?;
        Ok(config)
    }

    /// Deletes temp files that a crash mid-write left in `.shelf/`.
    pub async fn clean_up_temp_files(&self) -> anyhow::Result<()> {
        let dir = self.root.join(".shelf");
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') && name.contains(TMP_MARKER) {
                debug!(file = %name, "removing leftover temp file");
                tokio::fs::remove_file(entry.path()).await?;
            }
        }
        Ok(())
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

    fn keeper(root: &tempfile::TempDir) -> BookKeeper {
        fs::create_dir(root.path().join(".shelf")).unwrap();
        BookKeeper::new(root.path()).unwrap()
    }

    #[test]
    fn merge_changes_only_patched_fields() {
        let mut base = json!({"a": 1, "b": {"c": 2, "d": 3}, "e": [1, 2]});
        merge(&mut base, json!({"b": {"c": 5}, "e": [9]}));
        assert_eq!(base, json!({"a": 1, "b": {"c": 5, "d": 3}, "e": [9]}));
    }

    #[tokio::test]
    async fn missing_config_is_default() {
        let root = tempfile::tempdir().unwrap();
        let keeper = keeper(&root);
        assert_eq!(keeper.read_config().await.unwrap(), Config::default());
    }

    #[tokio::test]
    async fn update_changes_only_sent_fields() {
        let root = tempfile::tempdir().unwrap();
        let keeper = keeper(&root);
        let patch = |json| serde_json::from_value::<Config>(json).unwrap();

        keeper
            .update_config(&patch(json!({"reader": {"flow": "PAGINATED", "theme": "DARK"}})))
            .await
            .unwrap();
        // SCROLLED is the zero value: `optional` is what lets it through.
        let config = keeper
            .update_config(&patch(json!({"reader": {"flow": "SCROLLED"}})))
            .await
            .unwrap();

        assert_eq!(
            serde_json::to_value(&config).unwrap(),
            json!({"reader": {"flow": "SCROLLED", "theme": "DARK"}})
        );
    }

    #[tokio::test]
    async fn update_keeps_unknown_fields() {
        let root = tempfile::tempdir().unwrap();
        let keeper = keeper(&root);
        fs::write(keeper.config_path(), r#"{"future": 1}"#).unwrap();

        keeper.update_config(&Config::default()).await.unwrap();

        let stored: Value =
            serde_json::from_str(&fs::read_to_string(keeper.config_path()).unwrap()).unwrap();
        assert_eq!(stored, json!({"future": 1}));
    }

    #[tokio::test]
    async fn clean_up_removes_only_temp_files() {
        let root = tempfile::tempdir().unwrap();
        let keeper = keeper(&root);
        let shelf = root.path().join(".shelf");
        fs::create_dir(shelf.join("books")).unwrap();
        fs::write(shelf.join("state.json"), "{}").unwrap();
        fs::write(shelf.join(".config.json.tmp-1-2"), "{").unwrap();

        keeper.clean_up_temp_files().await.unwrap();

        assert!(shelf.join("state.json").exists());
        assert!(shelf.join("books").exists());
        assert!(!shelf.join(".config.json.tmp-1-2").exists());
    }
}
