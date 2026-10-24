use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use tokio_util::time::{DelayQueue, delay_queue::Key};
use tracing::{error, info, warn};

use crate::{bookkeeper::BookKeeper, ebook_ingest::EbookExtractor, watcher::LibraryWatcher};
use std::{
    collections::{HashMap, hash_map::Entry},
    future::poll_fn,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

const DEBOUNCE: Duration = Duration::from_secs(5);

async fn sync_library(root: PathBuf) -> anyhow::Result<()> {
    let keeper = BookKeeper::new(&root)?;
    let hashes = keeper.all_book_hashes()?;
    let hashes = Arc::new(Mutex::new(hashes));
    let files = keeper.all_book_file_paths()?;
    files.par_iter().for_each(|f| {
        let extractor = EbookExtractor::new(&root, &f)
            .expect(&format!("Failed to creae an extractor for {:?}", f));
        match extractor.extract() {
            Ok(hash) => {
                info!("Synced {:?}", f);
                hashes.lock().unwrap().remove(&hash);
            }
            Err(e) => {
                error!("Failed to sync {:?} due to {}", f, e);
            }
        };
    });
    let hashes = Arc::try_unwrap(hashes).unwrap().into_inner().unwrap();
    keeper.clean_up_hashes(hashes.into())?;
    Ok(())
}

async fn extractor(root: PathBuf) -> anyhow::Result<()> {
    let (mut watcher, mut recv) = LibraryWatcher::initialize(&root)?;
    watcher.watch().await?;
    let mut queue: DelayQueue<PathBuf> = DelayQueue::new();
    let mut keys: HashMap<PathBuf, Key> = HashMap::new();

    loop {
        tokio::select! {
            msg = recv.recv() => match msg {
                Some(path) => match keys.entry(path) {
                    Entry::Occupied(e) => queue.reset(e.get(), DEBOUNCE),
                    Entry::Vacant(e) => {
                        let key = queue.insert(e.key().clone(), DEBOUNCE);
                        e.insert(key);
                    }
                },
                None => break,
            },
            // An empty DelayQueue yields None immediately, so without the guard this would spin.
            Some(expired) = poll_fn(|cx| queue.poll_expired(cx)), if !queue.is_empty() => {
                let path = expired.into_inner();
                keys.remove(&path);
                let root = root.clone();
                rayon::spawn(move || {
                    match EbookExtractor::new(&root, &path).and_then(|e| e.extract()) {
                        Ok(_) => info!(path = ?path, "synced"),
                        Err(e) => warn!(path = ?path, error = %e, "sync failed"),
                    }
                });
            }
        }
    }
    Ok(())
}
