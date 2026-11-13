use std::{collections::HashMap, path::Path, sync::Arc};

use anyhow::{Context, Result, anyhow};
use shelf_proto::bookkeeping::{BookLocation, BookMetadata};
use shelf_utility::routines::transform_concurrent_blocking;
use tokio::task::spawn_blocking;
use tracing::{debug, info, warn};

use crate::{
    ebook_ingest::EbookExtractor,
    library::locations::location,
    storage::{LibraryStorage, LocalLibraryStorage},
};

#[derive(Debug, Default)]
/// What a sync found and did.
pub struct SyncReport {
    pub books: usize,
    pub hashed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub removed: usize,
}

/// What syncing one book file found: its id, and its location.
enum Synced {
    /// Its copy's stamp was current, so it wasn't read.
    Skipped(String, BookLocation),
    /// It was read and hashed, and extracted if needed.
    Hashed(String, BookLocation),
    /// It was hashed but couldn't be extracted. Its folder is kept, progress included, and its
    /// location has no stamp, so it's read again next time.
    Failed(String, BookLocation, anyhow::Error),
}

/// Brings `.shelf/` up to date with the book files, two books per CPU at a time. Unless `force`
/// is set, copies unchanged since their stamp aren't read, and known books aren't re-extracted.
/// Then sets each book's locations to the copies found, and deletes the metadata and thumbnail of
/// books no file produced. Their folders and progress stay (`clean_up_progress` deletes them).
pub async fn sync_library(storage: &LocalLibraryStorage, force: bool) -> Result<SyncReport> {
    let listing = storage.clone();
    let (mut stale, before, files) = spawn_blocking(move || {
        let before: HashMap<String, BookMetadata> = listing
            .all_book_metadata()?
            .into_iter()
            .map(|m| (m.id.clone(), m))
            .collect();
        let files: Vec<_> = listing.all_book_file_paths()?.into_iter().collect();
        anyhow::Ok((listing.all_book_hashes_in_books_dir()?, before, files))
    })
    .await??;

    // Each copy's path, to the book it was last seen as.
    let known: HashMap<String, BookMetadata> = before
        .values()
        .flat_map(|m| m.locations.iter().map(move |l| (l.path.clone(), m.clone())))
        .collect();
    let known = Arc::new(known);
    // Twice the CPUs: while one job waits on the disk, another can use its core.
    let in_flight = 2 * std::thread::available_parallelism().map_or(2, |n| n.get());
    let results = transform_concurrent_blocking(&files, in_flight, |_, file| {
        let (storage, known, file) = (storage.clone(), known.clone(), file.clone());
        move || sync_file(&storage, &file, &known, force)
    })
    .await;

    let mut report = SyncReport {
        books: files.len(),
        ..Default::default()
    };
    let mut found: HashMap<String, Vec<BookLocation>> = HashMap::new();
    for (file, result) in files.iter().zip(results) {
        let (id, copy) = match result {
            Ok(Synced::Skipped(id, copy)) => {
                report.skipped += 1;
                (id, copy)
            }
            Ok(Synced::Hashed(id, copy)) => {
                debug!(path = ?file, "synced");
                report.hashed += 1;
                (id, copy)
            }
            Ok(Synced::Failed(id, copy, e)) => {
                warn!(path = ?file, error = %format!("{e:#}"), "couldn't extract, will retry on the next event or start");
                report.failed += 1;
                (id, copy)
            }
            Err(e) => {
                warn!(path = ?file, error = %format!("{e:#}"), "couldn't read, will retry on the next event or start");
                report.failed += 1;
                continue;
            }
        };
        stale.remove(&id);
        found.entry(id).or_default().push(copy);
    }

    let writer = storage.clone();
    spawn_blocking(move || set_locations(&writer, found, &before)).await?;
    let cleaner = storage.clone();
    report.removed = spawn_blocking(move || cleaner.clean_up_books(stale)).await??;
    info!(?report, force, "library synced");
    Ok(report)
}

