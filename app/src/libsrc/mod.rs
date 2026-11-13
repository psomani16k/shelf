use anyhow::Result;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use shelf_core::{
    library::{Library, LibraryError},
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

use crate::libsrc::{
    app_library::AppLibrary,
    local::{LocalLibrary, LocalLibraryBuilder},
    remote::{RemoteLibrary, RemoteLibraryBuilder},
};

pub mod app_library;
pub mod local;
pub mod remote;

#[derive(Clone, Deserialize, Serialize)]
pub enum LibrarySource {
    Remote(RemoteLibraryBuilder),
    Local(LocalLibraryBuilder),
}

impl LibrarySource {
    pub fn name(&self) -> &str {
        match self {
            Self::Local(builder) => builder.name(),
            Self::Remote(builder) => builder.name(),
        }
    }

    /// A local library has no accounts, so it counts as signed in.
    pub fn signed_in(&self) -> bool {
        match self {
            Self::Local(_) => true,
            Self::Remote(builder) => builder.signed_in(),
        }
    }

    /// `on_change` fires when a local library's watcher syncs a book; a server pushes nothing.
    pub async fn build(self, on_change: impl Fn() + Send + Sync + 'static) -> Result<AnyLibrary> {
        Ok(match self {
            Self::Local(builder) => AnyLibrary::Local(builder.build(on_change).await?),
            Self::Remote(builder) => AnyLibrary::Remote(builder.build()),
        })
    }
}

/// `Library` returns `impl Future`, so it can't be a `dyn` trait object; this enum stands in for one.
pub enum AnyLibrary {
    Local(LocalLibrary),
    Remote(RemoteLibrary),
}

impl AnyLibrary {
    /// `None` for a local library, which has no accounts.
    pub fn remote(&self) -> Option<&RemoteLibrary> {
        match self {
            Self::Local(_) => None,
            Self::Remote(lib) => Some(lib),
        }
    }
}

macro_rules! dispatch {
    ($self:ident, $lib:ident => $call:expr) => {
        match $self {
            AnyLibrary::Local($lib) => $call,
            AnyLibrary::Remote($lib) => $call,
        }
    };
}

impl Library for AnyLibrary {
    async fn list_books(&self, req: ListBooksRequest) -> ListBooksResponse {
        dispatch!(self, lib => lib.list_books(req).await)
    }

    async fn list_sub_directories(&self, req: ListDirectoriesRequest) -> ListDirectoriesResponse {
        dispatch!(self, lib => lib.list_sub_directories(req).await)
    }

    async fn get_cover(&self, id: &str) -> Result<FileContent, LibraryError> {
        dispatch!(self, lib => lib.get_cover(id).await)
    }

    async fn get_book(&self, id: &str) -> Result<FileContent, LibraryError> {
        dispatch!(self, lib => lib.get_book(id).await)
    }

    async fn get_progress(
        &self,
        req: GetProgressRequest,
    ) -> Result<GetProgressResponse, LibraryError> {
        dispatch!(self, lib => lib.get_progress(req).await)
    }

    async fn set_progress(&self, req: SetProgressRequest) -> Result<(), LibraryError> {
        dispatch!(self, lib => lib.set_progress(req).await)
    }

    async fn get_state(&self, req: GetStateRequest) -> Result<GetStateResponse, LibraryError> {
        dispatch!(self, lib => lib.get_state(req).await)
    }

    async fn set_state(&self, req: SetStateRequest) -> Result<(), LibraryError> {
        dispatch!(self, lib => lib.set_state(req).await)
    }

    async fn get_config(&self, req: GetConfigRequest) -> Result<Config, LibraryError> {
        dispatch!(self, lib => lib.get_config(req).await)
    }

    async fn set_config(&self, req: SetConfigRequest) -> Result<Config, LibraryError> {
        dispatch!(self, lib => lib.set_config(req).await)
    }

    async fn rebuild_index(&self) -> Result<RebuildIndexResponse, LibraryError> {
        dispatch!(self, lib => lib.rebuild_index().await)
    }

    async fn clean_up_progress(&self) -> Result<CleanUpProgressResponse, LibraryError> {
        dispatch!(self, lib => lib.clean_up_progress().await)
    }
    async fn upload_book(
        &self,
        req: UploadEbookRequest,
        book: Bytes,
    ) -> Result<UploadEbookResponse, LibraryError> {
        dispatch!(self, lib => lib.upload_book(req, book).await)
    }

    async fn create_directory(
        &self,
        req: CreateDirectoryRequest,
    ) -> Result<CreateDirectoryResponse, LibraryError> {
        dispatch!(self, lib => lib.create_directory(req).await)
    }

    async fn delete_items(&self, req: DeleteItemsRequest) -> Result<DeleteItemsResponse, LibraryError> {
        dispatch!(self, lib => lib.delete_items(req).await)
    }
}

impl AppLibrary for AnyLibrary {
    fn get_name(&self) -> String {
        dispatch!(self, lib => lib.get_name())
    }

    fn get_builder(&self) -> LibrarySource {
        dispatch!(self, lib => lib.get_builder())
    }
}
