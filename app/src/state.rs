use std::sync::Arc;

use anyhow::{Context, Result};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tracing::warn;

use crate::{app_data::AppData, libsrc::AnyLibrary};

/// The watcher synced a book; the frontend refetches its listings.
const LIBRARY_CHANGED: &str = "library-changed";

pub struct AppState {
    pub data: Mutex<AppData>,
    library: Mutex<Option<Arc<AnyLibrary>>>,
}

impl AppState {
    pub fn load(app: &AppHandle) -> Result<Self> {
        Ok(Self {
            data: Mutex::new(AppData::load(app)?),
            library: Mutex::new(None),
        })
    }

    /// Opens the library at `index`, replacing the open one. A local library runs its startup
    /// sync here, so this resolves when it's ready to browse.
    pub async fn open(&self, app: &AppHandle, index: usize) -> Result<()> {
        let source = self
            .data
            .lock()
            .await
            .libraries()
            .get(index)
            .cloned()
            .with_context(|| format!("no library at index {index}"))?;
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

    pub async fn library(&self) -> Result<Arc<AnyLibrary>> {
        self.library.lock().await.clone().context("no library is open")
    }
}
