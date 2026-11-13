use std::path::{Path, PathBuf};

use anyhow::Result;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use shelf_core::{
    library::{Library, LibraryError, book_library::BookLibrary, user::DEFAULT_USER},
    storage::FileContent,
};
use shelf_proto::{
    api::{
        CreateDirectoryRequest, CreateDirectoryResponse, DeleteItemsRequest, DeleteItemsResponse,
        GetConfigRequest, GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListDirectoriesRequest, ListDirectoriesResponse,
        CleanUpProgressResponse, RebuildIndexResponse, SetConfigRequest, SetProgressRequest, SetStateRequest,
        UploadEbookRequest, UploadEbookResponse,
    },
    config::Config,
};
use tokio::sync::Mutex;

use crate::libsrc::{LibrarySource, app_library::AppLibrary};

#[derive(Clone, Deserialize, Serialize)]
pub struct LocalLibraryBuilder {
    root: String,
    name: String,
    /// Whose progress this device reads and writes in the library.
    #[serde(default = "default_user")]
    user: String,
}

fn default_user() -> String {
    DEFAULT_USER.to_owned()
}

impl LocalLibraryBuilder {
    pub fn new(root: &Path, name: &str) -> Self {
        Self {
            root: root.to_string_lossy().to_string(),
            name: name.to_string(),
            user: default_user(),
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
        LocalLibrary::new(Path::new(&self.root), self.name, self.user, on_change).await
    }
}

pub struct LocalLibrary {
    book_library: BookLibrary,
    // A second rebuild waits for the first instead of hashing everything twice at once.
    rebuild_lock: Mutex<()>,
    root: PathBuf,
    name: String,
    user: String,
}

impl LocalLibrary {
    /// `on_change` runs after the watcher syncs a book, so the UI can refresh its listing.
    pub async fn new(
        root: &Path,
        name: String,
        user: String,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self> {
        let lib = BookLibrary::open(root.to_path_buf(), on_change).await?;
        Ok(Self {
            book_library: lib,
            rebuild_lock: Mutex::new(()),
            root: root.to_path_buf(),
            name,
            user,
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

    // The user in progress, state and config requests comes from the app data, never the frontend.
    async fn get_progress(
        &self,
        mut req: GetProgressRequest,
    ) -> Result<GetProgressResponse, LibraryError> {
        req.user = self.user.clone();
        self.book_library.get_progress(req).await
    }

    async fn set_progress(&self, mut req: SetProgressRequest) -> Result<(), LibraryError> {
        req.user = self.user.clone();
        self.book_library.set_progress(req).await
    }

    async fn get_state(&self, mut req: GetStateRequest) -> Result<GetStateResponse, LibraryError> {
        req.user = self.user.clone();
        self.book_library.get_state(req).await
    }

    async fn set_state(&self, mut req: SetStateRequest) -> Result<(), LibraryError> {
        req.user = self.user.clone();
        self.book_library.set_state(req).await
    }

    async fn get_config(&self, mut req: GetConfigRequest) -> Result<Config, LibraryError> {
        req.user = self.user.clone();
        self.book_library.get_config(req).await
    }

    async fn set_config(&self, mut req: SetConfigRequest) -> Result<Config, LibraryError> {
        req.user = self.user.clone();
        self.book_library.set_config(req).await
    }

    async fn rebuild_index(&self) -> Result<RebuildIndexResponse, LibraryError> {
        let _guard = self.rebuild_lock.lock().await;
        self.book_library.rebuild_index().await
    }

    async fn clean_up_progress(&self) -> Result<CleanUpProgressResponse, LibraryError> {
        self.book_library.clean_up_progress().await
    }
    async fn upload_book(
        &self,
        req: UploadEbookRequest,
        book: Bytes,
    ) -> Result<UploadEbookResponse, LibraryError> {
        self.book_library.upload_book(req, book).await
    }

    async fn create_directory(
        &self,
        req: CreateDirectoryRequest,
    ) -> Result<CreateDirectoryResponse, LibraryError> {
        self.book_library.create_directory(req).await
    }

    async fn delete_items(&self, req: DeleteItemsRequest) -> Result<DeleteItemsResponse, LibraryError> {
        self.book_library.delete_items(req).await
    }
}

impl AppLibrary for LocalLibrary {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn get_builder(&self) -> LibrarySource {
        LibrarySource::Local(LocalLibraryBuilder {
            root: self.root.to_string_lossy().to_string(),
            name: self.name.clone(),
            user: self.user.clone(),
        })
    }
}
