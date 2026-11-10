use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use shelf_core::{
    library::{Library, LibraryError, book_library::BookLibrary},
    storage::FileContent,
};
use shelf_proto::{
    api::{
        GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListDirectoriesRequest, ListDirectoriesResponse,
        RebuildIndexResponse, SetProgressRequest, SetStateRequest,
    },
    config::Config,
};
use tokio::sync::Mutex;

use crate::libsrc::{LibrarySource, app_library::AppLibrary};

#[derive(Clone, Deserialize, Serialize)]
pub struct LocalLibraryBuilder {
    root: String,
    name: String,
}

impl LocalLibraryBuilder {
    pub fn new(root: &Path, name: &str) -> Self {
        Self {
            root: root.to_string_lossy().to_string(),
            name: name.to_string(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    /// Opens the folder: starts its watcher and syncs `.shelf/`, so it takes as long as the sync.
    pub async fn build(self, on_change: impl Fn() + Send + Sync + 'static) -> Result<LocalLibrary> {
        LocalLibrary::new(Path::new(&self.root), self.name, on_change).await
    }
}

pub struct LocalLibrary {
    book_library: BookLibrary,
    // A second rebuild waits for the first instead of hashing everything twice at once.
    rebuild_lock: Mutex<()>,
    root: PathBuf,
    name: String,
}

impl LocalLibrary {
    /// `on_change` runs after the watcher syncs a book, so the UI can refresh its listing.
    pub async fn new(
        root: &Path,
        name: String,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self> {
        let lib = BookLibrary::open(root.to_path_buf(), on_change).await?;
        Ok(Self {
            book_library: lib,
            rebuild_lock: Mutex::new(()),
            root: root.to_path_buf(),
            name,
        })
    }
}

impl Library for LocalLibrary {
    async fn list_books(&self, req: ListBooksRequest) -> ListBooksResponse {
        self.book_library.list_books(req).await
    }

    async fn list_sub_directories(&self, req: ListDirectoriesRequest) -> ListDirectoriesResponse {
        self.book_library.list_sub_directories(req).await
    }

    async fn get_cover(&self, id: &str) -> Result<FileContent, LibraryError> {
        self.book_library.get_cover(id).await
    }

    async fn get_book(&self, id: &str) -> Result<FileContent, LibraryError> {
        self.book_library.get_book(id).await
    }

    async fn get_progress(
        &self,
        req: GetProgressRequest,
    ) -> Result<GetProgressResponse, LibraryError> {
        self.book_library.get_progress(req).await
    }

    async fn set_progress(&self, req: SetProgressRequest) -> Result<(), LibraryError> {
        self.book_library.set_progress(req).await
    }

    async fn get_state(&self, req: GetStateRequest) -> Result<GetStateResponse, LibraryError> {
        self.book_library.get_state(req).await
    }

    async fn set_state(&self, req: SetStateRequest) -> Result<(), LibraryError> {
        self.book_library.set_state(req).await
    }

    async fn get_config(&self) -> Result<Config, LibraryError> {
        self.book_library.get_config().await
    }

    async fn set_config(&self, patch: Config) -> Result<Config, LibraryError> {
        self.book_library.set_config(patch).await
    }

    async fn rebuild_index(&self) -> Result<RebuildIndexResponse, LibraryError> {
        let _guard = self.rebuild_lock.lock().await;
        self.book_library.rebuild_index().await
    }
}

impl AppLibrary for LocalLibrary {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn get_builder(&self) -> LibrarySource {
        LibrarySource::Local(LocalLibraryBuilder::new(&self.root, &self.name))
    }
}
