use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::libsrc::LibrarySource;

/// In the app's private data dir, never in a library.
const STORE_FILE: &str = "app_data.json";
const LIBRARIES_KEY: &str = "libraries";
/// The name of the library the app opens on start.
const CURRENT_KEY: &str = "current";
/// How many log files to keep, the one being written included.
const LOG_FILES_KEY: &str = "log_files";
/// The interface's size, in percent.
const UI_SCALE_KEY: &str = "ui_scale";

pub const DEFAULT_LOG_FILES: u32 = 5;
pub const MAX_LOG_FILES: u32 = 50;

pub const DEFAULT_UI_SCALE: u32 = 100;
pub const MIN_UI_SCALE: u32 = 50;
pub const MAX_UI_SCALE: u32 = 200;
pub const UI_SCALE_STEP: u32 = 10;

#[derive(Deserialize, Serialize)]
pub struct AppData {
    libraries: Vec<LibrarySource>,
    current: Option<String>,
    log_files: u32,
    ui_scale: u32,
    /// What `load` had to fall back on. It runs before logging starts, so the caller logs these.
    #[serde(skip)]
    load_problems: Vec<String>,
}

impl AppData {
    /// Missing data starts empty and unreadable data falls back to defaults, rather than blocking
    /// the app. Runs before logging starts, so problems are kept for `take_load_problems`.
    pub fn load(app: &AppHandle) -> Result<Self> {
        let store = app.store(STORE_FILE).context("couldn't open the app data store")?;
        let mut load_problems = Vec::new();
        let libraries = match store.get(LIBRARIES_KEY) {
            None => Vec::new(),
            Some(json) => serde_json::from_value(json).unwrap_or_else(|e| {
                load_problems.push(format!("unreadable library list, starting empty: {e}"));
                Vec::new()
            }),
        };
        let current = store
            .get(CURRENT_KEY)
            .and_then(|json| serde_json::from_value(json).ok());
        let log_files = match store.get(LOG_FILES_KEY) {
            None => DEFAULT_LOG_FILES,
            Some(json) => serde_json::from_value(json.clone())
                .ok()
                .and_then(|n| validate_log_files(n).ok())
                .unwrap_or_else(|| {
                    load_problems.push(format!(
                        "log file count {json} isn't 1 to {MAX_LOG_FILES}, using {DEFAULT_LOG_FILES}"
                    ));
                    DEFAULT_LOG_FILES
                }),
        };
        let ui_scale = match store.get(UI_SCALE_KEY) {
            None => DEFAULT_UI_SCALE,
            Some(json) => serde_json::from_value(json.clone())
                .ok()
                .and_then(|n| validate_ui_scale(n).ok())
                .unwrap_or_else(|| {
                    load_problems.push(format!(
                        "interface scale {json} isn't valid, using {DEFAULT_UI_SCALE}"
                    ));
                    DEFAULT_UI_SCALE
                }),
        };
        Ok(Self {
            libraries,
            current,
            log_files,
            ui_scale,
            load_problems,
        })
    }

    pub fn take_load_problems(&mut self) -> Vec<String> {
        std::mem::take(&mut self.load_problems)
    }

    pub fn save(&self, app: &AppHandle) -> Result<()> {
        let store = app.store(STORE_FILE).context("couldn't open the app data store")?;
        store.set(LIBRARIES_KEY, serde_json::to_value(&self.libraries)?);
        store.set(CURRENT_KEY, serde_json::to_value(&self.current)?);
        store.set(LOG_FILES_KEY, self.log_files);
        store.set(UI_SCALE_KEY, self.ui_scale);
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

    /// Stores (or with `None` forgets) a remote library's login key.
    pub fn set_remote_token(&mut self, name: &str, token: Option<String>) -> Result<()> {
        match self.libraries.iter_mut().find(|l| l.name() == name) {
            Some(LibrarySource::Remote(builder)) => builder.set_token(token),
            Some(LibrarySource::Local(_)) => bail!("{name} is a local library, with no accounts"),
            None => bail!("there's no library called {name}"),
        }
        Ok(())
    }

    pub fn log_files(&self) -> u32 {
        self.log_files
    }

    pub fn set_log_files(&mut self, n: u32) -> Result<()> {
        self.log_files = validate_log_files(n)?;
        Ok(())
    }

    pub fn ui_scale(&self) -> u32 {
        self.ui_scale
    }

    pub fn set_ui_scale(&mut self, percent: u32) -> Result<()> {
        self.ui_scale = validate_ui_scale(percent)?;
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

pub fn validate_log_files(n: u32) -> Result<u32> {
    ensure!(
        (1..=MAX_LOG_FILES).contains(&n),
        "Keep between 1 and {MAX_LOG_FILES} log files"
    );
    Ok(n)
}

pub fn validate_ui_scale(percent: u32) -> Result<u32> {
    ensure!(
        (MIN_UI_SCALE..=MAX_UI_SCALE).contains(&percent) && percent % UI_SCALE_STEP == 0,
        "Scale must be {MIN_UI_SCALE}–{MAX_UI_SCALE}% in steps of {UI_SCALE_STEP}"
    );
    Ok(percent)
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

    #[test]
    fn log_files_must_be_in_range() {
        assert!(validate_log_files(0).is_err());
        assert_eq!(validate_log_files(1).unwrap(), 1);
        assert_eq!(validate_log_files(MAX_LOG_FILES).unwrap(), MAX_LOG_FILES);
        assert!(validate_log_files(MAX_LOG_FILES + 1).is_err());
    }

    #[test]
    fn ui_scale_must_be_a_step_in_range() {
        assert!(validate_ui_scale(MIN_UI_SCALE - UI_SCALE_STEP).is_err());
        assert_eq!(validate_ui_scale(MIN_UI_SCALE).unwrap(), MIN_UI_SCALE);
        assert_eq!(validate_ui_scale(120).unwrap(), 120);
        assert!(validate_ui_scale(125).is_err());
        assert_eq!(validate_ui_scale(MAX_UI_SCALE).unwrap(), MAX_UI_SCALE);
        assert!(validate_ui_scale(MAX_UI_SCALE + UI_SCALE_STEP).is_err());
    }
}
