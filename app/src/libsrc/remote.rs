use std::{
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use bytes::Bytes;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use shelf_core::{
    library::{Library, LibraryError},
    storage::FileContent,
};
use shelf_proto::{
    api::{
        ApiError, ApproveUserRequestRequest, ChangePasswordRequest, DenyUserRequestRequest,
        ListUserRequestsRequest, ListUserRequestsResponse, ListUsersRequest, ListUsersResponse,
        SetUserRoleRequest, SetUserScopeRequest, SignUpRequest, CreateDirectoryRequest, CreateDirectoryResponse, DeleteItemsRequest,
        DeleteItemsResponse, GetConfigRequest, GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListDirectoriesRequest, ListDirectoriesResponse,
        RebuildIndexRequest, CleanUpProgressRequest, CleanUpProgressResponse, RebuildIndexResponse, SetConfigRequest, SetProgressRequest, SetStateRequest,
        LoginRequest, LoginResponse, LogoutRequest, UploadEbookRequest, UploadEbookResponse, User,
    },
    config::Config,
};
use tauri_plugin_http::reqwest::{Client, RequestBuilder, Response, StatusCode, Url, header};
use tokio::{fs, io::AsyncWriteExt, sync::RwLock};
use tracing::warn;

use crate::libsrc::{LibrarySource, app_library::AppLibrary};

#[derive(Clone, Deserialize, Serialize)]
pub struct RemoteLibraryBuilder {
    base_url: String,
    cache_dir: String,
    name: String,
    /// The server's login key, kept in the app's private data so the device stays signed in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

impl RemoteLibraryBuilder {
    pub fn new(base_url: &str, cache_dir: &Path, name: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            cache_dir: cache_dir.to_string_lossy().to_string(),
            name: name.to_string(),
            token: None,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn signed_in(&self) -> bool {
        self.token.is_some()
    }

    pub fn set_token(&mut self, token: Option<String>) {
        self.token = token;
    }

    /// Only sets up the client; the first request is what reaches the server.
    pub fn build(self) -> RemoteLibrary {
        let mut library = RemoteLibrary::new(self.base_url, PathBuf::from(self.cache_dir), self.name);
        library.token = RwLock::new(self.token);
        library
    }
}

pub struct RemoteLibrary {
    base_url: String,
    client: Client,
    /// Downloaded covers and books, so the trait can hand out paths like the local library does.
    cache_dir: PathBuf,
    name: String,
    /// Sent as a bearer token on every request; `None` until signed in.
    token: RwLock<Option<String>>,
}

/// The status of a failed request and the server's message, kept so callers can tell a 404,
/// 400 or 409 apart from the rest.
#[derive(Debug)]
struct HttpStatus {
    status: StatusCode,
    message: String,
}

impl fmt::Display for HttpStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.status)
    }
}

impl std::error::Error for HttpStatus {}

impl RemoteLibrary {
    /// `base_url` is the server's origin, such as `http://nas:8080`, with or without a trailing slash.
    pub fn new(base_url: String, cache_dir: PathBuf, name: String) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client");
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            client,
            cache_dir,
            name,
            token: RwLock::new(None),
        }
    }

    async fn authorized(&self, request: RequestBuilder) -> RequestBuilder {
        match self.token.read().await.as_deref() {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    /// POSTs `req` as JSON to `/api{path}` and decodes the JSON reply.
    async fn api_call<S: Serialize, T: DeserializeOwned>(&self, req: &S, path: &str) -> Result<T> {
        let url = format!("{}/api{path}", self.base_url);
        let body = serde_json::to_vec(req).context("couldn't encode the request")?;
        let request = self
            .client
            .post(&url)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body);
        self.send(request, &url).await
    }

    /// Sends the request with the login key and decodes the JSON reply, or turns an `ApiError`
    /// body into an `HttpStatus` error.
    async fn send<T: DeserializeOwned>(&self, request: RequestBuilder, url: &str) -> Result<T> {
        let response = self
            .authorized(request)
            .await
            .send()
            .await
            .with_context(|| format!("{url} failed"))?;

        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .with_context(|| format!("{url}: couldn't read the response"))?;
        if !status.is_success() {
            let message = serde_json::from_slice::<ApiError>(&bytes)
                .map(|e| e.message)
                .unwrap_or_else(|_| status.canonical_reason().unwrap_or("error").to_owned());
            let context = format!("{url}: {message}");
            return Err(HttpStatus { status, message }).context(context);
        }
        // A 204 has no body; `null` decodes to `()` for the set_* calls.
        let json = if bytes.is_empty() {
            b"null"
        } else {
            &bytes[..]
        };
        serde_json::from_slice(json)
            .with_context(|| format!("{url}: unexpected response body"))
    }

    /// GETs `/api/books/{id}/{kind}` into the cache once; ids are content hashes, so a cached file
    /// never goes stale.
    async fn cached_file(&self, id: &str, kind: &str, ext: &str) -> Result<PathBuf> {
        // The id becomes a file name, so a server can't send `../` and write outside the cache.
        ensure!(
            !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric()),
            "unsafe book id {id:?}"
        );
        let dir = self.cache_dir.join(kind);
        let path = dir.join(format!("{id}.{ext}"));
        if fs::try_exists(&path).await? {
            return Ok(path);
        }

        let url = format!("{}/api/books/{id}/{kind}", self.base_url);
        let response = self
            .authorized(self.client.get(&url))
            .await
            .send()
            .await
            .with_context(|| format!("GET {url} failed"))?;
        let status = response.status();
        if !status.is_success() {
            let bytes = response.bytes().await.unwrap_or_default();
            let message = serde_json::from_slice::<ApiError>(&bytes)
                .map(|e| e.message)
                .unwrap_or_default();
            return Err(HttpStatus { status, message }).context(format!("GET {url} failed"));
        }

        fs::create_dir_all(&dir).await?;
        let tmp = dir.join(format!(".{id}.{ext}.part-{}", std::process::id()));
        if let Err(e) = download(response, &tmp).await {
            let _ = fs::remove_file(&tmp).await;
            return Err(e);
        }
        fs::rename(&tmp, &path).await?;
        Ok(path)
    }
}

