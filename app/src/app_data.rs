use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tracing::warn;

use crate::libsrc::LibrarySource;

/// In the app's private data dir, never in a library.
const STORE_FILE: &str = "app_data.json";
const LIBRARIES_KEY: &str = "libraries";

#[derive(Default, Deserialize, Serialize)]
pub struct AppData {
    libraries: Vec<LibrarySource>,
}

impl AppData {
    /// Missing data starts empty; unreadable data does too, with a warning, rather than blocking the app.
    pub fn load(app: &AppHandle) -> Result<Self> {
        let store = app.store(STORE_FILE).context("couldn't open the app data store")?;
        let libraries = match store.get(LIBRARIES_KEY) {
            None => Vec::new(),
            Some(json) => serde_json::from_value(json).unwrap_or_else(|e| {
                warn!(error = %e, "unreadable library list, starting empty");
                Vec::new()
            }),
        };
        Ok(Self { libraries })
    }

    pub fn save(&self, app: &AppHandle) -> Result<()> {
        let store = app.store(STORE_FILE).context("couldn't open the app data store")?;
        store.set(LIBRARIES_KEY, serde_json::to_value(&self.libraries)?);
        store.save().context("couldn't save app data")?;
        Ok(())
    }

    pub fn libraries(&self) -> &[LibrarySource] {
        &self.libraries
    }

    pub fn add_library(&mut self, source: LibrarySource) {
        self.libraries.push(source);
    }

    pub fn remove_library(&mut self, index: usize) -> Option<LibrarySource> {
        (index < self.libraries.len()).then(|| self.libraries.remove(index))
    }
}
