use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Wry,
    plugin::{Builder, TauriPlugin},
};

use crate::error::CommandError;

/// The platform pieces Rust can't do itself. On Android they're in `ShelfPlugin.kt`; elsewhere
/// the commands answer with what a desktop needs (storage is always readable, no insets).
pub fn init() -> TauriPlugin<Wry> {
    Builder::new("shelf")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                let handle = _api.register_android_plugin("com.psomani16k.shelf", "ShelfPlugin")?;
                _app.manage(Native(handle));
            }
            Ok(())
        })
        .build()
}

#[cfg(target_os = "android")]
struct Native(tauri::plugin::PluginHandle<Wry>);

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Insets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

#[derive(Deserialize)]
struct Granted {
    granted: bool,
}

#[derive(Deserialize)]
struct Picked {
    path: String,
}

#[tauri::command]
pub async fn storage_granted(app: AppHandle) -> Result<bool, CommandError> {
    #[cfg(target_os = "android")]
    {
        return Ok(call::<Granted>(&app, "storageGranted").await?.granted);
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(true)
    }
}

/// Resolves once the user is back from the permission prompt or settings page.
#[tauri::command]
pub async fn request_storage(app: AppHandle) -> Result<bool, CommandError> {
    #[cfg(target_os = "android")]
    {
        return Ok(call::<Granted>(&app, "requestStorage").await?.granted);
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(true)
    }
}

/// Android's folder picker, mapped to a filesystem path. Desktops use the dialog plugin instead.
#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Result<String, CommandError> {
    #[cfg(target_os = "android")]
    {
        return Ok(call::<Picked>(&app, "pickFolder").await?.path);
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err(anyhow::anyhow!("no native folder picker on this platform").into())
    }
}

#[tauri::command]
pub async fn system_insets(app: AppHandle) -> Result<Insets, CommandError> {
    #[cfg(target_os = "android")]
    {
        return call(&app, "systemInsets").await;
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(Insets::default())
    }
}

/// `run_mobile_plugin` blocks until Kotlin resolves, which for a picker is until the user is done.
#[cfg(target_os = "android")]
async fn call<T: serde::de::DeserializeOwned + Send + 'static>(
    app: &AppHandle,
    command: &'static str,
) -> Result<T, CommandError> {
    use tauri::Manager;
    let handle = app.state::<Native>().0.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        handle.run_mobile_plugin::<T>(command, serde_json::json!({}))
    })
    .await
    .map_err(|e| anyhow::anyhow!("{command}: {e}"))?;
    result.map_err(|e| anyhow::anyhow!("{command}: {e}").into())
}
