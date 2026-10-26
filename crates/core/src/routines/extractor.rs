use anyhow::Context;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use tokio::sync::{RwLock, mpsc::UnboundedReceiver};
use tokio_util::time::{DelayQueue, delay_queue::Key};
use tracing::{debug, info, warn};

use crate::{bookkeeper::BookKeeper, ebook_ingest::EbookExtractor, timeutil::Duration};
use shelf_proto::bookkeeping::BookMetadata;
use std::{
    collections::{HashMap, hash_map::Entry},
    future::poll_fn,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

const DEBOUNCE: Duration = Duration::from_secs(5);

/// What a sync found and did.
pub struct SyncReport {
    pub books: usize,
    pub hashed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub removed: usize,
}

/// Brings `.shelf/` up to date with the book files. Unless `force` is set, books unchanged since
/// their `last_update_time` aren't hashed. Blocks while it extracts: call it inside `spawn_blocking`.
pub fn sync_library(root: &Path, force: bool) -> anyhow::Result<SyncReport> {
    let keeper = BookKeeper::new(root)?;
    let hashes = keeper.all_book_hashes()?;
    let hashes = Arc::new(Mutex::new(hashes));
    let known: HashMap<String, BookMetadata> = keeper
        .all_book_metadata()?
        .into_iter()
        .map(|m| (m.path.clone(), m))
        .collect();
    let files = keeper.all_book_file_paths()?;
    let (hashed, skipped, failed) = (
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    );
    files.par_iter().for_each(|f| {
        let result = EbookExtractor::new(root, f).and_then(|e| {
            if !force
                && let Some(known) = known.get(&e.rel_path())
                && let Some(id) = e.unchanged_id(known)?
            {
                skipped.fetch_add(1, Ordering::Relaxed);
                return Ok(id);
            }
            hashed.fetch_add(1, Ordering::Relaxed);
            e.extract()
        });
        match result {
            Ok(hash) => {
                debug!(path = ?f, "synced");
                hashes.lock().unwrap().remove(&hash);
            }
            Err(e) => {
                warn!(path = ?f, error = %e, "couldn't extract, will retry on the next event or start");
                failed.fetch_add(1, Ordering::Relaxed);
            }
        };
    });
    let hashes = Arc::try_unwrap(hashes).unwrap().into_inner().unwrap();
    let removed = hashes.len();
    keeper.clean_up_hashes(hashes)?;
    let report = SyncReport {
        books: files.len(),
        hashed: hashed.into_inner(),
        skipped: skipped.into_inner(),
        failed: failed.into_inner(),
        removed,
    };
    info!(
        books = report.books,
        hashed = report.hashed,
        skipped = report.skipped,
        failed = report.failed,
        removed = report.removed,
        force,
        "library synced"
    );
    Ok(report)
}

/// Syncs each path from the watcher 5 s after its last event, keeping `metadata` up to date and
/// calling `on_change` after each change. Runs until the watcher is dropped.
pub async fn extractor(
    root: PathBuf,
    mut recv: UnboundedReceiver<PathBuf>,
    metadata: Arc<RwLock<Vec<BookMetadata>>>,
    on_change: impl Fn() + Send + Sync + 'static,
) {
    let root: Arc<Path> = root.into();
    let on_change = Arc::new(on_change);
    let mut queue: DelayQueue<PathBuf> = DelayQueue::new();
    let mut keys: HashMap<PathBuf, Key> = HashMap::new();

    loop {
        tokio::select! {
            msg = recv.recv() => match msg {
                Some(path) => {
                    debug!(path = ?path, "watcher event");
                    match keys.entry(path) {
                        Entry::Occupied(e) => queue.reset(e.get(), DEBOUNCE),
                        Entry::Vacant(e) => {
                            let key = queue.insert(e.key().clone(), DEBOUNCE);
                            e.insert(key);
                        }
                    }
                }
                None => break,
            },
            // An empty DelayQueue yields None immediately, so without the guard this would spin.
            Some(expired) = poll_fn(|cx| queue.poll_expired(cx)), if !queue.is_empty() => {
                let path = expired.into_inner();
                keys.remove(&path);
                let (root, metadata, on_change) = (root.clone(), metadata.clone(), on_change.clone());
                rayon::spawn(move || match sync_path(&root, &path, &metadata) {
                    Ok(true) => {
                        info!(path = ?path, "synced");
                        on_change();
                    }
                    Ok(false) => debug!(path = ?path, "unchanged since last sync"),
                    Err(e) => warn!(path = ?path, error = %e, "sync failed"),
                });
            }
        }
    }
}

/// Extracts a book that exists, or drops a gone one from the index. Its `.shelf/books/` folder
/// is left for the startup sync: a move's remove event can arrive before its create event.
/// Returns false when the file hasn't changed since its metadata was written.
fn sync_path(
    root: &Path,
    path: &Path,
    metadata: &RwLock<Vec<BookMetadata>>,
) -> anyhow::Result<bool> {
    if !path.exists() {
        let rel_path = path.strip_prefix(root)?.to_string_lossy();
        metadata.blocking_write().retain(|m| m.path != rel_path);
        return Ok(true);
    }
    let extractor = EbookExtractor::new(root, path)?;
    let rel_path = extractor.rel_path();
    let known = metadata
        .blocking_read()
        .iter()
        .find(|m| m.path == rel_path)
        .cloned();
    if let Some(known) = known
        && extractor.unchanged_id(&known)?.is_some()
    {
        return Ok(false);
    }
    let hash = extractor.extract()?;
    let meta = BookKeeper::new(root)?
        .book_metadata(&hash)?
        .context("no metadata after extraction")?;
    let mut books = metadata.blocking_write();
    // Matching on id means a moved book only changes its `path`.
    match books.iter_mut().find(|b| b.id == meta.id) {
        Some(book) => *book = meta,
        None => books.push(meta),
    }
    Ok(true)
}
