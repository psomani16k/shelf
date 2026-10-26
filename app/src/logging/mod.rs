mod files;

use std::{io::Write, sync::Arc};

use anyhow::{Context, anyhow};
use serde::Serialize;
use shelf_core::timeutil::Time;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::{FsExt, OpenOptions};
use tracing::{Subscriber, error, info};
use tracing_subscriber::{
    EnvFilter, Layer, fmt, layer::SubscriberExt, registry::LookupSpan, util::SubscriberInitExt,
};

pub use files::LogFiles;

use crate::{app_data::MAX_LOG_FILES, error::CommandError, state::AppState};

/// How much *Copy logs* puts on the clipboard.
const COPY_BYTES: usize = 256 * 1024;

const RELEASE_FILTER: &str = "info";
// Everything below us is chatty at debug; keep theirs at info.
const DEBUG_FILTER: &str = "debug,tauri=info,wry=info,reqwest=info,hyper_util=info,notify=info";

type Result<T> = std::result::Result<T, CommandError>;

/// JSON lines (the server's format) into this run's file in the app log dir, keeping the newest
/// `keep_files` files; plus a console copy: stdout on desktop, logcat on Android. `log` records (tauri, and the frontend through tauri-plugin-log) are turned into
/// `tracing` events by the `LogTracer` that `init` installs.
pub fn init(app: &AppHandle, keep_files: u32) -> anyhow::Result<Arc<LogFiles>> {
    let dir = app.path().app_log_dir().context("no app log dir")?;
    let files = LogFiles::open(&dir, keep_files as usize, Time::now().whole_seconds())
        .with_context(|| format!("couldn't open the log files in {}", dir.display()))?;
    let files = Arc::new(files);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(if cfg!(debug_assertions) {
            DEBUG_FILTER
        } else {
            RELEASE_FILTER
        })
    });
    let json = fmt::layer()
        .json()
        .flatten_event(true)
        .with_current_span(true)
        .with_span_list(false)
        .with_ansi(false)
        .with_writer(files.clone());
    tracing_subscriber::registry()
        .with(filter)
        .with(json)
        .with(console_layer())
        .try_init()
        .context("couldn't install the logger")?;

    log_panics();
    info!(dir = %dir.display(), keep_files = files.keep(), "logging started");
    Ok(files)
}

#[cfg(not(target_os = "android"))]
fn console_layer<S>() -> Option<impl Layer<S>>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    Some(fmt::layer().with_writer(std::io::stdout))
}

#[cfg(target_os = "android")]
fn console_layer<S>() -> Option<impl Layer<S>>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    use paranoid_android::AndroidLogMakeWriter;

    // Logcat records the time and level itself.
    Some(
        fmt::layer()
            .event_format(fmt::format().with_level(false).without_time())
            .with_ansi(false)
            .with_writer(AndroidLogMakeWriter::new("Shelf".to_owned())),
    )
}

/// Logs a panic before the default hook runs, so it ends up in exported logs.
fn log_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        error!(panic = %panic, "panicked");
        default(panic);
    }));
}

#[derive(Serialize)]
pub struct LogSettings {
    keep_files: u32,
    max_keep_files: u32,
}

impl LogSettings {
    fn of(files: &LogFiles) -> Self {
        Self {
            keep_files: files.keep() as u32,
            max_keep_files: MAX_LOG_FILES,
        }
    }
}

#[tauri::command]
pub fn log_settings(files: State<'_, Arc<LogFiles>>) -> LogSettings {
    LogSettings::of(&files)
}

/// Saves the count in the app data and deletes files past it right away.
#[tauri::command]
pub async fn set_log_files(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, Arc<LogFiles>>,
    keep_files: u32,
) -> Result<LogSettings> {
    {
        let mut data = state.data.lock().await;
        data.set_log_files(keep_files).map_err(CommandError::invalid)?;
        data.save(&app)?;
    }

    let files = files.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        files.set_keep(keep_files as usize)?;
        info!(keep_files, "log file count changed");
        anyhow::Ok(LogSettings::of(&files))
    })
    .await
    .context("deleting old log files stopped")?
    .context("couldn't delete old log files")
    .map_err(CommandError::from)
}

