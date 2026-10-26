use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tracing::warn;

use crate::libsrc::LibrarySource;

/// In the app's private data dir, never in a library.
const STORE_FILE: &str = "app_data.json";
const LIBRARIES_KEY: &str = "libraries";
/// The name of the library the app opens on start.
const CURRENT_KEY: &str = "current";

#[derive(Default, Deserialize, Serialize)]
pub struct AppData {
    libraries: Vec<LibrarySource>,
    current: Option<String>,
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
        let current = store
            .get(CURRENT_KEY)
            .and_then(|json| serde_json::from_value(json).ok());
        Ok(Self { libraries, current })
    }

    pub fn save(&self, app: &AppHandle) -> Result<()> {
        let store = app.store(STORE_FILE).context("couldn't open the app data store")?;
        store.set(LIBRARIES_KEY, serde_json::to_value(&self.libraries)?);
        store.set(CURRENT_KEY, serde_json::to_value(&self.current)?);
        store.save().context("couldn't save app data")?;
        Ok(())
    }

    pub fn libraries(&self) -> &[LibrarySource] {
        &self.libraries
    }

    pub fn find(&self, name: &str) -> Option<&LibrarySource> {
        self.libraries.iter().find(|l| l.name() == name)
    }

    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn set_current(&mut self, name: &str) -> Result<()> {
        ensure!(self.find(name).is_some(), "there's no library called {name}");
        self.current = Some(name.to_owned());
        Ok(())
    }

    /// Adds the library and makes it the current one.
    pub fn add_library(&mut self, source: LibrarySource) -> Result<()> {
        let name = validate_name(&self.libraries, source.name())?;
        self.libraries.push(source);
        self.current = Some(name);
        Ok(())
    }

    pub fn remove_library(&mut self, name: &str) -> Option<LibrarySource> {
        let index = self.libraries.iter().position(|l| l.name() == name)?;
        if self.current.as_deref() == Some(name) {
            self.current = None;
        }
        Some(self.libraries.remove(index))
    }
}

/// The trimmed name, if it's non-empty and no other library has it (ignoring case).
pub fn validate_name(existing: &[LibrarySource], name: &str) -> Result<String> {
    let name = name.trim();
    ensure!(!name.is_empty(), "Give the library a name");
    if existing
        .iter()
        .any(|l| l.name().to_lowercase() == name.to_lowercase())
    {
        bail!("There's already a library called {name}");
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::libsrc::local::LocalLibraryBuilder;

    fn library(name: &str) -> LibrarySource {
        LibrarySource::Local(LocalLibraryBuilder::new(Path::new("/books"), name))
    }

    #[test]
    fn name_is_trimmed() {
        assert_eq!(validate_name(&[], "  Sci-Fi ").unwrap(), "Sci-Fi");
    }

    #[test]
    fn name_is_required() {
        assert!(validate_name(&[], "").is_err());
        assert!(validate_name(&[], "   ").is_err());
    }

    #[test]
    fn name_is_unique_ignoring_case() {
        let existing = [library("Sci-Fi")];
        assert!(validate_name(&existing, "sci-fi").is_err());
        assert!(validate_name(&existing, " Sci-Fi ").is_err());
        assert_eq!(validate_name(&existing, "Fantasy").unwrap(), "Fantasy");
    }
}
