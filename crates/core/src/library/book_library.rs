use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Duration};

use anyhow::Result;
use bytes::Bytes;

use shelf_proto::{
    api::{
        CleanUpProgressResponse, CreateDirectoryRequest, CreateDirectoryResponse, DeleteItemsRequest, DeleteItemsResponse,
        GetConfigRequest, GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListDirectoriesRequest, ListDirectoriesResponse,
        RebuildIndexResponse, SetConfigRequest, SetProgressRequest, SetStateRequest,
        UploadEbookRequest, UploadEbookResponse,
    },
    bookkeeping::BookMetadata,
    config::Config,
};
use shelf_utility::routines::Debounced;
use tokio::sync::{Mutex, RwLock, mpsc::UnboundedReceiver};
use tracing::{debug, info, warn};

use crate::{
    storage::{FileContent, LibraryStorage, LocalLibraryStorage},
    library::{Library, LibraryError, scope::EVERYTHING, user::validate_user},
    routines::{extractor::sync_library, index_sync::IndexSync},
    watcher::LibraryWatcher,
};

/// The whole library. `view` gives one user's part of it; the `Library` methods see everything.
pub struct BookLibrary {
    pub(super) storage: LocalLibraryStorage,
    pub(super) metadata: Arc<RwLock<Vec<BookMetadata>>>,
    // Two concurrent sets would each read the old file, and one would lose the other's fields.
    config_lock: Mutex<()>,
    // Uploads, new folders and deletes check what's on disk, then change it.
    pub(super) files_lock: Mutex<()>,
    // Watcher jobs hold it to read and a rebuild to write, so a rebuild never interleaves with
    // their index and `metadata.json` writes.
    sync_gate: Arc<RwLock<()>>,
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
        let storage = {
            let root = root.clone();
            tokio::task::spawn_blocking(move || LocalLibraryStorage::new(&root)).await??
        };
        sync_library(&storage, false).await?;
        let listing = storage.clone();
        let metadata = tokio::task::spawn_blocking(move || listing.all_book_metadata()).await??;
        info!(root = %root.display(), books = metadata.len(), "library opened");

        if let Err(e) = storage.clean_up_temp_files().await {
            warn!(error = %e, "couldn't remove leftover temp files");
        }

        let library = Self {
            storage,
            metadata: Arc::new(RwLock::new(metadata)),
            config_lock: Mutex::new(()),
            files_lock: Mutex::new(()),
            sync_gate: Arc::new(RwLock::new(())),
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
        let sync = IndexSync::new(self.storage.clone(), self.metadata.clone());
        let gate = self.sync_gate.clone();
        let job = move |path: PathBuf| {
            let _gate = gate.blocking_read();
            match sync.sync_path(&path) {
                Ok(true) => {
                    info!(path = ?path, "synced");
                    on_change();
                }
                Ok(false) => debug!(path = ?path, "unchanged since last sync"),
                Err(e) => warn!(path = ?path, error = %format!("{e:#}"), "sync failed"),
            }
        };
        tokio::spawn(Debounced::new(Self::DEBOUNCE).receive_and_process_blocking(job, recv));
    }

    /// Runs blocking `LocalLibraryStorage` file I/O off the async worker threads.
    pub(super) async fn with_storage<T: Send + 'static>(
        &self,
        f: impl FnOnce(LocalLibraryStorage) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let storage = self.storage.clone();
        tokio::task::spawn_blocking(move || f(storage)).await?
    }
}

impl Library for BookLibrary {
    async fn list_books(&self, req: ListBooksRequest) -> ListBooksResponse {
        self.view(&EVERYTHING).list_books(req).await
    }

    async fn list_sub_directories(&self, req: ListDirectoriesRequest) -> ListDirectoriesResponse {
        self.view(&EVERYTHING).list_sub_directories(req).await
    }

    async fn get_cover(&self, id: &str) -> Result<FileContent, LibraryError> {
        self.view(&EVERYTHING).get_cover(id).await
    }

    async fn get_book(&self, id: &str) -> Result<FileContent, LibraryError> {
        self.view(&EVERYTHING).get_book(id).await
    }

    async fn get_progress(
        &self,
        req: GetProgressRequest,
    ) -> Result<GetProgressResponse, LibraryError> {
        self.view(&EVERYTHING).get_progress(req).await
    }

    async fn set_progress(&self, req: SetProgressRequest) -> Result<(), LibraryError> {
        self.view(&EVERYTHING).set_progress(req).await
    }

    async fn get_state(&self, req: GetStateRequest) -> Result<GetStateResponse, LibraryError> {
        self.view(&EVERYTHING).get_state(req).await
    }

    async fn set_state(&self, req: SetStateRequest) -> Result<(), LibraryError> {
        self.view(&EVERYTHING).set_state(req).await
    }

    async fn get_config(&self, req: GetConfigRequest) -> Result<Config, LibraryError> {
        validate_user(&req.user)?;
        Ok(self.storage.read_config(&req.user).await?)
    }

    async fn set_config(&self, req: SetConfigRequest) -> Result<Config, LibraryError> {
        validate_user(&req.user)?;
        let patch = req.patch.unwrap_or_default();
        let _guard = self.config_lock.lock().await;
        Ok(self.storage.update_config(&req.user, &patch).await?)
    }

    /// Waits for uploads, deletes and watcher jobs, so none of them interleaves with it.
    async fn rebuild_index(&self) -> Result<RebuildIndexResponse, LibraryError> {
        let _files = self.files_lock.lock().await;
        let _gate = self.sync_gate.write().await;
        let report = sync_library(&self.storage, true).await?;
        let metadata = self.with_storage(|s| s.all_book_metadata()).await?;
        *self.metadata.write().await = metadata;
        Ok(RebuildIndexResponse {
            books: report.books as u32,
            failed: report.failed as u32,
            removed: report.removed as u32,
        })
    }

    /// Takes the same locks as `rebuild_index`, so no book's folder is half made meanwhile.
    async fn clean_up_progress(&self) -> Result<CleanUpProgressResponse, LibraryError> {
        let _files = self.files_lock.lock().await;
        let _gate = self.sync_gate.write().await;
        // A book still here whose `metadata.json` another device's sync deleted keeps its folder.
        let keep: HashSet<String> = self.metadata.read().await.iter().map(|m| m.id.clone()).collect();
        let books = self.with_storage(move |s| s.clean_up_progress(&keep)).await?;
        info!(books, "cleaned up the progress of removed books");
        Ok(CleanUpProgressResponse {
            books: books as u32,
        })
    }

    async fn upload_book(
        &self,
        req: UploadEbookRequest,
        book: Bytes,
    ) -> Result<UploadEbookResponse, LibraryError> {
        self.view(&EVERYTHING).upload_book(req, book).await
    }

    async fn create_directory(
        &self,
        req: CreateDirectoryRequest,
    ) -> Result<CreateDirectoryResponse, LibraryError> {
        self.view(&EVERYTHING).create_directory(req).await
    }

    async fn delete_items(&self, req: DeleteItemsRequest) -> Result<DeleteItemsResponse, LibraryError> {
        self.view(&EVERYTHING).delete_items(req).await
    }
}