#[derive(Serialize)]
pub struct LogFileList {
    /// Where the files are, for finding them on a desktop.
    dir: String,
    /// Newest first.
    files: Vec<LogFileInfo>,
}

#[derive(Serialize)]
pub struct LogFileInfo {
    /// `shelf-<started>.log`.
    name: String,
    bytes: u64,
    /// When the app started this file, RFC 3339.
    started: String,
    /// The file's modification time, RFC 3339.
    last_write: Option<String>,
    /// The file this run is writing to.
    current: bool,
}

#[tauri::command]
pub async fn list_log_files(files: State<'_, Arc<LogFiles>>) -> Result<LogFileList> {
    let files = files.inner().clone();
    let dir = files.dir().display().to_string();
    let list = tauri::async_runtime::spawn_blocking(move || files.list())
        .await
        .context("listing the log files stopped")?
        .context("couldn't list the log files")?;
    Ok(LogFileList {
        dir,
        files: list
            .into_iter()
            .map(|file| LogFileInfo {
                name: file.name,
                bytes: file.bytes,
                started: file.started.to_string(),
                last_write: file.modified.map(|t| Time::from(t).to_string()),
                current: file.current,
            })
            .collect(),
    })
}

/// Puts the most recent lines on the clipboard.
#[tauri::command]
pub async fn copy_logs(app: AppHandle, files: State<'_, Arc<LogFiles>>) -> Result<()> {
    let files = files.inner().clone();
    let tail = tauri::async_runtime::spawn_blocking(move || files.tail(COPY_BYTES))
        .await
        .context("reading the logs stopped")?
        .context("couldn't read the logs")?;
    app.clipboard()
        .write_text(String::from_utf8_lossy(&tail).into_owned())
        .context("couldn't copy to the clipboard")?;
    Ok(())
}

/// Asks where to save, then writes every log file into one. `false` if the user cancelled.
#[tauri::command]
pub async fn save_logs(app: AppHandle, files: State<'_, Arc<LogFiles>>) -> Result<bool> {
    let files = files.inner().clone();
    let name = format!("shelf-logs-{}.log", Time::now());
    save_with_picker(app, &name, move || {
        files.read_all().context("couldn't read the logs")
    })
    .await
}

/// Asks where to save, then copies one log file there. `false` if the user cancelled.
#[tauri::command]
pub async fn save_log_file(
    app: AppHandle,
    files: State<'_, Arc<LogFiles>>,
    name: String,
) -> Result<bool> {
    if !files::is_log_name(&name) {
        return Err(CommandError::invalid(anyhow!("{name} isn't a log file")));
    }
    let files = files.inner().clone();
    let suggested = name.clone();
    save_with_picker(app, &suggested, move || {
        files
            .read(&name)
            .with_context(|| format!("couldn't read {name}"))
    })
    .await
}

/// The system save picker, then `contents` written to the chosen file. Reading happens after
/// the picker closes, so a saved log includes everything up to that moment.
async fn save_with_picker(
    app: AppHandle,
    suggested_name: &str,
    contents: impl FnOnce() -> anyhow::Result<Vec<u8>> + Send + 'static,
) -> Result<bool> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        // Colons aren't allowed in names on Android's shared storage.
        .set_file_name(suggested_name.replace(':', ""))
        .add_filter("Log files", &["log"])
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    let Some(path) = rx.await.context("the save dialog went away")? else {
        return Ok(false);
    };

    tauri::async_runtime::spawn_blocking(move || {
        let contents = contents()?;
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        // Also opens Android's content:// URIs from the system save screen.
        let mut file = app
            .fs()
            .open(path, options)
            .context("couldn't create the file")?;
        file.write_all(&contents)
            .context("couldn't write the file")?;
        anyhow::Ok(())
    })
    .await
    .context("saving the logs stopped")??;
    Ok(true)
}