/// Signing in to the server. Not `Library` operations: a local library has no accounts.
impl RemoteLibrary {
    /// Keeps the new key for every later request; the caller stores it.
    pub async fn login(&self, req: &LoginRequest) -> Result<LoginResponse, LibraryError> {
        let res: LoginResponse = self
            .api_call(req, "/auth/login")
            .await
            .map_err(to_library_error)?;
        *self.token.write().await = Some(res.token.clone());
        Ok(res)
    }

    /// Forgets the key even when the server can't be reached.
    pub async fn logout(&self) -> Result<(), LibraryError> {
        let result = self.api_call::<_, ()>(&LogoutRequest {}, "/auth/logout").await;
        *self.token.write().await = None;
        result.map_err(to_library_error)
    }

    pub async fn me(&self) -> Result<User, LibraryError> {
        let url = format!("{}/api/auth/me", self.base_url);
        self.send(self.client.get(&url), &url)
            .await
            .map_err(to_library_error)
    }

    pub async fn change_password(&self, req: &ChangePasswordRequest) -> Result<(), LibraryError> {
        self.api_call(req, "/auth/password")
            .await
            .map_err(to_library_error)
    }

    /// Needs no key: it asks an admin for an account.
    pub async fn sign_up(&self, req: &SignUpRequest) -> Result<(), LibraryError> {
        self.api_call(req, "/auth/signup")
            .await
            .map_err(to_library_error)
    }

    pub async fn list_user_requests(&self) -> Result<ListUserRequestsResponse, LibraryError> {
        self.api_call(&ListUserRequestsRequest {}, "/users/requests/list")
            .await
            .map_err(to_library_error)
    }

    pub async fn approve_user_request(
        &self,
        req: &ApproveUserRequestRequest,
    ) -> Result<(), LibraryError> {
        self.api_call(req, "/users/requests/approve")
            .await
            .map_err(to_library_error)
    }

    pub async fn deny_user_request(&self, req: &DenyUserRequestRequest) -> Result<(), LibraryError> {
        self.api_call(req, "/users/requests/deny")
            .await
            .map_err(to_library_error)
    }

    pub async fn list_users(&self) -> Result<ListUsersResponse, LibraryError> {
        self.api_call(&ListUsersRequest {}, "/users/list")
            .await
            .map_err(to_library_error)
    }

    pub async fn set_user_role(&self, req: &SetUserRoleRequest) -> Result<(), LibraryError> {
        self.api_call(req, "/users/role/set")
            .await
            .map_err(to_library_error)
    }

    pub async fn set_user_scope(&self, req: &SetUserScopeRequest) -> Result<(), LibraryError> {
        self.api_call(req, "/users/scope/set")
            .await
            .map_err(to_library_error)
    }

    pub async fn list_scope_directories(
        &self,
        req: &ListDirectoriesRequest,
    ) -> Result<ListDirectoriesResponse, LibraryError> {
        self.api_call(req, "/users/scope/directories")
            .await
            .map_err(to_library_error)
    }
}

async fn download(mut response: Response, to: &Path) -> Result<()> {
    let mut file = fs::File::create(to).await?;
    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(())
}

fn to_library_error(e: anyhow::Error) -> LibraryError {
    match e.downcast_ref::<HttpStatus>() {
        Some(s) if s.status == StatusCode::NOT_FOUND => LibraryError::NotFound,
        Some(s) if s.status == StatusCode::BAD_REQUEST => LibraryError::Invalid(s.message.clone()),
        Some(s) if s.status == StatusCode::CONFLICT => LibraryError::Conflict(s.message.clone()),
        Some(s) if s.status == StatusCode::UNAUTHORIZED => {
            LibraryError::Unauthorized(s.message.clone())
        }
        Some(s) if s.status == StatusCode::FORBIDDEN => LibraryError::Forbidden(s.message.clone()),
        Some(s) if s.status == StatusCode::TOO_MANY_REQUESTS => {
            LibraryError::TooManyRequests(s.message.clone())
        }
        _ => LibraryError::Internal(e),
    }
}

