use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};
use shelf_proto::bookkeeping::BookMetadata;
use tokio::sync::RwLock;
use tracing::warn;

use crate::{
    ebook_ingest::EbookExtractor,
    library::locations::{location, remove_from_index, remove_locations, under, upsert_location},
    storage::{LibraryStorage, LocalLibraryStorage, is_epub},
};

/// Brings the in-memory index up to date for one path from the watcher, judging only by what is
/// in the storage now. After the startup sync the index is the truth about where each book's
/// copies are, and `metadata.json` is written from it. Blocking: run it on the blocking pool.
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
            let folder = path.to_string_lossy();
            Ok(remove_from_index(&mut self.index.blocking_write(), under(&folder)))
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

    /// Indexes a copy that exists, or drops a gone one. Works whichever order a move's remove
    /// and create arrive in. A book left with no copies leaves the index; its `.shelf/books/`
    /// folder is left for the startup sync, since the file may just be moving.
    fn sync_book(&self, path: &Path) -> Result<bool> {
        let extractor = EbookExtractor::new(self.storage.clone(), path.to_path_buf());
        let rel_path = extractor.rel_path();
        let here = |p: &str| p == rel_path;
        // Checked before `read`, so a gone copy never costs a read.
        if self.storage.book_modified(path)?.is_none() {
            return Ok(remove_from_index(&mut self.index.blocking_write(), here));
        }
        let known = self
            .index
            .blocking_read()
            .iter()
            .find(|m| location(m, &rel_path).is_some())
            .cloned();
        if let Some(known) = known
            && extractor
                .predict_id_change_using_modified_time(&known)?
                .is_some()
        {
            return Ok(false);
        }
        let Some(meta) = extractor.read()?.extract(false)? else {
            // Deleted while being read.
            return Ok(remove_from_index(&mut self.index.blocking_write(), here));
        };
        let copy = location(&meta, &rel_path)
            .context("extracted without its location")?
            .clone();

        let mut books = self.index.blocking_write();
        let mut changed = Vec::new();
        // An edited copy is a different book now.
        for book in books.iter_mut().filter(|b| b.id != meta.id) {
            if remove_locations(book, here) {
                changed.push(book.id.clone());
            }
        }
        books.retain(|b| !b.locations.is_empty());
        match books.iter_mut().find(|b| b.id == meta.id) {
            Some(book) => {
                if upsert_location(book, copy) {
                    changed.push(book.id.clone());
                }
            }
            None => {
                // The copies `metadata.json` lists may be stale; this one is known to exist.
                books.push(BookMetadata {
                    locations: vec![copy],
                    ..meta
                });
                changed.push(books[books.len() - 1].id.clone());
            }
        }
        for book in books.iter().filter(|b| changed.contains(&b.id)) {
            self.storage.set_book_metadata(&book.id, book)?;
        }
        Ok(!changed.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};

    use shelf_proto::bookkeeping::BookLocation;

    use super::*;

    /// The SHA-1 of an empty file.
    const EMPTY_SHA1: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
    /// The SHA-1 of "x".
    const X_SHA1: &str = "11f6ad8ec52a2984abaafd7c3b516503785c2072";

    fn setup(index: Vec<BookMetadata>) -> (tempfile::TempDir, IndexSync) {
        let root = tempfile::tempdir().unwrap();
        let storage = LocalLibraryStorage::new(root.path()).unwrap();
        (root, IndexSync::new(storage, Arc::new(RwLock::new(index))))
    }

    fn book(id: &str, paths: &[&str]) -> BookMetadata {
        BookMetadata {
            id: id.into(),
            locations: paths
                .iter()
                .map(|p| BookLocation {
                    path: (*p).into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    fn entry(path: &str) -> BookMetadata {
        book(path, &[path])
    }

    /// Writes the file, and `metadata.json` for its id, as a known book would have.
    fn known_file(sync: &IndexSync, path: &str, contents: &str, meta: &BookMetadata) {
        let file = sync.storage.library_root().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
        fs::create_dir_all(sync.storage.book_dir_by_id(&meta.id)).unwrap();
        sync.storage.set_book_metadata(&meta.id, meta).unwrap();
    }

    fn indexed(sync: &IndexSync) -> Vec<(String, Vec<String>)> {
        let mut books: Vec<_> = sync
            .index
            .blocking_read()
            .iter()
            .map(|b| (b.id.clone(), b.locations.iter().map(|l| l.path.clone()).collect()))
            .collect();
        books.sort();
        books
    }

    fn stored_paths(sync: &IndexSync, id: &str) -> Vec<String> {
        let meta = sync.storage.book_metadata(id).unwrap().unwrap();
        meta.locations.into_iter().map(|l| l.path).collect()
    }

    fn indexed_paths(sync: &IndexSync) -> Vec<String> {
        let mut paths: Vec<_> = indexed(sync).into_iter().flat_map(|(_, p)| p).collect();
        paths.sort();
        paths
    }

    #[test]
    fn renamed_folder_moves_its_books() {
        let old = book(EMPTY_SHA1, &["old/dune.epub"]);
        let (_root, sync) = setup(vec![old.clone()]);
        let library = sync.storage.library_root().to_owned();
        // `old/` was renamed to `new/`; `.shelf/` still has the book under its old path.
        known_file(&sync, "new/dune.epub", "", &old);

        assert!(sync.sync_path(&library.join("new")).unwrap());
        assert!(sync.sync_path(&library.join("old")).unwrap());
        let only_new = vec![(EMPTY_SHA1.to_owned(), vec!["new/dune.epub".to_owned()])];
        assert_eq!(indexed(&sync), only_new);
    }

    #[test]
    fn a_move_works_whichever_event_comes_first() {
        let old = book(EMPTY_SHA1, &["a.epub"]);
        for remove_first in [true, false] {
            let (_root, sync) = setup(vec![old.clone()]);
            let library = sync.storage.library_root().to_owned();
            known_file(&sync, "b.epub", "", &old);

            if remove_first {
                sync.sync_path(&library.join("a.epub")).unwrap();
            }
            sync.sync_path(&library.join("b.epub")).unwrap();
            sync.sync_path(&library.join("a.epub")).unwrap();

            assert_eq!(indexed_paths(&sync), ["b.epub"], "remove first: {remove_first}");
        }
    }

    #[test]
    fn two_copies_are_one_book_and_deleting_one_keeps_it() {
        let dune = book(EMPTY_SHA1, &["a.epub"]);
        let (_root, sync) = setup(vec![dune.clone()]);
        let library = sync.storage.library_root().to_owned();
        known_file(&sync, "a.epub", "", &dune);
        known_file(&sync, "b/a.epub", "", &dune);

        assert!(sync.sync_path(&library.join("b/a.epub")).unwrap());
        assert_eq!(indexed_paths(&sync), ["a.epub", "b/a.epub"]);
        assert_eq!(stored_paths(&sync, EMPTY_SHA1), ["a.epub", "b/a.epub"]);
        assert!(!sync.sync_path(&library.join("b/a.epub")).unwrap());

        fs::remove_file(library.join("a.epub")).unwrap();
        assert!(sync.sync_path(&library.join("a.epub")).unwrap());
        assert_eq!(indexed_paths(&sync), ["b/a.epub"]);
    }

    #[test]
    fn an_edited_copy_becomes_another_book_and_the_other_copy_stays() {
        let dune = book(EMPTY_SHA1, &["a.epub", "b.epub"]);
        let (_root, sync) = setup(vec![dune.clone()]);
        let library = sync.storage.library_root().to_owned();
        known_file(&sync, "a.epub", "", &dune);
        // `b.epub` was edited; its new bytes are a known book with no copies yet.
        known_file(&sync, "b.epub", "x", &book(X_SHA1, &[]));

        assert!(sync.sync_path(&library.join("b.epub")).unwrap());
        let books = vec![
            (X_SHA1.to_owned(), vec!["b.epub".to_owned()]),
            (EMPTY_SHA1.to_owned(), vec!["a.epub".to_owned()]),
        ];
        let mut sorted = books.clone();
        sorted.sort();
        assert_eq!(indexed(&sync), sorted);
        assert_eq!(stored_paths(&sync, EMPTY_SHA1), ["a.epub"]);
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
    fn gone_book_is_dropped_without_reading_book_data() {
        let (_root, sync) = setup(vec![entry("a/gone.epub"), entry("a/kept.epub")]);
        let library = sync.storage.library_root().to_owned();
        // Reading `.shelf/books/` would now fail, so this passes only if it isn't scanned.
        fs::create_dir(library.join(".shelf")).unwrap();
        File::create(sync.storage.books_dir()).unwrap();

        assert!(sync.sync_path(&library.join("a/gone.epub")).unwrap());
        assert_eq!(indexed_paths(&sync), ["a/kept.epub"]);
    }

    #[test]
    fn gone_non_book_file_changes_nothing() {
        let (_root, sync) = setup(vec![entry("notes/a.epub")]);
        let notes = sync.storage.library_root().join("notes.txt");

        assert!(!sync.sync_path(&notes).unwrap());
        assert_eq!(indexed_paths(&sync), ["notes/a.epub"]);
    }
}
