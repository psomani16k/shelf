use anyhow::Context;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use tokio::sync::{RwLock, mpsc::UnboundedReceiver};
use tokio_util::time::{DelayQueue, delay_queue::Key};
use tracing::{debug, info, warn};

use crate::{bookkeeper::BookKeeper, ebook_ingest::EbookExtractor};
use shelf_proto::bookkeeping::BookMetadata;
use std::{
    collections::{HashMap, hash_map::Entry},
    future::poll_fn,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

const DEBOUNCE: Duration = Duration::from_secs(5);

/// Blocks while it extracts every book: call it inside `spawn_blocking`.
pub fn sync_library(root: &Path) -> anyhow::Result<()> {
    let keeper = BookKeeper::new(root)?;
    let hashes = keeper.all_book_hashes()?;
    let hashes = Arc::new(Mutex::new(hashes));
    let files = keeper.all_book_file_paths()?;
    let failed = AtomicUsize::new(0);
    files.par_iter().for_each(|f| {
        let result = EbookExtractor::new(root, f).and_then(|e| e.extract());
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
    info!(
        books = files.len(),
        failed = failed.into_inner(),
        removed,
        "library synced"
    );
    Ok(())
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
                    Ok(()) => {
                        info!(path = ?path, "synced");
                        on_change();
                    }
                    Err(e) => warn!(path = ?path, error = %e, "sync failed"),
                });
            }
        }
    }
}

/// Extracts a book that exists, or drops a gone one from the index. Its `.shelf/books/` folder
/// is left for the startup sync: a move's remove event can arrive before its create event.
fn sync_path(root: &Path, path: &Path, metadata: &RwLock<Vec<BookMetadata>>) -> anyhow::Result<()> {
    if !path.exists() {
        let rel_path = path.strip_prefix(root)?.to_string_lossy();
        metadata.blocking_write().retain(|m| m.path != rel_path);
        return Ok(());
    }
    let hash = EbookExtractor::new(root, path)?.extract()?;
    let meta = BookKeeper::new(root)?
        .book_metadata(&hash)?
        .context("no metadata after extraction")?;
    let mut books = metadata.blocking_write();
    // Matching on id means a moved book only changes its `path`.
    match books.iter_mut().find(|b| b.id == meta.id) {
        Some(book) => *book = meta,
        None => books.push(meta),
    }
    Ok(())
}
