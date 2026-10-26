use std::sync::Arc;

use anyhow::{Context, Result};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tracing::warn;

use crate::{
    app_data::AppData,
    libsrc::{AnyLibrary, app_library::AppLibrary},
};

/// The watcher synced a book; the frontend refetches its listings.
const LIBRARY_CHANGED: &str = "library-changed";

pub struct AppState {
    pub data: Mutex<AppData>,
    library: Mutex<Option<Arc<AnyLibrary>>>,
}

impl AppState {
    pub fn new(data: AppData) -> Self {
        Self {
            data: Mutex::new(data),
            library: Mutex::new(None),
        }
    }

    /// Opens the library called `name`, replacing the open one. A local library runs its startup
    /// sync here, so this resolves when it's ready to browse.
    pub async fn open(&self, app: &AppHandle, name: &str) -> Result<()> {
        let source = self
            .data
            .lock()
            .await
            .find(name)
            .cloned()
            .with_context(|| format!("there's no library called {name}"))?;
        let app = app.clone();
        let library = source
            .build(move || {
                if let Err(e) = app.emit(LIBRARY_CHANGED, ()) {
                    warn!(error = %e, "couldn't send library-changed");
                }
            })
            .await?;
        // Dropping the previous library stops its watcher.
        *self.library.lock().await = Some(Arc::new(library));
        Ok(())
    }

    pub async fn close(&self) {
        *self.library.lock().await = None;
    }

    pub async fn open_name(&self) -> Option<String> {
        self.library.lock().await.as_ref().map(|l| l.get_name())
    }

    pub async fn library(&self) -> Result<Arc<AnyLibrary>> {
        self.library.lock().await.clone().context("no library is open")
    }
}
