use std::{path::Path, sync::Arc};

use anyhow::Result;
use shelf_proto::bookkeeping::BookMetadata;
use tokio::sync::RwLock;
use tracing::warn;

use crate::{
    storage::{LibraryStorage, LocalLibraryStorage, is_epub},
    ebook_ingest::EbookExtractor,
};

/// Brings the in-memory index up to date for one path from the watcher, judging only by what is
/// in the storage now. Blocking: run it on the blocking pool.
pub struct IndexSync {
    storage: LocalLibraryStorage,
    index: Arc<RwLock<Vec<BookMetadata>>>,
}

impl IndexSync {
    pub fn new(storage: LocalLibraryStorage, index: Arc<RwLock<Vec<BookMetadata>>>) -> Self {
        Self { storage, index }
    }

    /// `path` is the watcher's absolute path. Returns whether the index changed.
    pub fn sync_path(&self, path: &Path) -> Result<bool> {
        let path = self.storage.relative_path(path)?;
        if self.storage.is_folder(&path)? {
            self.sync_folder(&path)
        } else if is_epub(&path) {
            self.sync_book(&path)
        } else {
            // Gone and not a book: a folder that was moved away or deleted.
            self.drop_folder(&path)
        }
    }

    /// For a folder moved or copied in: inotify reports the folder, not the books inside it.
    fn sync_folder(&self, dir: &Path) -> Result<bool> {
        let mut changed = false;
        for book in self.storage.book_file_paths_under(dir)? {
            match self.sync_book(&book) {
                Ok(book_changed) => changed |= book_changed,
                Err(e) => warn!(path = ?book, error = %format!("{e:#}"), "sync failed"),
            }
        }
        Ok(changed)
    }

    /// Extracts a book that exists, or drops a gone one. A gone book's `.shelf/books/` folder is
    /// left for the startup sync: a move's remove event can arrive before its create event.
    fn sync_book(&self, path: &Path) -> Result<bool> {
        let extractor = EbookExtractor::new(self.storage.clone(), path.to_path_buf());
        let rel_path = extractor.rel_path();
        let known = self
            .index
            .blocking_read()
            .iter()
            .find(|m| m.path == rel_path)
            .cloned();
        if let Some(known) = known
            && extractor
                .predict_id_change_using_modified_time(&known)?
                .is_some()
        {
            return Ok(false);
        }
        let (hash, meta) = extractor.read()?.extract(false)?;
        let mut books = self.index.blocking_write();
        let len = books.len();
        match (hash, meta) {
            (Some(hash), Some(meta)) => {
                if books.contains(&meta) {
                    return Ok(false);
                }
                // By path too: an edited book's new hash leaves its old entry behind.
                books.retain(|b| b.id != hash && b.path != meta.path);
                books.push(meta);
                return Ok(true);
            }
            // Also by path: a move's create job may already have put this id back.
            (Some(hash), None) => books.retain(|b| b.id != hash || b.path != rel_path),
            (None, _) => books.retain(|b| b.path != rel_path),
        }
        Ok(books.len() != len)
    }

    fn drop_folder(&self, path: &Path) -> Result<bool> {
        // The trailing slash keeps `old/` from matching `older/`.
        let prefix = format!("{}/", path.to_string_lossy());
        let mut books = self.index.blocking_write();
        let len = books.len();
        books.retain(|b| !b.path.starts_with(&prefix));
        Ok(books.len() != len)
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};

    use super::*;

    /// The SHA-1 of an empty file.
    const EMPTY_SHA1: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";

    fn setup(index: Vec<BookMetadata>) -> (tempfile::TempDir, IndexSync) {
        let root = tempfile::tempdir().unwrap();
        let storage = LocalLibraryStorage::new(root.path()).unwrap();
        (root, IndexSync::new(storage, Arc::new(RwLock::new(index))))
    }

    fn entry(path: &str) -> BookMetadata {
        BookMetadata {
            path: path.into(),
            id: path.into(),
            ..Default::default()
        }
    }

    fn indexed_paths(sync: &IndexSync) -> Vec<String> {
        let mut paths: Vec<_> = sync
            .index
            .blocking_read()
            .iter()
            .map(|b| b.path.clone())
            .collect();
        paths.sort();
        paths
    }

    #[test]
    fn renamed_folder_moves_its_books() {
        let old = BookMetadata {
            id: EMPTY_SHA1.into(),
            ..entry("old/dune.epub")
        };
        let (_root, sync) = setup(vec![old.clone()]);
        let library = sync.storage.library_root().to_owned();
        // `old/` was renamed to `new/`; `.shelf/` still has the book under its old path.
        fs::create_dir(library.join("new")).unwrap();
        File::create(library.join("new/dune.epub")).unwrap();
        fs::create_dir_all(sync.storage.book_dir_by_id(EMPTY_SHA1)).unwrap();
        sync.storage.set_book_metadata(EMPTY_SHA1, &old).unwrap();

        assert!(sync.sync_path(&library.join("new")).unwrap());
        let index = sync.index.blocking_read();
        assert_eq!(index.len(), 1);
        assert_eq!((index[0].path.as_str(), index[0].id.as_str()), ("new/dune.epub", EMPTY_SHA1));
    }

    #[test]
    fn gone_folder_drops_only_its_books() {
        let books = ["old/a.epub", "old/b.epub", "older/c.epub", "other/d.epub"];
        let (_root, sync) = setup(books.map(entry).to_vec());
        let old = sync.storage.library_root().join("old");

        assert!(sync.sync_path(&old).unwrap());
        assert_eq!(indexed_paths(&sync), ["older/c.epub", "other/d.epub"]);
        assert!(!sync.sync_path(&old).unwrap());
    }

    #[test]
    fn gone_non_book_file_changes_nothing() {
        let (_root, sync) = setup(vec![entry("notes/a.epub")]);
        let notes = sync.storage.library_root().join("notes.txt");

        assert!(!sync.sync_path(&notes).unwrap());
        assert_eq!(indexed_paths(&sync), ["notes/a.epub"]);
    }
}
