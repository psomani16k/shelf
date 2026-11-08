use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, anyhow};

use shelf_proto::{
    api::{
        GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListBooksResponseItem, ListDirectoriesRequest,
        ListDirectoriesResponse, RebuildIndexResponse, SetProgressRequest, SetStateRequest,
    },
    bookkeeping::{BookMetadata, BookProgress, EpubProgress, book_file_type},
    config::Config,
};
use shelf_utility::routines::Debounced;
use tokio::sync::{Mutex, RwLock, mpsc::UnboundedReceiver};
use tracing::{debug, info, warn};

use crate::{
    bookkeeper::BookKeeper,
    library::{Library, LibraryError},
    routines::{extractor::sync_library, index_sync::IndexSync},
    state::{read_state, write_state},
    watcher::LibraryWatcher,
};

pub struct BookLibrary {
    keeper: BookKeeper,
    metadata: Arc<RwLock<Vec<BookMetadata>>>,
    // Two concurrent sets would each read the old file, and one would lose the other's fields.
    config_lock: Mutex<()>,
    // Dropping it closes the extractor's channel, which ends the extractor task.
    _watcher: LibraryWatcher,
}

impl BookLibrary {
    const DEBOUNCE: Duration = Duration::from_secs(5);

    /// Starts the watcher, syncs `.shelf/` with the folder, loads every book's metadata, then
    /// starts the extractor, which calls `on_change` after each book it syncs. Must run inside tokio.
    pub async fn open(root: PathBuf, on_change: impl Fn() + Send + Sync + 'static) -> Result<Self> {
        let (mut watcher, recv) = LibraryWatcher::initialize(&root)?;
        watcher.watch().await?;
        // Events that arrive during the sync wait in `recv` until the extractor starts.
        let keeper = {
            let root = root.clone();
            tokio::task::spawn_blocking(move || BookKeeper::new(&root)).await??
        };
        sync_library(&keeper, false).await?;
        let listing = keeper.clone();
        let metadata = tokio::task::spawn_blocking(move || listing.all_book_metadata()).await??;
        info!(root = %root.display(), books = metadata.len(), "library opened");

        if let Err(e) = keeper.clean_up_temp_files().await {
            warn!(error = %e, "couldn't remove leftover temp files");
        }

        let library = Self {
            keeper,
            metadata: Arc::new(RwLock::new(metadata)),
            config_lock: Mutex::new(()),
            _watcher: watcher,
        };
        library.receive_and_process_library_updates(recv, on_change);
        Ok(library)
    }

    /// Syncs each path from the watcher 5 s after its last event, on the blocking pool with one
    /// job per CPU at most. Runs until the watcher is dropped.
    fn receive_and_process_library_updates(
        &self,
        recv: UnboundedReceiver<PathBuf>,
        on_change: impl Fn() + Send + Sync + 'static,
    ) {
        let sync = IndexSync::new(self.keeper.clone(), self.metadata.clone());
        let job = move |path: PathBuf| match sync.sync_path(&path) {
            Ok(true) => {
                info!(path = ?path, "synced");
                on_change();
            }
            Ok(false) => debug!(path = ?path, "unchanged since last sync"),
            Err(e) => warn!(path = ?path, error = %format!("{e:#}"), "sync failed"),
        };
        tokio::spawn(Debounced::new(Self::DEBOUNCE).receive_and_process_blocking(job, recv));
    }

    /// Ids index into `.shelf/books/`, so only known ones may reach the disk (no `../` tricks).
    async fn has_book(&self, id: &str) -> bool {
        self.metadata.read().await.iter().any(|m| m.id == id)
    }

    /// Runs blocking `BookKeeper` file I/O off the async worker threads.
    async fn with_keeper<T: Send + 'static>(
        &self,
        f: impl FnOnce(BookKeeper) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let keeper = self.keeper.clone();
        tokio::task::spawn_blocking(move || f(keeper)).await?
    }
}

impl Library for BookLibrary {
    async fn list_books(&self, req: ListBooksRequest) -> ListBooksResponse {
        let (dir, prefix) = folder_prefix(&req.directory_path);

        let mut books: Vec<_> = self
            .metadata
            .read()
            .await
            .iter()
            .filter_map(|m| {
                let file_name = m.path.strip_prefix(&prefix)?;
                // Books in subfolders belong to that subfolder's listing.
                if file_name.contains('/') {
                    return None;
                }
                Some(ListBooksResponseItem {
                    id: m.id.clone(),
                    file_name: file_name.to_owned(),
                    title: m.title.clone(),
                    authors: m.authors.clone(),
                })
            })
            .collect();
        books.sort_by_cached_key(|b| b.title.as_deref().unwrap_or(&b.file_name).to_lowercase());

        ListBooksResponse {
            current_directory: dir.to_owned(),
            books,
        }
    }

