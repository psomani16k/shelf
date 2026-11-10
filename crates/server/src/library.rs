use std::path::PathBuf;

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

pub struct ServerLibrary {
    lib: BookLibrary,
    // A second rebuild waits for the first instead of hashing everything twice at once.
    rebuild_lock: Mutex<()>,
}

impl ServerLibrary {
    pub async fn open(
        root: PathBuf,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> anyhow::Result<Self> {
        let lib = BookLibrary::open(root, on_change).await?;
        Ok(Self {
            lib,
            rebuild_lock: Mutex::new(()),
        })
    }
}

impl Library for ServerLibrary {
    async fn list_books(&self, req: ListBooksRequest) -> ListBooksResponse {
        self.lib.list_books(req).await
    }

    async fn list_sub_directories(&self, req: ListDirectoriesRequest) -> ListDirectoriesResponse {
        self.lib.list_sub_directories(req).await
    }

    async fn get_cover(&self, id: &str) -> Result<FileContent, LibraryError> {
        self.lib.get_cover(id).await
    }

    async fn get_book(&self, id: &str) -> Result<FileContent, LibraryError> {
        self.lib.get_book(id).await
    }

    async fn get_progress(
        &self,
        req: GetProgressRequest,
    ) -> Result<GetProgressResponse, LibraryError> {
        self.lib.get_progress(req).await
    }

    async fn set_progress(&self, req: SetProgressRequest) -> Result<(), LibraryError> {
        self.lib.set_progress(req).await
    }

    async fn get_state(&self, req: GetStateRequest) -> Result<GetStateResponse, LibraryError> {
        self.lib.get_state(req).await
    }

    async fn set_state(&self, req: SetStateRequest) -> Result<(), LibraryError> {
        self.lib.set_state(req).await
    }

    async fn get_config(&self) -> Result<Config, LibraryError> {
        self.lib.get_config().await
    }

    async fn set_config(&self, patch: Config) -> Result<Config, LibraryError> {
        self.lib.set_config(patch).await
    }

    async fn rebuild_index(&self) -> Result<RebuildIndexResponse, LibraryError> {
        let _guard = self.rebuild_lock.lock().await;
        self.lib.rebuild_index().await
    }
}
