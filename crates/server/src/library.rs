use std::path::PathBuf;

use shelf_core::library::{
    Library, LibraryError, book_library::BookLibrary, scope::Scope, view::LibraryView,
};
use shelf_proto::{
    api::{
        CleanUpProgressResponse, DeleteThemeRequest, GetConfigRequest, ListThemesRequest,
        ListThemesResponse, RebuildIndexResponse, RenameThemeRequest, SetConfigRequest,
        SetThemeRequest,
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
    pub async fn open(root: PathBuf) -> anyhow::Result<Self> {
        // Nothing to tell: web clients refetch on their own.
        let lib = BookLibrary::open(root, || {}).await?;
        Ok(Self {
            lib,
            rebuild_lock: Mutex::new(()),
        })
    }

    /// One user's part of the library. Everything that touches books goes through it.
    pub fn view<'a>(&'a self, scope: &'a Scope) -> LibraryView<'a> {
        self.lib.view(scope)
    }

    pub async fn get_config(&self, req: GetConfigRequest) -> Result<Config, LibraryError> {
        self.lib.get_config(req).await
    }

    pub async fn set_config(&self, req: SetConfigRequest) -> Result<Config, LibraryError> {
        self.lib.set_config(req).await
    }

    pub async fn list_themes(&self, req: ListThemesRequest) -> Result<ListThemesResponse, LibraryError> {
        self.lib.list_themes(req).await
    }

    pub async fn set_theme(&self, req: SetThemeRequest) -> Result<ListThemesResponse, LibraryError> {
        self.lib.set_theme(req).await
    }

    pub async fn delete_theme(&self, req: DeleteThemeRequest) -> Result<ListThemesResponse, LibraryError> {
        self.lib.delete_theme(req).await
    }

    pub async fn rename_theme(&self, req: RenameThemeRequest) -> Result<ListThemesResponse, LibraryError> {
        self.lib.rename_theme(req).await
    }

    /// The whole library, whoever asks.
    pub async fn rebuild_index(&self) -> Result<RebuildIndexResponse, LibraryError> {
        let _guard = self.rebuild_lock.lock().await;
        self.lib.rebuild_index().await
    }

    /// The whole library, whoever asks.
    pub async fn clean_up_progress(&self) -> Result<CleanUpProgressResponse, LibraryError> {
        self.lib.clean_up_progress().await
    }
}
