use std::{collections::HashSet, path::PathBuf, sync::Arc};

use anyhow::Result;
use bytes::Bytes;

use shelf_proto::{
    api::{
        AddAnnotationRequest, DeleteAnnotationRequest, ListAnnotationsRequest,
        ListAnnotationsResponse, UpdateAnnotationRequest,
        CleanUpProgressResponse, CreateDirectoryRequest, CreateDirectoryResponse, DeleteItemsRequest, DeleteItemsResponse,
        ExportUserDataRequest, ImportUserDataRequest, ImportUserDataResponse, UserData,
        DeleteThemeRequest, GetConfigRequest, GetProgressRequest, GetProgressResponse,
        GetStateRequest, GetStateResponse, ListBooksRequest, ListBooksResponse,
        ListDirectoriesRequest, ListDirectoriesResponse, ListThemesRequest, ListThemesResponse,
        RebuildIndexResponse, RenameThemeRequest, SetConfigRequest, SetProgressRequest,
        SetStateRequest, SetThemeRequest, UploadEbookRequest, UploadEbookResponse,
    },
    bookkeeping::{Annotation, BookMetadata},
    config::{Config, ReaderConfig},
};
use shelf_utility::{routines::Debounced, timeutil::Duration};
use tokio::sync::{Mutex, RwLock, mpsc::UnboundedReceiver};
use tracing::{debug, info, warn};

use crate::{
    storage::{FileContent, LibraryStorage, LocalLibraryStorage},
    library::{Library, LibraryError, scope::EVERYTHING, user::validate_user},
    routines::{extractor::sync_library, index_sync::IndexSync},
    themes,
    watcher::LibraryWatcher,
};

/// The whole library. `view` gives one user's part of it; the `Library` methods see everything.
pub struct BookLibrary {
    pub(super) storage: LocalLibraryStorage,
    pub(super) metadata: Arc<RwLock<Vec<BookMetadata>>>,
    // Two concurrent sets would each read the old file, and one would lose the other's fields.
    // Also held for the users' themes, whose renames change the config too.
    pub(super) config_lock: Mutex<()>,
    // Each annotation change reads a user's whole file for a book, changes it and writes it back.
    pub(super) annotations_lock: Mutex<()>,
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
    /// starts the extractor, which calls `on_change` after each sync that changed the index.
    /// Must run inside tokio.
    pub async fn open(root: PathBuf, on_change: impl Fn() + Send + Sync + 'static) -> Result<Self> {
        // Events that arrive during the sync wait in `recv` until the extractor starts.
        let (watcher, recv, storage) = {
            let root = root.clone();
            tokio::task::spawn_blocking(move || -> Result<_> {
                let (mut watcher, recv) = LibraryWatcher::initialize(&root)?;
                watcher.watch()?;
                Ok((watcher, recv, LocalLibraryStorage::new(&root)?))
            })
            .await??
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
            annotations_lock: Mutex::new(()),
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

    /// The users with data in `.shelf/users/`, sorted; what a local library offers to sign in as.
    pub async fn user_names(&self) -> Result<Vec<String>> {
        self.with_storage(|s| s.user_names()).await
    }
}

impl Library for BookLibrary {
    async fn list_books(&self, req: ListBooksRequest) -> Result<ListBooksResponse, LibraryError> {
        self.view(&EVERYTHING).list_books(req).await
    }

    async fn list_sub_directories(
        &self,
        req: ListDirectoriesRequest,
    ) -> Result<ListDirectoriesResponse, LibraryError> {
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

    async fn list_annotations(
        &self,
        req: ListAnnotationsRequest,
    ) -> Result<ListAnnotationsResponse, LibraryError> {
        self.view(&EVERYTHING).list_annotations(req).await
    }

    async fn add_annotation(&self, req: AddAnnotationRequest) -> Result<Annotation, LibraryError> {
        self.view(&EVERYTHING).add_annotation(req).await
    }

    async fn update_annotation(
        &self,
        req: UpdateAnnotationRequest,
    ) -> Result<Annotation, LibraryError> {
        self.view(&EVERYTHING).update_annotation(req).await
    }

    async fn delete_annotation(&self, req: DeleteAnnotationRequest) -> Result<(), LibraryError> {
        self.view(&EVERYTHING).delete_annotation(req).await
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

    async fn list_themes(&self, req: ListThemesRequest) -> Result<ListThemesResponse, LibraryError> {
        validate_user(&req.user)?;
        themes::list_themes(&self.storage, &req.user).await
    }

    async fn set_theme(&self, req: SetThemeRequest) -> Result<ListThemesResponse, LibraryError> {
        validate_user(&req.user)?;
        let _guard = self.config_lock.lock().await;
        themes::set_theme(&self.storage, &req.user, req.spec.unwrap_or_default()).await?;
        themes::list_themes(&self.storage, &req.user).await
    }

    async fn delete_theme(&self, req: DeleteThemeRequest) -> Result<ListThemesResponse, LibraryError> {
        validate_user(&req.user)?;
        let _guard = self.config_lock.lock().await;
        themes::delete_theme(&self.storage, &req.user, &req.name).await?;
        themes::list_themes(&self.storage, &req.user).await
    }

    async fn rename_theme(&self, req: RenameThemeRequest) -> Result<ListThemesResponse, LibraryError> {
        validate_user(&req.user)?;
        let _guard = self.config_lock.lock().await;
        let (from, to) = themes::rename_theme(&self.storage, &req.user, &req.from, &req.to).await?;
        let chosen = self.storage.read_config(&req.user).await?.reader.and_then(|r| r.theme);
        if from != to && chosen.as_deref() == Some(from.as_str()) {
            let patch = Config {
                reader: Some(ReaderConfig {
                    theme: Some(to),
                    ..Default::default()
                }),
                ..Default::default()
            };
            self.storage.update_config(&req.user, &patch).await?;
        }
        themes::list_themes(&self.storage, &req.user).await
    }

    async fn export_user_data(&self, req: ExportUserDataRequest) -> Result<UserData, LibraryError> {
        self.view(&EVERYTHING).export_user_data(req).await
    }

    async fn import_user_data(
        &self,
        req: ImportUserDataRequest,
    ) -> Result<ImportUserDataResponse, LibraryError> {
        self.view(&EVERYTHING).import_user_data(req).await
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
