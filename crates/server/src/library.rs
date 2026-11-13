use std::path::PathBuf;

use shelf_core::library::{
    Library, LibraryError, book_library::BookLibrary, scope::Scope, view::LibraryView,
};
use shelf_proto::{
    api::{CleanUpProgressResponse, GetConfigRequest, RebuildIndexResponse, SetConfigRequest},
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
