use std::path::{Path, PathBuf};

use anyhow::{Context, ensure};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use bytes::Bytes;
use serde::Serialize;
use shelf_core::{
    library::{Library, LibraryError},
    storage::FileContent,
};
use shelf_proto::{
    api::{
        ApproveUserRequestRequest, ChangePasswordRequest, DenyUserRequestRequest,
        ListUserRequestsResponse, ListUsersResponse, SetUserRoleRequest, SetUserScopeRequest, SignUpRequest, CreateDirectoryRequest, CreateDirectoryResponse, DeleteItemsRequest, DeleteItemsResponse,
        GetConfigRequest, GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListDirectoriesRequest, ListDirectoriesResponse,
        CleanUpProgressResponse, RebuildIndexResponse, SetConfigRequest, SetProgressRequest, SetStateRequest,
        LoginRequest, UploadEbookRequest, UploadEbookResponse, User, ExportUserDataRequest,
        ImportUserDataRequest, ImportUserDataResponse, UserData, role::Value as Role,
    },
    config::Config,
};
use tauri::{AppHandle, Manager, Runtime, State, ipc::Response};
use tokio::{fs, io::AsyncWriteExt};
use tauri_plugin_http::reqwest::Url;

use crate::{
    app_data::validate_name,
    error::CommandError,
    logging::save_with_picker,
    libsrc::{
        AnyLibrary, LibrarySource, app_library::AppLibrary, local::LocalLibraryBuilder,
        remote::{RemoteLibrary, RemoteLibraryBuilder},
    },
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
    /// Whether a remote library has a login key, or a local one has a chosen user.
    signed_in: bool,
    /// Who reads a local library on this device; never set for a remote one.
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<String>,
}

