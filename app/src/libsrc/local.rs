use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use shelf_core::{
    library::BookLibrary,
    traits::{Library, LibraryError},
};
use shelf_proto::{
    api::{
        GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListDirectoriesRequest, ListDirectoriesResponse,
        RebuildIndexResponse, SetProgressRequest, SetStateRequest,
    },
    config::Config,
};

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
    pub async fn build(
        self,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> Result<LocalLibrary> {
        LocalLibrary::new(Path::new(&self.root), self.name, on_change).await
    }
}

pub struct LocalLibrary {
    lib: BookLibrary,
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
            lib,
            root: root.to_path_buf(),
            name,
        })
    }
}

impl Library for LocalLibrary {
    async fn list_books(&self, req: ListBooksRequest) -> ListBooksResponse {
        self.lib.list_books(req).await
    }

    async fn list_sub_directories(&self, req: ListDirectoriesRequest) -> ListDirectoriesResponse {
        self.lib.list_sub_directories(req).await
    }

    async fn get_cover(&self, id: &str) -> Result<PathBuf, LibraryError> {
        self.lib.get_cover(id).await
    }

    async fn get_book(&self, id: &str) -> Result<PathBuf, LibraryError> {
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
        self.lib.rebuild_index().await
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
