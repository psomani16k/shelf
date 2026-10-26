use std::path::Path;

use anyhow::{Context, ensure};
use serde::Serialize;
use shelf_core::traits::Library;
use shelf_proto::{
    api::{
        GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListDirectoriesRequest, ListDirectoriesResponse,
        RebuildIndexResponse, SetProgressRequest, SetStateRequest,
    },
    config::Config,
};
use tauri::{AppHandle, Manager, State, ipc::Response};
use tauri_plugin_http::reqwest::Url;

use crate::{
    app_data::validate_name,
    error::CommandError,
    libsrc::{LibrarySource, local::LocalLibraryBuilder, remote::RemoteLibraryBuilder},
    state::AppState,
};

type Result<T> = std::result::Result<T, CommandError>;

/// What the frontend sees of a stored library.
#[derive(Serialize)]
pub struct LibraryInfo {
    name: String,
    kind: &'static str,
    /// The folder for a local library, the server URL for a remote one.
    location: String,
}

impl From<&LibrarySource> for LibraryInfo {
    fn from(source: &LibrarySource) -> Self {
        match source {
            LibrarySource::Local(b) => Self {
                name: b.name().to_owned(),
                kind: "local",
                location: b.root().to_owned(),
            },
            LibrarySource::Remote(b) => Self {
                name: b.name().to_owned(),
                kind: "remote",
                location: b.base_url().to_owned(),
            },
        }
    }
}

/// The stored libraries and the one the app opens.
#[derive(Serialize)]
pub struct Libraries {
    current: Option<String>,
    libraries: Vec<LibraryInfo>,
}

#[tauri::command]
pub async fn list_libraries(state: State<'_, AppState>) -> Result<Libraries> {
    let data = state.data.lock().await;
    Ok(Libraries {
        current: data.current().map(str::to_owned),
        libraries: data.libraries().iter().map(LibraryInfo::from).collect(),
    })
}

/// Adds a folder on this device as a library and makes it the current one.
#[tauri::command]
pub async fn add_local_library(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    path: String,
) -> Result<LibraryInfo> {
    let name = checked_name(&state, &name).await?;
    let source = local_source(&name, &path)
        .await
        .map_err(CommandError::invalid)?;
    add(&app, &state, source).await
}

async fn local_source(name: &str, path: &str) -> anyhow::Result<LibrarySource> {
    let root = tokio::fs::canonicalize(path)
        .await
        .with_context(|| format!("can't open {path}"))?;
    ensure!(root.is_dir(), "{} is not a folder", root.display());
    Ok(LibrarySource::Local(LocalLibraryBuilder::new(&root, name)))
}

/// Adds a Shelf server as a library and makes it the current one.
#[tauri::command]
pub async fn add_remote_library(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    base_url: String,
) -> Result<LibraryInfo> {
    let name = checked_name(&state, &name).await?;
    let source = remote_source(&app, &name, &base_url).map_err(CommandError::invalid)?;
    add(&app, &state, source).await
}

fn remote_source(app: &AppHandle, name: &str, base_url: &str) -> anyhow::Result<LibrarySource> {
    // A bare `nas:8080` would parse as the scheme `nas`.
    let base_url = if base_url.contains("://") {
        base_url.to_owned()
    } else {
        format!("http://{base_url}")
    };
    let url = Url::parse(&base_url).with_context(|| format!("{base_url} isn't a valid URL"))?;
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "{base_url}: only http and https servers are supported"
    );
    let host = url.host_str().context("the URL has no host")?;
    let port = url.port_or_known_default().unwrap_or(80);
    // Shared by every library on this server: files are cached by content hash.
    let cache_dir = app
        .path()
        .app_cache_dir()
        .context("no app cache dir")?
        .join("remote")
        .join(format!("{host}-{port}"));
    Ok(LibrarySource::Remote(RemoteLibraryBuilder::new(
        &base_url, &cache_dir, name,
    )))
}