impl From<&LibrarySource> for LibraryInfo {
    fn from(source: &LibrarySource) -> Self {
        let signed_in = source.signed_in();
        match source {
            LibrarySource::Local(b) => Self {
                name: b.name().to_owned(),
                kind: "local",
                location: b.root().to_owned(),
                signed_in,
                user: b.user().map(str::to_owned),
            },
            LibrarySource::Remote(b) => Self {
                name: b.name().to_owned(),
                kind: "remote",
                location: b.base_url().to_owned(),
                signed_in,
                user: None,
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

/// The interface's size on this device, in percent.
#[tauri::command]
pub async fn ui_scale(state: State<'_, AppState>) -> Result<u32> {
    Ok(state.data.lock().await.ui_scale())
}

/// 50 to 200 in steps of 10, else a 400.
#[tauri::command]
pub async fn set_ui_scale(
    app: AppHandle,
    state: State<'_, AppState>,
    percent: u32,
) -> Result<u32> {
    let mut data = state.data.lock().await;
    data.set_ui_scale(percent).map_err(CommandError::invalid)?;
    data.save(&app)?;
    tracing::info!(percent, "interface scale changed");
    Ok(percent)
}

/// Forgets the library on this device; its folder and `.shelf/` data are untouched.
/// Closes it first if it's the open one.
#[tauri::command]
pub async fn remove_library(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<()> {
    let removed = {
        let mut data = state.data.lock().await;
        let removed = data
            .remove_library(&name)
            .with_context(|| format!("there's no library called {name}"))?;
        data.save(&app)?;
        removed
    };
    if state.open_name().await.as_deref() == Some(name.as_str()) {
        state.close().await;
    }
    // Ends the session on the server too, without making the user wait for it.
    if let LibrarySource::Remote(builder) = removed
        && builder.signed_in()
    {
        tauri::async_runtime::spawn(async move {
            if let Err(e) = builder.build().logout().await {
                tracing::warn!(error = ?e, "couldn't sign out of a removed library");
            }
        });
    }
    Ok(())
}

/// The open library as a remote one; a local library has no accounts.
fn remote(library: &AnyLibrary) -> Result<&RemoteLibrary> {
    library.remote().ok_or_else(|| {
        LibraryError::Invalid("a local library has no accounts".to_owned()).into()
    })
}

/// Signs in to the open remote library and keeps the key, so this device stays signed in. A local
/// library takes the username alone, with no password, and reads as that user from now on.
#[tauri::command]
pub async fn login(app: AppHandle, state: State<'_, AppState>, req: LoginRequest) -> Result<User> {
    let library = state.library().await?;
    if let Some(local) = library.local() {
        local.set_user(Some(req.username.clone())).await?;
        let mut data = state.data.lock().await;
        data.set_local_user(&library.get_name(), Some(req.username.clone()))?;
        data.save(&app)?;
        tracing::info!("local library user chosen");
        return Ok(User {
            username: req.username,
            role: Role::Owner as i32,
        });
    }
    let res = remote(&library)?.login(&req).await?;
    let mut data = state.data.lock().await;
    data.set_remote_token(&library.get_name(), Some(res.token))?;
    data.save(&app)?;
    Ok(res.user.unwrap_or_default())
}

/// Forgets the key on this device even if the server can't be reached. For a local library, forgets
/// who's reading, so the next open asks again.
#[tauri::command]
pub async fn logout(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let library = state.library().await?;
    if let Some(local) = library.local() {
        local.set_user(None).await?;
        let mut data = state.data.lock().await;
        data.set_local_user(&library.get_name(), None)?;
        data.save(&app)?;
        return Ok(());
    }
    if let Err(e) = remote(&library)?.logout().await {
        tracing::warn!(error = ?e, "couldn't sign out on the server");
    }
    let mut data = state.data.lock().await;
    data.set_remote_token(&library.get_name(), None)?;
    data.save(&app)?;
    Ok(())
}

/// The users with data in the open local library, to choose who's reading.
#[tauri::command]
pub async fn list_local_users(state: State<'_, AppState>) -> Result<Vec<String>> {
    let library = state.library().await?;
    let local = library.local().ok_or_else(|| {
        CommandError::from(LibraryError::Invalid("this isn't a local library".to_owned()))
    })?;
    Ok(local.user_names().await?)
}

/// The signed-in user; `None` for a local library, which has no accounts or roles.
#[tauri::command]
pub async fn me(state: State<'_, AppState>) -> Result<Option<User>> {
    let library = state.library().await?;
    match library.remote() {
        Some(remote) => Ok(Some(remote.me().await?)),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn change_password(state: State<'_, AppState>, req: ChangePasswordRequest) -> Result<()> {
    let library = state.library().await?;
    Ok(remote(&library)?.change_password(&req).await?)
}

/// Asks the open remote library's admins for an account; needs no sign-in.
#[tauri::command]
pub async fn sign_up(state: State<'_, AppState>, req: SignUpRequest) -> Result<()> {
    let library = state.library().await?;
    Ok(remote(&library)?.sign_up(&req).await?)
}

#[tauri::command]
pub async fn list_user_requests(state: State<'_, AppState>) -> Result<ListUserRequestsResponse> {
    let library = state.library().await?;
    Ok(remote(&library)?.list_user_requests().await?)
}

#[tauri::command]
pub async fn approve_user_request(
    state: State<'_, AppState>,
    req: ApproveUserRequestRequest,
) -> Result<()> {
    let library = state.library().await?;
    Ok(remote(&library)?.approve_user_request(&req).await?)
}

#[tauri::command]
pub async fn deny_user_request(state: State<'_, AppState>, req: DenyUserRequestRequest) -> Result<()> {
    let library = state.library().await?;
    Ok(remote(&library)?.deny_user_request(&req).await?)
}

#[tauri::command]
pub async fn list_users(state: State<'_, AppState>) -> Result<ListUsersResponse> {
    let library = state.library().await?;
    Ok(remote(&library)?.list_users().await?)
}

#[tauri::command]
pub async fn set_user_role(state: State<'_, AppState>, req: SetUserRoleRequest) -> Result<()> {
    let library = state.library().await?;
    Ok(remote(&library)?.set_user_role(&req).await?)
}

#[tauri::command]
pub async fn set_user_scope(state: State<'_, AppState>, req: SetUserScopeRequest) -> Result<()> {
    let library = state.library().await?;
    Ok(remote(&library)?.set_user_scope(&req).await?)
}

#[tauri::command]
pub async fn list_scope_directories(
    state: State<'_, AppState>,
    req: ListDirectoriesRequest,
) -> Result<ListDirectoriesResponse> {
    let library = state.library().await?;
    Ok(remote(&library)?.list_scope_directories(&req).await?)
}

/// Resolves once the library is ready to browse; a local one syncs `.shelf/` first.
#[tauri::command]
pub async fn open_library(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<()> {
    Ok(state.open(&app, &name).await?)
}

#[tauri::command]
pub async fn list_books(
    state: State<'_, AppState>,
    req: ListBooksRequest,
) -> Result<ListBooksResponse> {
    Ok(state.library().await?.list_books(req).await?)
}

#[tauri::command]
pub async fn list_directories(
    state: State<'_, AppState>,
    req: ListDirectoriesRequest,
) -> Result<ListDirectoriesResponse> {
    Ok(state.library().await?.list_sub_directories(req).await?)
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
pub async fn get_config(state: State<'_, AppState>, req: GetConfigRequest) -> Result<Config> {
    Ok(state.library().await?.get_config(req).await?)
}

#[tauri::command]
pub async fn set_config(state: State<'_, AppState>, req: SetConfigRequest) -> Result<Config> {
    Ok(state.library().await?.set_config(req).await?)
}

/// The signed-in user's data (a local library's chosen reader), without their name.
#[tauri::command]
pub async fn export_user_data(
    state: State<'_, AppState>,
    req: ExportUserDataRequest,
) -> Result<UserData> {
    Ok(state.library().await?.export_user_data(req).await?)
}

#[tauri::command]
pub async fn import_user_data(
    state: State<'_, AppState>,
    req: ImportUserDataRequest,
) -> Result<ImportUserDataResponse> {
    Ok(state.library().await?.import_user_data(req).await?)
}

/// Asks where to save, then writes `contents` there. `false` if the user cancelled.
#[tauri::command]
pub async fn save_json_file(app: AppHandle, name: String, contents: String) -> Result<bool> {
    save_with_picker(app, &name, ("JSON files", &["json"]), move || Ok(contents.into_bytes())).await
}

/// Takes as long as hashing every book: for a local library, every file is read.
#[tauri::command]
pub async fn rebuild_index(state: State<'_, AppState>) -> Result<RebuildIndexResponse> {
    Ok(state.library().await?.rebuild_index().await?)
}

/// Deletes every user's progress of books no file has any more.
#[tauri::command]
pub async fn clean_up_progress(state: State<'_, AppState>) -> Result<CleanUpProgressResponse> {
    Ok(state.library().await?.clean_up_progress().await?)
}

#[tauri::command]
pub async fn create_directory(
    state: State<'_, AppState>,
    req: CreateDirectoryRequest,
) -> Result<CreateDirectoryResponse> {
    Ok(state.library().await?.create_directory(req).await?)
}

/// Deletes books and whole folders from the library, on disk or on the server.
#[tauri::command]
pub async fn delete_items(
    state: State<'_, AppState>,
    req: DeleteItemsRequest,
) -> Result<DeleteItemsResponse> {
    Ok(state.library().await?.delete_items(req).await?)
}

/// One base64 chunk of a book being uploaded, appended to a file in the app cache. Sent in
/// chunks because IPC on Linux and Android turns raw bytes into a JSON array of numbers. The
/// call with `done` uploads the book to the library and returns its response; the others
/// return `null`.
#[tauri::command]
pub async fn upload_book(
    app: AppHandle,
    state: State<'_, AppState>,
    req: UploadEbookRequest,
    upload_id: String,
    offset: u64,
    data: String,
    done: bool,
) -> Result<Option<UploadEbookResponse>> {
    // It becomes a file name.
    if upload_id.is_empty() || !upload_id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err(CommandError::invalid(anyhow::anyhow!("bad upload id {upload_id:?}")));
    }
    let chunk = BASE64
        .decode(data)
        .map_err(|e| CommandError::invalid(e.into()))?;
    let path = uploads_dir(&app)?.join(format!("{upload_id}.part"));
    append_chunk(&path, offset, &chunk).await?;
    if !done {
        return Ok(None);
    }

    let book = fs::read(&path)
        .await
        .with_context(|| format!("couldn't read {}", path.display()));
    let _ = fs::remove_file(&path).await;
    let book = Bytes::from(book?);
    Ok(Some(state.library().await?.upload_book(req, book).await?))
}

fn uploads_dir<R: Runtime>(app: &AppHandle<R>) -> anyhow::Result<PathBuf> {
    Ok(app.path().app_cache_dir().context("no app cache dir")?.join("uploads"))
}

/// Chunks must arrive in order: `offset` 0 starts the file, every other one is its length.
async fn append_chunk(path: &Path, offset: u64, chunk: &[u8]) -> Result<()> {
    let mut file = if offset == 0 {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).await.context("couldn't create the uploads folder")?;
        }
        fs::File::create(path).await
    } else {
        fs::OpenOptions::new().append(true).open(path).await
    }
    .with_context(|| format!("couldn't open {}", path.display()))?;

    let len = file.metadata().await.context("couldn't read the upload's size")?.len();
    if len != offset {
        return Err(CommandError::invalid(anyhow::anyhow!(
            "upload chunk at {offset}, but {len} bytes have arrived"
        )));
    }
    file.write_all(chunk).await.context("couldn't write the upload")?;
    file.flush().await.context("couldn't write the upload")?;
    Ok(())
}

/// Uploads cut off by a crash or a closed app.
pub async fn clean_up_uploads<R: Runtime>(app: AppHandle<R>) {
    let Ok(dir) = uploads_dir(&app) else { return };
    if let Err(e) = fs::remove_dir_all(&dir).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(error = %e, "couldn't remove leftover uploads");
    }
}

/// The thumbnail JPEG as raw bytes. Not a custom URI scheme: WebKitGTK won't load one from the
/// `http://` dev server page, so `<img src>` never worked in `cargo tauri dev` on Linux.
#[tauri::command]
pub async fn get_cover(state: State<'_, AppState>, id: String) -> Result<Response> {
    let cover = state.library().await?.get_cover(&id).await?;
    raw_file(cover).await
}

/// The EPUB as raw bytes.
#[tauri::command]
pub async fn get_book_file(state: State<'_, AppState>, id: String) -> Result<Response> {
    let book = state.library().await?.get_book(&id).await?;
    raw_file(book).await
}

/// `invoke` hands raw bytes to JS as an ArrayBuffer, not a JSON array of numbers.
async fn raw_file(content: FileContent) -> Result<Response> {
    let bytes = match content {
        FileContent::Local(path) => tokio::fs::read(&path)
            .await
            .with_context(|| format!("couldn't read {}", path.display()))?,
        FileContent::Bytes(bytes) => Vec::from(bytes),
    };
    Ok(Response::new(bytes))
}
