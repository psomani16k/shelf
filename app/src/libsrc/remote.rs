use std::{
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use shelf_core::traits::{Library, LibraryError};
use shelf_proto::{
    api::{
        ApiError, GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListDirectoriesRequest, ListDirectoriesResponse,
        RebuildIndexRequest, RebuildIndexResponse, SetProgressRequest, SetStateRequest,
    },
    config::Config,
};
use tauri_plugin_http::reqwest::{Client, Response, StatusCode, header};
use tokio::{fs, io::AsyncWriteExt};
use tracing::warn;

use crate::libsrc::{LibrarySource, app_library::AppLibrary};

#[derive(Clone, Deserialize, Serialize)]
pub struct RemoteLibraryBuilder {
    base_url: String,
    cache_dir: String,
    name: String,
}

impl RemoteLibraryBuilder {
    pub fn new(base_url: &str, cache_dir: &Path, name: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            cache_dir: cache_dir.to_string_lossy().to_string(),
            name: name.to_string(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Only sets up the client; the first request is what reaches the server.
    pub fn build(self) -> RemoteLibrary {
        RemoteLibrary::new(self.base_url, PathBuf::from(self.cache_dir), self.name)
    }
}

pub struct RemoteLibrary {
    base_url: String,
    client: Client,
    /// Downloaded covers and books, so the trait can hand out paths like the local library does.
    cache_dir: PathBuf,
    name: String,
}

/// The status of a failed request, kept so callers can tell a 404 apart from the rest.
#[derive(Debug)]
struct HttpStatus(StatusCode);

impl fmt::Display for HttpStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
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
        }
    }

    /// POSTs `req` as JSON to `/api{path}` and decodes the JSON reply.
    async fn api_call<S: Serialize, T: DeserializeOwned>(&self, req: &S, path: &str) -> Result<T> {
        let url = format!("{}/api{path}", self.base_url);
        let body = serde_json::to_vec(req).context("couldn't encode the request")?;
        let response = self
            .client
            .post(&url)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .with_context(|| format!("POST {url} failed"))?;

        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .with_context(|| format!("POST {url}: couldn't read the response"))?;
        if !status.is_success() {
            let message = serde_json::from_slice::<ApiError>(&bytes)
                .map(|e| e.message)
                .unwrap_or_else(|_| status.canonical_reason().unwrap_or("error").to_owned());
            return Err(HttpStatus(status)).context(format!("POST {url}: {message}"));
        }
        // A 204 has no body; `null` decodes to `()` for the set_* calls.
        let json = if bytes.is_empty() {
            b"null"
        } else {
            &bytes[..]
        };
        serde_json::from_slice(json)
            .with_context(|| format!("POST {url}: unexpected response body"))
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
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url} failed"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(HttpStatus(status)).context(format!("GET {url} failed"));
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
        Some(HttpStatus(StatusCode::NOT_FOUND)) => LibraryError::NotFound,
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

    async fn get_cover(&self, id: &str) -> Result<PathBuf, LibraryError> {
        self.cached_file(id, "cover", "jpeg")
            .await
            .map_err(to_library_error)
    }

    async fn get_book(&self, id: &str) -> Result<PathBuf, LibraryError> {
        self.cached_file(id, "file", "epub")
            .await
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

    async fn get_config(&self) -> Result<Config, LibraryError> {
        self.api_call(&(), "/config/get")
            .await
            .map_err(to_library_error)
    }

    async fn set_config(&self, patch: Config) -> Result<Config, LibraryError> {
        self.api_call(&patch, "/config/set")
            .await
            .map_err(to_library_error)
    }

    /// The server rebuilds its own index; nothing here is cached by path.
    async fn rebuild_index(&self) -> Result<RebuildIndexResponse, LibraryError> {
        self.api_call(&RebuildIndexRequest {}, "/library/rebuild")
            .await
            .map_err(to_library_error)
    }
}

impl AppLibrary for RemoteLibrary {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn get_builder(&self) -> LibrarySource {
        LibrarySource::Remote(RemoteLibraryBuilder::new(
            &self.base_url,
            &self.cache_dir,
            &self.name,
        ))
    }
}
