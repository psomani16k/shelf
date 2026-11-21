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

#[cfg(target_os = "android")]
#[derive(Deserialize)]
struct Named {
    name: String,
}

/// What this device calls itself, sent at login so the user can tell their sessions apart:
/// Android's device name ("OnePlus 11 5G"), elsewhere `Shelf on <hostname>`. "" if unknown.
pub async fn device_name(app: &AppHandle) -> String {
    #[cfg(target_os = "android")]
    let name = call::<Named>(app, "deviceName").await.map(|n| n.name);
    #[cfg(not(target_os = "android"))]
    let name = {
        let _ = app;
        tokio::fs::read_to_string("/proc/sys/kernel/hostname")
            .await
            .map(|host| format!("Shelf on {}", host.trim()))
    };
    // Only the error: the name itself is never logged.
    name.unwrap_or_else(|e| {
        tracing::warn!(error = ?e, "couldn't read the device name");
        String::new()
    })
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

/// Whether Android's floating Copy/Share bar shows for a text selection. The reader hides it while
/// the book has the screen, so only its own toolbar shows; elsewhere there's no such bar.
#[tauri::command]
pub async fn set_selection_menu(app: AppHandle, shown: bool) -> Result<(), CommandError> {
    #[cfg(target_os = "android")]
    {
        call_with::<serde::de::IgnoredAny>(&app, "setSelectionMenu", serde_json::json!({ "shown": shown }))
            .await?;
    }
    #[cfg(not(target_os = "android"))]
    let _ = (app, shown);
    Ok(())
}

/// While reading: Android's `FLAG_KEEP_SCREEN_ON`; on Linux, GTK's idle inhibit (the session
/// manager's, so the screen neither blanks nor locks).
#[tauri::command]
pub async fn set_keep_screen_on(app: AppHandle, on: bool) -> Result<(), CommandError> {
    #[cfg(target_os = "android")]
    {
        call_with::<serde::de::IgnoredAny>(&app, "setKeepScreenOn", serde_json::json!({ "on": on }))
            .await?;
    }
    #[cfg(target_os = "linux")]
    inhibit_idle(&app, on)?;
    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    let _ = (app, on);
    Ok(())
}

#[cfg(target_os = "linux")]
fn inhibit_idle(app: &AppHandle, on: bool) -> anyhow::Result<()> {
    use anyhow::Context;
    use tauri::Manager;
    let window = app.webview_windows().into_values().next().context("no window")?;
    // GTK is main-thread only, and `with_webview` runs there.
    window.with_webview(move |webview| {
        use gtk::prelude::*;
        use std::cell::Cell;
        thread_local! { static COOKIE: Cell<Option<u32>> = const { Cell::new(None) }; }
        let toplevel = webview.inner().toplevel();
        let Some(gtk_window) = toplevel.and_then(|w| w.downcast::<gtk::Window>().ok()) else {
            tracing::warn!("no GTK window to keep the screen on for");
            return;
        };
        let Some(gtk_app) = gtk_window.application() else {
            tracing::warn!("the window has no GTK application to inhibit idle with");
            return;
        };
        COOKIE.with(|cookie| match (on, cookie.get()) {
            (true, None) => {
                let flags = gtk::ApplicationInhibitFlags::IDLE;
                let id = gtk_app.inhibit(Some(&gtk_window), flags, Some("Reading"));
                if id == 0 {
                    tracing::warn!("the session refused to keep the screen on");
                } else {
                    cookie.set(Some(id));
                }
            }
            (false, Some(id)) => {
                gtk_app.uninhibit(id);
                cookie.set(None);
            }
            _ => {}
        });
    })?;
    Ok(())
}

#[cfg(target_os = "android")]
async fn call<T: serde::de::DeserializeOwned + Send + 'static>(
    app: &AppHandle,
    command: &'static str,
) -> Result<T, CommandError> {
    call_with(app, command, serde_json::json!({})).await
}

/// `run_mobile_plugin` blocks until Kotlin resolves, which for a picker is until the user is done.
#[cfg(target_os = "android")]
async fn call_with<T: serde::de::DeserializeOwned + Send + 'static>(
    app: &AppHandle,
    command: &'static str,
    args: serde_json::Value,
) -> Result<T, CommandError> {
    use tauri::Manager;
    let handle = app.state::<Native>().0.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        handle.run_mobile_plugin::<T>(command, args)
    })
    .await
    .map_err(|e| anyhow::anyhow!("{command}: {e}"))?;
    result.map_err(|e| anyhow::anyhow!("{command}: {e}").into())
}