/// Syncs one book file and returns its id and location. An `Err` means it couldn't be hashed.
/// Blocking.
fn sync_file(
    storage: &LocalLibraryStorage,
    file: &Path,
    known: &HashMap<String, BookMetadata>,
    force: bool,
) -> Result<Synced> {
    let book = EbookExtractor::new(storage.clone(), file.to_path_buf());
    let path = book.rel_path();
    if !force
        && let Some(known) = known.get(&path)
        && let Some(id) = book.predict_id_change_using_modified_time(known)?
        && let Some(copy) = location(known, &path)
    {
        return Ok(Synced::Skipped(id, copy.clone()));
    }
    let book = book.read()?;
    let id = book.hash().context("deleted during the sync")?.to_owned();
    Ok(match book.extract(force) {
        Ok(Some(meta)) => {
            let copy = location(&meta, &path).context("extracted without its location")?;
            Synced::Hashed(id, copy.clone())
        }
        Ok(None) => return Err(anyhow!("deleted during the sync")),
        Err(e) => {
            let copy = BookLocation {
                path,
                last_update_time: String::new(),
            };
            Synced::Failed(id, copy, e)
        }
    })
}

/// Writes each book's `metadata.json` whose locations aren't the copies found. A book with no
/// `metadata.json` (new, and its extraction failed) is left for the next sync. Blocking.
fn set_locations(
    storage: &LocalLibraryStorage,
    found: HashMap<String, Vec<BookLocation>>,
    before: &HashMap<String, BookMetadata>,
) {
    for (id, mut copies) in found {
        copies.sort_by(|a, b| a.path.cmp(&b.path));
        if before.get(&id).is_some_and(|m| m.locations == copies) {
            continue;
        }
        let written = storage.book_metadata(&id).and_then(|meta| match meta {
            Some(mut meta) if meta.locations != copies => {
                meta.locations = copies;
                storage.set_book_metadata(&id, &meta)
            }
            _ => Ok(()),
        });
        if let Err(e) = written {
            warn!(%id, error = %format!("{e:#}"), "couldn't save the book's locations");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        time::{Duration, UNIX_EPOCH},
    };

    use super::*;

    /// The SHA-1 of an empty file.
    const EMPTY_SHA1: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
    const OTHER_ID: &str = "0000000000000000000000000000000000000000";

    #[tokio::test]
    async fn sync_counts_skips_failures_and_stale_folders() {
        let root = tempfile::tempdir().unwrap();
        let storage = LocalLibraryStorage::new(root.path()).unwrap();
        let library = storage.library_root();

        // Known and unchanged: its stamp is later than its mtime (2025-10-26T04:45:00Z).
        File::create(library.join("a.epub"))
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_761_453_900))
            .unwrap();
        fs::create_dir_all(storage.book_dir_by_id(EMPTY_SHA1)).unwrap();
        let a = BookMetadata {
            id: EMPTY_SHA1.into(),
            locations: vec![copy("a.epub", "2025-10-26T05:00:00Z")],
            ..Default::default()
        };
        storage.set_book_metadata(EMPTY_SHA1, &a).unwrap();
        // New and not a valid EPUB.
        fs::write(library.join("bad.epub"), "not a zip").unwrap();
        // No file has this id any more.
        fs::create_dir_all(storage.book_dir_by_id(OTHER_ID)).unwrap();

        let report = sync_library(&storage, false).await.unwrap();

        let counts = (report.books, report.skipped, report.hashed, report.failed, report.removed);
        assert_eq!(counts, (2, 1, 0, 1, 0));
        assert!(storage.book_dir_by_id(EMPTY_SHA1).exists());
        // Nothing to delete in it, and folders are never deleted here.
        assert!(storage.book_dir_by_id(OTHER_ID).exists());
    }

    fn copy(path: &str, stamp: &str) -> BookLocation {
        BookLocation {
            path: path.into(),
            last_update_time: stamp.into(),
        }
    }

    /// An empty file, modified at 2025-10-26T04:45:00Z, known as `EMPTY_SHA1` at `known`.
    fn empty_book(storage: &LocalLibraryStorage, path: &str, known: &[BookLocation]) {
        let file = storage.library_root().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        File::create(&file)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_761_453_900))
            .unwrap();
        fs::create_dir_all(storage.book_dir_by_id(EMPTY_SHA1)).unwrap();
        let meta = BookMetadata {
            id: EMPTY_SHA1.into(),
            title: Some("Dune".into()),
            locations: known.to_vec(),
            ..Default::default()
        };
        storage.set_book_metadata(EMPTY_SHA1, &meta).unwrap();
    }

    fn paths(storage: &LocalLibraryStorage) -> Vec<String> {
        let meta = storage.book_metadata(EMPTY_SHA1).unwrap().unwrap();
        meta.locations.into_iter().map(|l| l.path).collect()
    }

    #[tokio::test]
    async fn every_copy_becomes_a_location_and_gone_ones_are_dropped() {
        let root = tempfile::tempdir().unwrap();
        let storage = LocalLibraryStorage::new(root.path()).unwrap();
        let known = [copy("a.epub", "2025-10-26T05:00:00Z"), copy("gone.epub", "2025-10-26T05:00:00Z")];
        empty_book(&storage, "a.epub", &known);
        empty_book(&storage, "b/a.epub", &known);

        let report = sync_library(&storage, false).await.unwrap();

        // The known copy's stamp covers its mtime; the new copy is hashed.
        assert_eq!((report.skipped, report.hashed), (1, 1));
        assert_eq!(paths(&storage), ["a.epub", "b/a.epub"]);
        let again = sync_library(&storage, false).await.unwrap();
        assert_eq!((again.skipped, again.hashed), (2, 0));
    }

    #[tokio::test]
    async fn a_gone_book_keeps_only_its_progress() {
        let root = tempfile::tempdir().unwrap();
        let storage = LocalLibraryStorage::new(root.path()).unwrap();
        let gone = BookMetadata {
            id: OTHER_ID.into(),
            locations: vec![copy("gone.epub", "2025-10-26T05:00:00Z")],
            ..Default::default()
        };
        let dir = storage.book_dir_by_id(OTHER_ID);
        fs::create_dir_all(dir.join("progress")).unwrap();
        fs::write(dir.join("progress/default.json"), "{}").unwrap();
        storage.set_book_metadata(OTHER_ID, &gone).unwrap();
        storage.set_book_thumbnail(OTHER_ID, b"jpeg").unwrap();

        let report = sync_library(&storage, false).await.unwrap();

        assert_eq!(report.removed, 1);
        assert!(!dir.join("metadata.json").exists());
        assert!(!storage.thumbnail_path(OTHER_ID).exists());
        assert!(dir.join("progress/default.json").exists());
        assert!(dir.exists());
        // Kept progress isn't counted again.
        assert_eq!(sync_library(&storage, false).await.unwrap().removed, 0);
    }

    #[tokio::test]
    async fn forced_sync_keeps_the_folder_of_a_book_that_fails() {
        let root = tempfile::tempdir().unwrap();
        let storage = LocalLibraryStorage::new(root.path()).unwrap();
        fs::write(storage.library_root().join("bad.epub"), "not a zip").unwrap();
        let id = EbookExtractor::new(storage.clone(), "bad.epub".into())
            .read()
            .unwrap()
            .hash()
            .unwrap()
            .to_owned();
        let progress = storage.book_dir_by_id(&id).join("progress/default.json");
        fs::create_dir_all(progress.parent().unwrap()).unwrap();
        fs::write(&progress, "{}").unwrap();

        let report = sync_library(&storage, true).await.unwrap();

        assert_eq!((report.failed, report.removed), (1, 0));
        assert!(progress.exists());
    }

    #[tokio::test]
    async fn corrupt_metadata_keeps_the_folder_and_extracts_again() {
        let root = tempfile::tempdir().unwrap();
        let storage = LocalLibraryStorage::new(root.path()).unwrap();
        File::create(storage.library_root().join("a.epub")).unwrap();
        let dir = storage.book_dir_by_id(EMPTY_SHA1);
        fs::create_dir_all(dir.join("progress")).unwrap();
        // Half a file, as a crash mid-write would leave it.
        fs::write(dir.join("metadata.json"), r#"{"path": "a.ep"#).unwrap();
        fs::write(dir.join("progress/default.json"), "{}").unwrap();

        let report = sync_library(&storage, false).await.unwrap();

        // An empty file isn't an EPUB, so extracting it again fails, but its id is known.
        assert_eq!((report.failed, report.removed), (1, 0));
        assert!(dir.join("progress/default.json").exists());
    }
}