impl Library for RemoteLibrary {
    async fn list_books(&self, req: ListBooksRequest) -> ListBooksResponse {
        let directory = req.directory_path.clone();
        self.api_call(&req, "/books/list")
            .await
            .unwrap_or_else(|e| {
                warn!(error = %format!("{e:#}"), "couldn't list books");
                ListBooksResponse {
                    current_directory: directory,
                    books: Vec::new(),
                }
            })
    }

    async fn list_sub_directories(&self, req: ListDirectoriesRequest) -> ListDirectoriesResponse {
        let directory = req.directory_path.clone();
        self.api_call(&req, "/directories/list")
            .await
            .unwrap_or_else(|e| {
                warn!(error = %format!("{e:#}"), "couldn't list directories");
                ListDirectoriesResponse {
                    current_directory: directory,
                    sub_directories: Vec::new(),
                }
            })
    }

    async fn get_cover(&self, id: &str) -> Result<FileContent, LibraryError> {
        self.cached_file(id, "cover", "jpeg")
            .await
            .map(FileContent::Local)
            .map_err(to_library_error)
    }

    async fn get_book(&self, id: &str) -> Result<FileContent, LibraryError> {
        self.cached_file(id, "file", "epub")
            .await
            .map(FileContent::Local)
            .map_err(to_library_error)
    }

    async fn get_progress(
        &self,
        req: GetProgressRequest,
    ) -> Result<GetProgressResponse, LibraryError> {
        self.api_call(&req, "/progress/get")
            .await
            .map_err(to_library_error)
    }

    async fn set_progress(&self, req: SetProgressRequest) -> Result<(), LibraryError> {
        self.api_call(&req, "/progress/set")
            .await
            .map_err(to_library_error)
    }

    async fn get_state(&self, req: GetStateRequest) -> Result<GetStateResponse, LibraryError> {
        self.api_call(&req, "/state/get")
            .await
            .map_err(to_library_error)
    }

    async fn set_state(&self, req: SetStateRequest) -> Result<(), LibraryError> {
        self.api_call(&req, "/state/set")
            .await
            .map_err(to_library_error)
    }

    async fn get_config(&self, req: GetConfigRequest) -> Result<Config, LibraryError> {
        self.api_call(&req, "/config/get")
            .await
            .map_err(to_library_error)
    }

    async fn set_config(&self, req: SetConfigRequest) -> Result<Config, LibraryError> {
        self.api_call(&req, "/config/set")
            .await
            .map_err(to_library_error)
    }

    /// The server rebuilds its own index; nothing here is cached by path.
    async fn rebuild_index(&self) -> Result<RebuildIndexResponse, LibraryError> {
        self.api_call(&RebuildIndexRequest {}, "/library/rebuild")
            .await
            .map_err(to_library_error)
    }

    async fn clean_up_progress(&self) -> Result<CleanUpProgressResponse, LibraryError> {
        self.api_call(&CleanUpProgressRequest {}, "/library/progress/clean")
            .await
            .map_err(to_library_error)
    }

    /// The server's upload route takes the request as the query and the book as the raw body.
    async fn upload_book(
        &self,
        req: UploadEbookRequest,
        book: Bytes,
    ) -> Result<UploadEbookResponse, LibraryError> {
        let upload = async {
            let mut url = Url::parse(&format!("{}/api/books/upload", self.base_url))?;
            url.query_pairs_mut()
                .append_pair("parent_directory", &req.parent_directory)
                .append_pair("file_name", &req.file_name);
            let request = self
                .client
                .post(url.clone())
                .header(header::CONTENT_TYPE, "application/epub+zip")
                .body(book);
            self.send::<UploadEbookResponse>(request, url.as_str()).await
        };
        upload.await.map_err(to_library_error)
    }

    async fn create_directory(
        &self,
        req: CreateDirectoryRequest,
    ) -> Result<CreateDirectoryResponse, LibraryError> {
        self.api_call(&req, "/directories/create")
            .await
            .map_err(to_library_error)
    }

    async fn delete_items(&self, req: DeleteItemsRequest) -> Result<DeleteItemsResponse, LibraryError> {
        self.api_call(&req, "/library/delete")
            .await
            .map_err(to_library_error)
    }
}

impl AppLibrary for RemoteLibrary {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn get_builder(&self) -> LibrarySource {
        let mut builder = RemoteLibraryBuilder::new(&self.base_url, &self.cache_dir, &self.name);
        builder.token = self.token.try_read().ok().and_then(|token| token.clone());
        LibrarySource::Remote(builder)
    }
}
