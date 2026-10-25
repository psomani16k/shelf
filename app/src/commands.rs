use std::path::Path;

use anyhow::{Context, ensure};
use serde::Serialize;
use shelf_core::traits::Library;
use shelf_proto::{
    api::{
        GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListDirectoriesRequest, ListDirectoriesResponse,
        SetProgressRequest, SetStateRequest,
    },
    config::Config,
};
use tauri::{AppHandle, Manager, State, ipc::Response};
use tauri_plugin_http::reqwest::Url;

use crate::{
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

#[tauri::command]
pub async fn list_libraries(state: State<'_, AppState>) -> Result<Vec<LibraryInfo>> {
    let data = state.data.lock().await;
    Ok(data.libraries().iter().map(LibraryInfo::from).collect())
}

#[tauri::command]
pub async fn add_local_library(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<LibraryInfo> {
    let source = local_source(&path).await?;
    add(&app, &state, source).await
}

async fn local_source(path: &str) -> anyhow::Result<LibrarySource> {
    let root = tokio::fs::canonicalize(path)
        .await
        .with_context(|| format!("can't open {path}"))?;
    ensure!(root.is_dir(), "{} is not a folder", root.display());
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string());
    Ok(LibrarySource::Local(LocalLibraryBuilder::new(&root, &name)))
}

#[tauri::command]
pub async fn add_remote_library(
    app: AppHandle,
    state: State<'_, AppState>,
    base_url: String,
) -> Result<LibraryInfo> {
    let source = remote_source(&app, &base_url)?;
    add(&app, &state, source).await
}

fn remote_source(app: &AppHandle, base_url: &str) -> anyhow::Result<LibrarySource> {
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
    let host = url.host_str().context("the URL has no host")?.to_owned();
    let port = url.port_or_known_default().unwrap_or(80);
    let cache_dir = app
        .path()
        .app_cache_dir()
        .context("no app cache dir")?
        .join("remote")
        .join(format!("{host}-{port}"));
    Ok(LibrarySource::Remote(RemoteLibraryBuilder::new(
        &base_url, &cache_dir, &host,
    )))
}

async fn add(app: &AppHandle, state: &AppState, source: LibrarySource) -> Result<LibraryInfo> {
    let info = LibraryInfo::from(&source);
    let mut data = state.data.lock().await;
    data.add_library(source);
    data.save(app)?;
    Ok(info)
}

/// Also closes the open library: with one library stored, it's always the one being removed.
#[tauri::command]
pub async fn remove_library(
    app: AppHandle,
    state: State<'_, AppState>,
    index: usize,
) -> Result<()> {
    {
        let mut data = state.data.lock().await;
        data.remove_library(index)
            .with_context(|| format!("no library at index {index}"))?;
        data.save(&app)?;
    }
    state.close().await;
    Ok(())
}

/// Resolves once the library is ready to browse; a local one syncs `.shelf/` first.
#[tauri::command]
pub async fn open_library(
    app: AppHandle,
    state: State<'_, AppState>,
    index: usize,
) -> Result<()> {
    Ok(state.open(&app, index).await?)
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