    async fn list_sub_directories(&self, req: ListDirectoriesRequest) -> ListDirectoriesResponse {
        let (dir, prefix) = folder_prefix(&req.directory_path);

        let metadata = self.metadata.read().await;
        let subfolders: BTreeSet<&str> = metadata
            .iter()
            .filter_map(|m| m.path.strip_prefix(&prefix)?.split_once('/'))
            .map(|(subfolder, _)| subfolder)
            .collect();
        let mut sub_directories: Vec<String> = subfolders.into_iter().map(String::from).collect();
        sub_directories.sort_by_cached_key(|d| d.to_lowercase());

        ListDirectoriesResponse {
            current_directory: dir.to_owned(),
            sub_directories,
        }
    }

    async fn get_cover(&self, id: &str) -> Result<PathBuf, LibraryError> {
        if !self.has_book(id).await {
            return Err(LibraryError::NotFound);
        }
        let path = self.keeper.book_thumbnail_path(id);
        let exists = tokio::fs::try_exists(&path)
            .await
            .with_context(|| format!("couldn't check {}", path.display()))?;
        // A book without a cover image has no thumbnail.
        if exists {
            Ok(path)
        } else {
            Err(LibraryError::NotFound)
        }
    }

    async fn get_book(&self, id: &str) -> Result<PathBuf, LibraryError> {
        let metadata = self.metadata.read().await;
        let book = metadata
            .iter()
            .find(|m| m.id == id)
            .ok_or(LibraryError::NotFound)?;
        // `path` comes from synced `.shelf/` data: an absolute path or `..` would escape the library.
        let rel = Path::new(&book.path);
        if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
            return Err(anyhow!("book {id} has an unsafe path: {}", book.path).into());
        }
        Ok(self.keeper.library_root().join(rel))
    }

    async fn get_progress(
        &self,
        req: GetProgressRequest,
    ) -> Result<GetProgressResponse, LibraryError> {
        if !self.has_book(&req.id).await {
            return Err(LibraryError::NotFound);
        }

        let id = req.id.clone();
        let epub_progress = match self.with_keeper(move |k| k.book_progress(&id)).await {
            Ok(progress) => progress.and_then(|p| p.epub_progress).map(|e| e.cfi),
            Err(e) => {
                warn!(id = %req.id, error = %e, "unreadable progress file");
                None
            }
        };
        Ok(GetProgressResponse {
            id: req.id,
            epub_progress,
        })
    }

    async fn set_progress(&self, req: SetProgressRequest) -> Result<(), LibraryError> {
        if !self.has_book(&req.id).await {
            return Err(LibraryError::NotFound);
        }
        let Some(cfi) = req.epub_progress else {
            return Ok(());
        };

        let progress = BookProgress {
            file_type: book_file_type::Value::Epub as i32,
            epub_progress: Some(EpubProgress { cfi }),
        };
        self.with_keeper(move |k| k.set_book_progress(&req.id, progress))
            .await?;
        Ok(())
    }

    async fn get_state(&self, _req: GetStateRequest) -> Result<GetStateResponse, LibraryError> {
        let state = read_state(self.keeper.library_root()).await?;
        let last_book_id = match state.last_book_id {
            Some(id) if self.has_book(&id).await => Some(id),
            _ => None,
        };
        Ok(GetStateResponse { last_book_id })
    }

    async fn set_state(&self, req: SetStateRequest) -> Result<(), LibraryError> {
        if let Some(id) = &req.last_book_id
            && !self.has_book(id).await
        {
            return Err(LibraryError::NotFound);
        }
        // Read first, so fields this server doesn't know about survive the write.
        let mut state = read_state(self.keeper.library_root()).await?;
        state.last_book_id = req.last_book_id;
        write_state(self.keeper.library_root(), &state).await?;
        Ok(())
    }

    async fn get_config(&self) -> Result<Config, LibraryError> {
        Ok(self.keeper.read_config().await?)
    }

    async fn set_config(&self, patch: Config) -> Result<Config, LibraryError> {
        let _guard = self.config_lock.lock().await;
        Ok(self.keeper.update_config(&patch).await?)
    }

    async fn rebuild_index(&self) -> Result<RebuildIndexResponse, LibraryError> {
        let report = sync_library(&self.keeper, true).await?;
        let metadata = self.with_keeper(|k| k.all_book_metadata()).await?;
        *self.metadata.write().await = metadata;
        Ok(RebuildIndexResponse {
            books: report.books as u32,
            failed: report.failed as u32,
            removed: report.removed as u32,
        })
    }
}

/// Returns the folder with slashes trimmed (`""` is the root) and the prefix its contents' paths start with.
fn folder_prefix(path: &str) -> (&str, String) {
    let dir = path.trim_matches('/');
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    (dir, prefix)
}
