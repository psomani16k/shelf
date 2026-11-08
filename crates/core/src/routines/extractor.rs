use std::{collections::HashMap, path::Path, sync::Arc};

use anyhow::{Context, Result};
use shelf_proto::bookkeeping::BookMetadata;
use shelf_utility::routines::transform_concurrent_blocking;
use tokio::task::spawn_blocking;
use tracing::{debug, info, warn};

use crate::{bookkeeper::BookKeeper, ebook_ingest::EbookExtractor};

#[derive(Debug, Default)]
/// What a sync found and did.
pub struct SyncReport {
    pub books: usize,
    pub hashed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub removed: usize,
}

/// What syncing one book file found.
enum Synced {
    /// Its metadata was current, so it wasn't read.
    Skipped(String),
    /// It was read and hashed, and extracted if needed.
    Hashed(String),
    /// It was hashed but couldn't be extracted. Its folder is kept, progress included.
    Failed(String, anyhow::Error),
}

/// Brings `.shelf/` up to date with the book files, two books per CPU at a time. Unless `force`
/// is set, books unchanged since their `last_update_time` aren't read, and known ones aren't
/// re-extracted. Then deletes the `.shelf/books/` folders no file produced.
pub async fn sync_library(keeper: &BookKeeper, force: bool) -> Result<SyncReport> {
    let listing = keeper.clone();
    let (mut stale, known, files) = spawn_blocking(move || {
        let known: HashMap<String, BookMetadata> = listing
            .all_book_metadata()?
            .into_iter()
            .map(|m| (m.path.clone(), m))
            .collect();
        let files: Vec<_> = listing.all_book_file_paths()?.into_iter().collect();
        anyhow::Ok((listing.all_book_hashes_in_books_dir()?, known, files))
    })
    .await??;

    let known = Arc::new(known);
    // Twice the CPUs: while one job waits on the disk, another can use its core.
    let in_flight = 2 * std::thread::available_parallelism().map_or(2, |n| n.get());
    let results = transform_concurrent_blocking(&files, in_flight, |_, file| {
        let (keeper, known, file) = (keeper.clone(), known.clone(), file.clone());
        move || sync_file(&keeper, &file, &known, force)
    })
    .await;

    let mut report = SyncReport {
        books: files.len(),
        ..Default::default()
    };
    for (file, result) in files.iter().zip(results) {
        match result {
            Ok(Synced::Skipped(id)) => {
                report.skipped += 1;
                stale.remove(&id);
            }
            Ok(Synced::Hashed(id)) => {
                debug!(path = ?file, "synced");
                report.hashed += 1;
                stale.remove(&id);
            }
            Ok(Synced::Failed(id, e)) => {
                warn!(path = ?file, error = %format!("{e:#}"), "couldn't extract, will retry on the next event or start");
                report.failed += 1;
                stale.remove(&id);
            }
            Err(e) => {
                warn!(path = ?file, error = %format!("{e:#}"), "couldn't read, will retry on the next event or start");
                report.failed += 1;
            }
        }
    }

    report.removed = stale.len();
    let cleaner = keeper.clone();
    spawn_blocking(move || cleaner.clean_up_hashes(stale)).await??;
    info!(?report, force, "library synced");
    Ok(report)
}

/// Syncs one book file and returns its id. An `Err` means it couldn't be hashed. Blocking.
fn sync_file(
    keeper: &BookKeeper,
    file: &Path,
    known: &HashMap<String, BookMetadata>,
    force: bool,
) -> Result<Synced> {
    let book = EbookExtractor::new(keeper.clone(), file)?;
    if !force
        && let Some(known) = known.get(&book.rel_path())
        && let Some(id) = book.predict_id_change_using_modified_time(known)?
    {
        return Ok(Synced::Skipped(id));
    }
    let book = book.read()?;
    let id = book.hash().context("deleted during the sync")?.to_owned();
    Ok(match book.extract(force) {
        Ok(_) => Synced::Hashed(id),
        Err(e) => Synced::Failed(id, e),
    })
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
        let keeper = BookKeeper::new(root.path()).unwrap();
        let library = keeper.library_root();

        // Known and unchanged: its stamp is later than its mtime (2025-10-26T04:45:00Z).
        File::create(library.join("a.epub"))
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_761_453_900))
            .unwrap();
        fs::create_dir_all(keeper.book_dir_by_id(EMPTY_SHA1)).unwrap();
        let a = BookMetadata {
            path: "a.epub".into(),
            id: EMPTY_SHA1.into(),
            last_update_time: Some("2025-10-26T05:00:00Z".into()),
            ..Default::default()
        };
        keeper.set_book_metadata(EMPTY_SHA1, &a).unwrap();
        // New and not a valid EPUB.
        fs::write(library.join("bad.epub"), "not a zip").unwrap();
        // No file has this id any more.
        fs::create_dir_all(keeper.book_dir_by_id(OTHER_ID)).unwrap();

        let report = sync_library(&keeper, false).await.unwrap();

        let counts = (report.books, report.skipped, report.hashed, report.failed, report.removed);
        assert_eq!(counts, (2, 1, 0, 1, 1));
        assert!(keeper.book_dir_by_id(EMPTY_SHA1).exists());
        assert!(!keeper.book_dir_by_id(OTHER_ID).exists());
    }

    #[tokio::test]
    async fn forced_sync_keeps_the_folder_of_a_book_that_fails() {
        let root = tempfile::tempdir().unwrap();
        let keeper = BookKeeper::new(root.path()).unwrap();
        let path = keeper.library_root().join("bad.epub");
        fs::write(&path, "not a zip").unwrap();
        let id = EbookExtractor::new(keeper.clone(), &path)
            .unwrap()
            .read()
            .unwrap()
            .hash()
            .unwrap()
            .to_owned();
        let progress = keeper.book_dir_by_id(&id).join("progress.json");
        fs::create_dir_all(keeper.book_dir_by_id(&id)).unwrap();
        fs::write(&progress, "{}").unwrap();

        let report = sync_library(&keeper, true).await.unwrap();

        assert_eq!((report.failed, report.removed), (1, 0));
        assert!(progress.exists());
    }
}