/// Checked before the source, so a bad name is reported before a bad path or URL.
async fn checked_name(state: &AppState, name: &str) -> Result<String> {
    validate_name(state.data.lock().await.libraries(), name).map_err(CommandError::invalid)
}

async fn add(app: &AppHandle, state: &AppState, source: LibrarySource) -> Result<LibraryInfo> {
    let info = LibraryInfo::from(&source);
    let mut data = state.data.lock().await;
    data.add_library(source)?;
    data.save(app)?;
    Ok(info)
}

/// Makes `name` the library the app opens, now and on the next start. Doesn't open it.
#[tauri::command]
pub async fn select_library(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<()> {
    let mut data = state.data.lock().await;
    data.set_current(&name)?;
    data.save(&app)?;
    Ok(())
}

/// Forgets the library on this device; its folder and `.shelf/` data are untouched.
/// Closes it first if it's the open one.
#[tauri::command]
pub async fn remove_library(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<()> {
    {
        let mut data = state.data.lock().await;
        data.remove_library(&name)
            .with_context(|| format!("there's no library called {name}"))?;
        data.save(&app)?;
    }
    if state.open_name().await.as_deref() == Some(name.as_str()) {
        state.close().await;
    }
    Ok(())
}

/// Resolves once the library is ready to browse; a local one syncs `.shelf/` first.
#[tauri::command]
pub async fn open_library(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<()> {
    Ok(state.open(&app, &name).await?)
}

#[tauri::command]
pub async fn list_books(
    state: State<'_, AppState>,
    req: ListBooksRequest,
) -> Result<ListBooksResponse> {
    Ok(state.library().await?.list_books(req).await)
}

#[tauri::command]
pub async fn list_directories(
    state: State<'_, AppState>,
    req: ListDirectoriesRequest,
) -> Result<ListDirectoriesResponse> {
    Ok(state.library().await?.list_sub_directories(req).await)
}

#[tauri::command]
pub async fn get_progress(
    state: State<'_, AppState>,
    req: GetProgressRequest,
) -> Result<GetProgressResponse> {
    Ok(state.library().await?.get_progress(req).await?)
}

#[tauri::command]
pub async fn set_progress(state: State<'_, AppState>, req: SetProgressRequest) -> Result<()> {
    Ok(state.library().await?.set_progress(req).await?)
}

#[tauri::command]
pub async fn get_state(
    state: State<'_, AppState>,
    req: GetStateRequest,
) -> Result<GetStateResponse> {
    Ok(state.library().await?.get_state(req).await?)
}

#[tauri::command]
pub async fn set_state(state: State<'_, AppState>, req: SetStateRequest) -> Result<()> {
    Ok(state.library().await?.set_state(req).await?)
}

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<Config> {
    Ok(state.library().await?.get_config().await?)
}

#[tauri::command]
pub async fn set_config(state: State<'_, AppState>, patch: Config) -> Result<Config> {
    Ok(state.library().await?.set_config(patch).await?)
}

/// Takes as long as hashing every book: for a local library, every file is read.
#[tauri::command]
pub async fn rebuild_index(state: State<'_, AppState>) -> Result<RebuildIndexResponse> {
    Ok(state.library().await?.rebuild_index().await?)
}

/// The thumbnail JPEG as raw bytes. Not a custom URI scheme: WebKitGTK won't load one from the
/// `http://` dev server page, so `<img src>` never worked in `cargo tauri dev` on Linux.
#[tauri::command]
pub async fn get_cover(state: State<'_, AppState>, id: String) -> Result<Response> {
    let path = state.library().await?.get_cover(&id).await?;
    raw_file(&path).await
}

/// The EPUB as raw bytes.
#[tauri::command]
pub async fn get_book_file(state: State<'_, AppState>, id: String) -> Result<Response> {
    let path = state.library().await?.get_book(&id).await?;
    raw_file(&path).await
}

/// `invoke` hands raw bytes to JS as an ArrayBuffer, not a JSON array of numbers.
async fn raw_file(path: &Path) -> Result<Response> {
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("couldn't read {}", path.display()))?;
    Ok(Response::new(bytes))
}
