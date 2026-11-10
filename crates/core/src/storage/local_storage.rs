use std::{
    collections::HashSet,
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::{Context, Result};
use shelf_proto::bookkeeping::{BookMetadata, BookProgress};
use shelf_utility::timeutil::Time;
use tracing::debug;

use super::{FileContent, LibraryStorage, is_epub};

/// Marks a temp file from `write_atomic`: `.<target name>.tmp-<pid>-<nanos>-<seq>`.
const TMP_MARKER: &str = ".tmp-";

/// Tells apart temp files that concurrent writes create in the same nanosecond.
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// A library in a folder on this machine, with its data in `<root>/.shelf/`.
#[derive(Debug, Clone)]
pub struct LocalLibraryStorage {
    root: Arc<Path>,
}

impl LocalLibraryStorage {
    pub fn new(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?.into();
        Ok(Self { root })
    }

    pub fn library_root(&self) -> &Path {
        &self.root
    }

    /// An absolute path inside the library (such as one from the watcher), relative to the root.
    pub fn relative_path(&self, path: &Path) -> Result<PathBuf> {
        // A gone file can't be canonicalized; the watcher's paths already are.
        let path = if path.exists() {
            path.canonicalize()?
        } else {
            path.to_path_buf()
        };
        Ok(path.strip_prefix(&self.root)?.to_path_buf())
    }

    pub fn books_dir(&self) -> PathBuf {
        self.root.join(".shelf/books")
    }

    pub fn book_dir_by_id(&self, hash: &str) -> PathBuf {
        self.root.join(format!(".shelf/books/{}", hash))
    }

    pub fn thumbnail_path(&self, hash: &str) -> PathBuf {
        self.book_dir_by_id(hash).join("thumbnail.jpeg")
    }

    pub fn shelf_file_path(&self, name: &str) -> PathBuf {
        self.root.join(".shelf").join(name)
    }

    pub fn config_path(&self) -> PathBuf {
        self.shelf_file_path("config.json")
    }

    pub fn state_path(&self) -> PathBuf {
        self.shelf_file_path("state.json")
    }

    /// Writes a temp file and renames it over `path`, so no one ever sees half a file. The temp
    /// file sits directly in `.shelf/`: on the same filesystem, and where the cleanup looks.
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let name = path.file_name().context("no file name")?.to_string_lossy();
        let tmp = self.shelf_file_path(&format!(
            ".{name}{TMP_MARKER}{}-{}-{}",
            std::process::id(),
            Time::now().unix_nanos(),
            TMP_SEQ.fetch_add(1, Ordering::Relaxed),
        ));

        let result = (|| {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            fs::rename(&tmp, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result.with_context(|| format!("couldn't write {}", path.display()))
    }

    fn walk(&self, dir: &Path, res: &mut HashSet<PathBuf>) -> Result<()> {
        for p in fs::read_dir(dir)? {
            let p = p?;
            if p.file_name().to_str().is_some_and(|f| f.starts_with(".")) {
                continue;
            }
            let t = p.file_type()?;
            if t.is_dir() {
                self.walk(&p.path(), res)?;
            }
            if t.is_file() && is_epub(&p.path()) {
                res.insert(p.path().strip_prefix(&self.root)?.to_path_buf());
            }
        }
        Ok(())
    }
}

impl LibraryStorage for LocalLibraryStorage {
    fn all_book_file_paths(&self) -> Result<HashSet<PathBuf>> {
        self.book_file_paths_under(Path::new(""))
    }

    fn book_file_paths_under(&self, dir: &Path) -> Result<HashSet<PathBuf>> {
        let mut res = HashSet::new();
        self.walk(&self.root.join(dir), &mut res)?;
        Ok(res)
    }

    fn is_folder(&self, path: &Path) -> Result<bool> {
        Ok(self.root.join(path).is_dir())
    }

    fn book_modified(&self, path: &Path) -> Result<Option<Time>> {
        match fs::metadata(self.root.join(path)) {
            Ok(meta) => Ok(Some(Time::from(meta.modified()?))),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn read_book(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        match fs::read(self.root.join(path)) {
            Ok(book) => Ok(Some(book)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn book_file(&self, path: &Path) -> Result<Option<FileContent>> {
        Ok(Some(FileContent::Local(self.root.join(path))))
    }

    fn all_book_hashes_in_books_dir(&self) -> Result<HashSet<String>> {
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

    fn book_metadata(&self, hash: &str) -> Result<Option<BookMetadata>> {
        let meta_file = self.book_dir_by_id(hash).join("metadata.json");
        if meta_file.exists() {
            let meta = fs::read_to_string(&meta_file)?;
            let meta = serde_json::from_str(&meta)?;
            return Ok(Some(meta));
        }
        Ok(None)
    }

    fn set_book_metadata(&self, hash: &str, metadata: &BookMetadata) -> Result<()> {
        let meta_file = self.book_dir_by_id(hash).join("metadata.json");
        self.write_atomic(&meta_file, serde_json::to_string_pretty(metadata)?.as_bytes())
    }

    fn create_book_dir(&self, hash: &str) -> Result<()> {
        fs::create_dir_all(self.book_dir_by_id(hash))?;
        Ok(())
    }

    fn book_progress(&self, hash: &str) -> Result<Option<BookProgress>> {
        let progress_file = self.book_dir_by_id(hash).join("progress.json");
        if progress_file.exists() {
            let p = fs::read_to_string(&progress_file)?;
            let p = serde_json::from_str(&p)?;
            return Ok(p);
        }
        Ok(None)
    }

    fn set_book_progress(&self, hash: &str, progress: BookProgress) -> Result<()> {
        let progress_file = self.book_dir_by_id(hash).join("progress.json");
        self.write_atomic(&progress_file, serde_json::to_string_pretty(&progress)?.as_bytes())
    }

    async fn book_thumbnail(&self, hash: &str) -> Result<Option<FileContent>> {
        let path = self.thumbnail_path(hash);
        let exists = tokio::fs::try_exists(&path)
            .await
            .with_context(|| format!("couldn't check {}", path.display()))?;
        Ok(exists.then_some(FileContent::Local(path)))
    }

    fn set_book_thumbnail(&self, hash: &str, thumbnail: &[u8]) -> Result<()> {
        self.write_atomic(&self.thumbnail_path(hash), thumbnail)
    }

    fn clean_up_hashes(&self, hashes: HashSet<String>) -> Result<()> {
        for hash in hashes {
            let _ = fs::remove_dir_all(self.book_dir_by_id(&hash));
        }
        Ok(())
    }

    async fn read_shelf_file(&self, name: &str) -> Result<Option<String>> {
        match tokio::fs::read_to_string(self.shelf_file_path(name)).await {
            Ok(json) => Ok(Some(json)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn write_shelf_file(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let (storage, path, bytes) = (self.clone(), self.shelf_file_path(name), bytes.to_vec());
        tokio::task::spawn_blocking(move || storage.write_atomic(&path, &bytes)).await?
    }

    async fn clean_up_temp_files(&self) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use shelf_proto::config::Config;

    use super::*;

    fn storage(root: &tempfile::TempDir) -> LocalLibraryStorage {
        fs::create_dir(root.path().join(".shelf")).unwrap();
        LocalLibraryStorage::new(root.path()).unwrap()
    }

    #[test]
    fn lists_books_relative_to_the_root() {
        let root = tempfile::tempdir().unwrap();
        let storage = storage(&root);
        fs::create_dir_all(root.path().join("a/.hidden")).unwrap();
        for file in ["top.EPUB", "a/b.epub", "a/notes.txt", "a/.c.epub", "a/.hidden/d.epub"] {
            fs::write(root.path().join(file), "").unwrap();
        }

        let paths = |set: HashSet<PathBuf>| {
            let mut v: Vec<_> = set.into_iter().collect();
            v.sort();
            v
        };
        assert_eq!(
            paths(storage.all_book_file_paths().unwrap()),
            [PathBuf::from("a/b.epub"), PathBuf::from("top.EPUB")]
        );
        assert_eq!(
            paths(storage.book_file_paths_under(Path::new("a")).unwrap()),
            [PathBuf::from("a/b.epub")]
        );
    }

    #[tokio::test]
    async fn missing_config_is_default() {
        let root = tempfile::tempdir().unwrap();
        let storage = storage(&root);
        assert_eq!(storage.read_config().await.unwrap(), Config::default());
    }

    #[tokio::test]
    async fn update_changes_only_sent_fields() {
        let root = tempfile::tempdir().unwrap();
        let storage = storage(&root);
        let patch = |json| serde_json::from_value::<Config>(json).unwrap();

        storage
            .update_config(&patch(json!({"reader": {"flow": "PAGINATED", "theme": "DARK"}})))
            .await
            .unwrap();
        // SCROLLED is the zero value: `optional` is what lets it through.
        let config = storage
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
        let storage = storage(&root);
        fs::write(storage.config_path(), r#"{"future": 1}"#).unwrap();

        storage.update_config(&Config::default()).await.unwrap();

        let stored: Value =
            serde_json::from_str(&fs::read_to_string(storage.config_path()).unwrap()).unwrap();
        assert_eq!(stored, json!({"future": 1}));
    }

    #[test]
    fn book_data_writes_leave_no_temp_files() {
        let root = tempfile::tempdir().unwrap();
        let storage = storage(&root);
        let id = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
        storage.create_book_dir(id).unwrap();
        let meta = BookMetadata {
            id: id.into(),
            ..Default::default()
        };

        storage.set_book_metadata(id, &meta).unwrap();
        storage.set_book_metadata(id, &meta).unwrap();
        storage.set_book_progress(id, BookProgress::default()).unwrap();
        storage.set_book_thumbnail(id, b"jpeg").unwrap();

        assert_eq!(storage.book_metadata(id).unwrap(), Some(meta));
        assert_eq!(fs::read(storage.thumbnail_path(id)).unwrap(), b"jpeg");
        let names = |dir: PathBuf| {
            let mut v: Vec<_> = fs::read_dir(dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            v.sort();
            v
        };
        assert_eq!(names(root.path().join(".shelf")), ["books"]);
        assert_eq!(
            names(storage.book_dir_by_id(id)),
            ["metadata.json", "progress.json", "thumbnail.jpeg"]
        );
    }

    #[tokio::test]
    async fn clean_up_removes_only_temp_files() {
        let root = tempfile::tempdir().unwrap();
        let storage = storage(&root);
        let shelf = root.path().join(".shelf");
        fs::create_dir(shelf.join("books")).unwrap();
        fs::write(shelf.join("state.json"), "{}").unwrap();
        fs::write(shelf.join(".config.json.tmp-1-2"), "{").unwrap();

        storage.clean_up_temp_files().await.unwrap();

        assert!(shelf.join("state.json").exists());
        assert!(shelf.join("books").exists());
        assert!(!shelf.join(".config.json.tmp-1-2").exists());
    }
}
