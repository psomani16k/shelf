use std::sync::Arc;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, FromRef, Path, Query, Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use shelf_core::{
    library::LibraryError,
    storage::FileContent,
};
use shelf_proto::{
    api::{
        role::Value as Role,
        CleanUpProgressRequest, CleanUpProgressResponse, CreateDirectoryRequest,
        CreateDirectoryResponse, DeleteItemsRequest, DeleteItemsResponse, GetConfigRequest, GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListDirectoriesRequest, ListDirectoriesResponse,
        RebuildIndexRequest, RebuildIndexResponse, SetConfigRequest, SetProgressRequest, SetStateRequest,
        UploadEbookRequest, UploadEbookResponse,
    },
    config::Config,
};

use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::{
    auth::{self, Auth, CurrentUser},
    error::ApiErrorResponse,
    library::ServerLibrary,
};

/// Like a Go server struct the handlers hang off; each handler takes only the part it needs.
#[derive(Clone)]
pub struct AppState {
    pub library: Lib,
    pub auth: Arc<Auth>,
}

pub type Lib = Arc<ServerLibrary>;

impl FromRef<AppState> for Lib {
    fn from_ref(state: &AppState) -> Self {
        state.library.clone()
    }
}

impl FromRef<AppState> for Arc<Auth> {
    fn from_ref(state: &AppState) -> Self {
        state.auth.clone()
    }
}

/// The whole book is held in memory while it's stored, so this also caps that.
const MAX_UPLOAD_BYTES: usize = 512 * 1024 * 1024;

/// Routes are grouped by the lowest role that may call them (docs/api.md#access-by-role).
pub fn router(state: AppState) -> Router<AppState> {
    let reader = Router::new()
        .route("/books/list", post(list_books))
        .route("/directories/list", post(list_directories))
        .route("/progress/get", post(get_progress))
        .route("/progress/set", post(set_progress))
        .route("/state/get", post(get_state))
        .route("/state/set", post(set_state))
        .route("/config/get", post(get_config))
        .route("/config/set", post(set_config))
        .route("/books/{id}/cover", get(cover))
        .route("/books/{id}/file", get(file))
        .route("/auth/me", get(auth::me))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/password", post(auth::change_password));
    let editor = Router::new()
        .route("/library/delete", post(delete_items))
        .route("/directories/create", post(create_directory))
        .route(
            "/books/upload",
            post(upload_book).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        .route_layer(middleware::from_fn(|req: Request, next: Next| {
            auth::require(Role::Editor, req, next)
        }));
    let admin = Router::new()
        .route("/library/rebuild", post(rebuild_index))
        .route("/library/progress/clean", post(clean_up_progress))
        .route("/users/requests/list", post(auth::requests::list))
        .route("/users/requests/approve", post(auth::requests::approve))
        .route("/users/requests/deny", post(auth::requests::deny))
        .route("/users/list", post(auth::users::list))
        .route("/users/role/set", post(auth::users::set_role))
        .route("/users/scope/set", post(auth::users::set_scope))
        .route("/users/scope/directories", post(auth::users::list_directories))
        .route_layer(middleware::from_fn(|req: Request, next: Next| {
            auth::require(Role::Admin, req, next)
        }));
    // `route_layer` only wraps routes that matched, so an unknown path stays a 404.
    let signed_in = reader
        .merge(editor)
        .merge(admin)
        .route_layer(middleware::from_fn_with_state(state, auth::authenticate));

    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/login", post(auth::login))
        .route("/auth/signup", post(auth::requests::sign_up))
        .merge(signed_in)
        // Unknown API paths get a JSON 404 instead of falling through to the frontend.
        .fallback(|| async { ApiErrorResponse(LibraryError::NotFound) })
}

// Paths are relative to the user's root, and books outside their scope act as missing.
async fn list_books(
    State(library): State<Lib>,
    user: CurrentUser,
    Json(req): Json<ListBooksRequest>,
) -> Json<ListBooksResponse> {
    Json(library.view(&user.scope).list_books(req).await)
}

async fn list_directories(
    State(library): State<Lib>,
    user: CurrentUser,
    Json(req): Json<ListDirectoriesRequest>,
) -> Json<ListDirectoriesResponse> {
    Json(library.view(&user.scope).list_sub_directories(req).await)
}

// The user in progress, state and config requests comes from the session, never the client.
async fn get_progress(
    State(library): State<Lib>,
    user: CurrentUser,
    Json(mut req): Json<GetProgressRequest>,
) -> Result<Json<GetProgressResponse>, ApiErrorResponse> {
    req.user = user.id.clone();
    Ok(Json(library.view(&user.scope).get_progress(req).await?))
}

async fn set_progress(
    State(library): State<Lib>,
    user: CurrentUser,
    Json(mut req): Json<SetProgressRequest>,
) -> Result<StatusCode, ApiErrorResponse> {
    req.user = user.id.clone();
    library.view(&user.scope).set_progress(req).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_state(
    State(library): State<Lib>,
    user: CurrentUser,
    Json(mut req): Json<GetStateRequest>,
) -> Result<Json<GetStateResponse>, ApiErrorResponse> {
    req.user = user.id.clone();
    Ok(Json(library.view(&user.scope).get_state(req).await?))
}

async fn set_state(
    State(library): State<Lib>,
    user: CurrentUser,
    Json(mut req): Json<SetStateRequest>,
) -> Result<StatusCode, ApiErrorResponse> {
    req.user = user.id.clone();
    library.view(&user.scope).set_state(req).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_config(
    State(library): State<Lib>,
    user: CurrentUser,
    Json(mut req): Json<GetConfigRequest>,
) -> Result<Json<Config>, ApiErrorResponse> {
    req.user = user.id;
    Ok(Json(library.get_config(req).await?))
}

async fn set_config(
    State(library): State<Lib>,
    user: CurrentUser,
    Json(mut req): Json<SetConfigRequest>,
) -> Result<Json<Config>, ApiErrorResponse> {
    req.user = user.id;
    Ok(Json(library.set_config(req).await?))
}

async fn rebuild_index(
    State(library): State<Lib>,
    Json(_req): Json<RebuildIndexRequest>,
) -> Result<Json<RebuildIndexResponse>, ApiErrorResponse> {
    Ok(Json(library.rebuild_index().await?))
}

async fn clean_up_progress(
    State(library): State<Lib>,
    Json(_req): Json<CleanUpProgressRequest>,
) -> Result<Json<CleanUpProgressResponse>, ApiErrorResponse> {
    Ok(Json(library.clean_up_progress().await?))
}

/// The EPUB is the raw body, like covers and files on the way out; the request is the query.
async fn upload_book(
    State(library): State<Lib>,
    user: CurrentUser,
    Query(req): Query<UploadEbookRequest>,
    book: Bytes,
) -> Result<Json<UploadEbookResponse>, ApiErrorResponse> {
    Ok(Json(library.view(&user.scope).upload_book(req, book).await?))
}

async fn create_directory(
    State(library): State<Lib>,
    user: CurrentUser,
    Json(req): Json<CreateDirectoryRequest>,
) -> Result<Json<CreateDirectoryResponse>, ApiErrorResponse> {
    Ok(Json(library.view(&user.scope).create_directory(req).await?))
}

async fn delete_items(
    State(library): State<Lib>,
    user: CurrentUser,
    Json(req): Json<DeleteItemsRequest>,
) -> Result<Json<DeleteItemsResponse>, ApiErrorResponse> {
    Ok(Json(library.view(&user.scope).delete_items(req).await?))
}

async fn cover(
    State(library): State<Lib>,
    user: CurrentUser,
    Path(id): Path<String>,
    req: Request,
) -> Result<Response, ApiErrorResponse> {
    let cover = library.view(&user.scope).get_cover(&id).await?;
    Ok(serve(cover, "image/jpeg", req).await)
}

async fn file(
    State(library): State<Lib>,
    user: CurrentUser,
    Path(id): Path<String>,
    req: Request,
) -> Result<Response, ApiErrorResponse> {
    let book = library.view(&user.scope).get_book(&id).await?;
    Ok(serve(book, "application/epub+zip", req).await)
}

/// A local file goes through `ServeFile`, which streams it and handles `Range`, `Last-Modified`
/// and the content type. Bytes are sent whole, as `mime`.
async fn serve(content: FileContent, mime: &'static str, req: Request) -> Response {
    let mut response = match content {
        FileContent::Local(path) => {
            let Ok(response) = ServeFile::new(path).oneshot(req).await;
            response.into_response()
        }
        FileContent::Bytes(bytes) => ([(header::CONTENT_TYPE, mime)], bytes).into_response(),
    };
    // No browser caching for now: a regenerated thumbnail keeps its URL.
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use shelf_proto::api::{ListUserRequestsResponse, ListUsersResponse, LoginResponse, User};
    use shelf_utility::timeutil::Time;
    use tempfile::TempDir;

    use super::*;
    use crate::data::DataFile;

    /// The API router over a temp library and data folder, with an owner, a reader, and `kid`,
    /// whose library is the `kid/` folder.
    struct TestServer {
        app: Router,
        _books: TempDir,
        _data: TempDir,
    }

    impl TestServer {
        async fn new() -> Self {
            let books = tempfile::tempdir().unwrap();
            let data = tempfile::tempdir().unwrap();
            let users = r#"{ "users": {
                "owner": { "password": "o", "role": "OWNER" },
                "reader": { "password": "r", "role": "READER" },
                "kid": { "password": "k", "role": "READER", "library_root": "kid" }
            } }"#;
            std::fs::write(data.path().join("data.json"), users).unwrap();
            std::fs::create_dir_all(books.path().join("kid/sub")).unwrap();
            let library = ServerLibrary::open(books.path().to_owned(), || {}).await.unwrap();
            let file = DataFile::load(data.path(), Time::now()).await.unwrap();
            let state = AppState {
                library: Arc::new(library),
                auth: Arc::new(Auth::new(file)),
            };
            Self {
                app: router(state.clone()).with_state(state),
                _books: books,
                _data: data,
            }
        }

        async fn call(
            &self,
            method: &str,
            path: &str,
            token: Option<&str>,
            body: &str,
        ) -> (StatusCode, String) {
            let mut req = axum::http::Request::builder()
                .method(method)
                .uri(path)
                .header(header::CONTENT_TYPE, "application/json");
            if let Some(token) = token {
                req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
            }
            let req = req.body(Body::from(body.to_owned())).unwrap();
            let res = self.app.clone().oneshot(req).await.unwrap();
            let status = res.status();
            let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
            (status, String::from_utf8_lossy(&bytes).into_owned())
        }

        async fn login(&self, username: &str, password: &str) -> String {
            let body = format!(r#"{{ "username": "{username}", "password": "{password}" }}"#);
            let (status, body) = self.call("POST", "/auth/login", None, &body).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            serde_json::from_str::<LoginResponse>(&body).unwrap().token
        }
    }

    #[tokio::test]
    async fn every_route_but_health_and_login_needs_a_session() {
        let server = TestServer::new().await;

        assert_eq!(server.call("GET", "/health", None, "").await.0, StatusCode::OK);
        let cases = [
            ("POST", "/books/list", None),
            ("GET", "/books/abc/cover", None),
            ("GET", "/books/abc/file", None),
            ("POST", "/books/list", Some("made-up")),
            ("GET", "/auth/me", None),
        ];
        for (method, path, token) in cases {
            let (status, _) = server.call(method, path, token, "{}").await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
        }
        assert_eq!(server.call("GET", "/nope", None, "").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn roles_limit_routes() {
        let server = TestServer::new().await;
        let reader = server.login("reader", "r").await;
        let owner = server.login("owner", "o").await;
        let folder = r#"{ "parent_directory": "", "directory_name": "new" }"#;

        let list = server.call("POST", "/books/list", Some(&reader), "{}").await;
        assert_eq!(list.0, StatusCode::OK);
        let create = server.call("POST", "/directories/create", Some(&reader), folder).await;
        assert_eq!(create.0, StatusCode::FORBIDDEN);
        let rebuild = server.call("POST", "/library/rebuild", Some(&reader), "{}").await;
        assert_eq!(rebuild.0, StatusCode::FORBIDDEN);

        let create = server.call("POST", "/directories/create", Some(&owner), folder).await;
        assert_eq!(create.0, StatusCode::OK, "{}", create.1);
    }

    #[tokio::test]
    async fn me_and_logout() {
        let server = TestServer::new().await;
        let token = server.login("reader", "r").await;

        let (status, body) = server.call("GET", "/auth/me", Some(&token), "").await;
        assert_eq!(status, StatusCode::OK);
        let me: User = serde_json::from_str(&body).unwrap();
        assert_eq!(me.username, "reader");
        assert_eq!(me.role(), Role::Reader);

        let logout = server.call("POST", "/auth/logout", Some(&token), "{}").await;
        assert_eq!(logout.0, StatusCode::NO_CONTENT);
        let after = server.call("GET", "/auth/me", Some(&token), "").await;
        assert_eq!(after.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_wrong_password_is_401() {
        let server = TestServer::new().await;
        let body = r#"{ "username": "owner", "password": "x" }"#;

        let (status, body) = server.call("POST", "/auth/login", None, body).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(body.contains("wrong username or password"));
    }

    #[tokio::test]
    async fn anyone_can_sign_up_and_only_admins_see_the_requests() {
        let server = TestServer::new().await;
        let body = r#"{ "username": "newbie", "password": "n" }"#;

        let (status, body) = server.call("POST", "/auth/signup", None, body).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");

        let reader = server.login("reader", "r").await;
        let denied = server.call("POST", "/users/requests/list", Some(&reader), "{}").await;
        assert_eq!(denied.0, StatusCode::FORBIDDEN);

        let owner = server.login("owner", "o").await;
        let (status, body) = server.call("POST", "/users/requests/list", Some(&owner), "{}").await;
        assert_eq!(status, StatusCode::OK);
        let list: ListUserRequestsResponse = serde_json::from_str(&body).unwrap();
        let approve = format!(r#"{{ "id": "{}", "role": "EDITOR" }}"#, list.requests[0].id);
        let approved = server.call("POST", "/users/requests/approve", Some(&owner), &approve).await;
        assert_eq!(approved.0, StatusCode::NO_CONTENT, "{}", approved.1);
        server.login("newbie", "n").await;
    }

    #[tokio::test]
    async fn only_admins_list_users_and_change_roles() {
        let server = TestServer::new().await;
        let reader = server.login("reader", "r").await;
        let denied = server.call("POST", "/users/list", Some(&reader), "{}").await;
        assert_eq!(denied.0, StatusCode::FORBIDDEN);

        let owner = server.login("owner", "o").await;
        let (status, body) = server.call("POST", "/users/list", Some(&owner), "{}").await;
        assert_eq!(status, StatusCode::OK);
        let list: ListUsersResponse = serde_json::from_str(&body).unwrap();
        assert_eq!(list.users.len(), 3);
        let ban = r#"{ "username": "reader", "role": "BANNED" }"#;
        let banned = server.call("POST", "/users/role/set", Some(&owner), ban).await;
        assert_eq!(banned.0, StatusCode::NO_CONTENT, "{}", banned.1);

        let after = server.call("POST", "/books/list", Some(&reader), "{}").await;
        assert_eq!(after.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_scoped_user_sees_their_folder_as_the_library() {
        let server = TestServer::new().await;
        let list = |dir: &str| format!(r#"{{ "directory_path": "{dir}" }}"#);

        let kid = server.login("kid", "k").await;
        let (status, body) = server.call("POST", "/directories/list", Some(&kid), &list("")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let root: ListDirectoriesResponse = serde_json::from_str(&body).unwrap();
        assert_eq!((root.current_directory.as_str(), root.sub_directories), ("", vec!["sub".into()]));
        let (_, body) = server.call("POST", "/directories/list", Some(&kid), &list("sub")).await;
        assert!(!body.contains("kid"), "{body}");

        let owner = server.login("owner", "o").await;
        let (_, body) = server.call("POST", "/directories/list", Some(&owner), &list("")).await;
        let all: ListDirectoriesResponse = serde_json::from_str(&body).unwrap();
        assert_eq!(all.sub_directories, ["kid"]);
    }

    #[tokio::test]
    async fn admins_set_scopes_from_the_whole_library() {
        let server = TestServer::new().await;
        let list = r#"{ "directory_path": "" }"#;
        let set = |root: &str| {
            format!(r#"{{ "username": "kid", "scope": {{ "library_root": "{root}", "books": [] }} }}"#)
        };

        let reader = server.login("reader", "r").await;
        for path in ["/users/scope/set", "/users/scope/directories"] {
            let (status, _) = server.call("POST", path, Some(&reader), &set("")).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
        }

        let owner = server.login("owner", "o").await;
        let (_, body) = server.call("POST", "/users/scope/directories", Some(&owner), list).await;
        let all: ListDirectoriesResponse = serde_json::from_str(&body).unwrap();
        assert_eq!(all.sub_directories, ["kid"]);

        let (status, body) = server.call("POST", "/users/scope/set", Some(&owner), &set("nope")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

        let kid = server.login("kid", "k").await;
        let (status, body) = server.call("POST", "/users/scope/set", Some(&owner), &set("kid/sub")).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        let (_, body) = server.call("POST", "/directories/list", Some(&kid), list).await;
        let now: ListDirectoriesResponse = serde_json::from_str(&body).unwrap();
        assert!(now.sub_directories.is_empty(), "{body}");
    }

    #[tokio::test]
    async fn only_admins_clean_up_progress() {
        let server = TestServer::new().await;
        let reader = server.login("reader", "r").await;
        let denied = server.call("POST", "/library/progress/clean", Some(&reader), "{}").await;
        assert_eq!(denied.0, StatusCode::FORBIDDEN);

        let owner = server.login("owner", "o").await;
        let (status, body) = server.call("POST", "/library/progress/clean", Some(&owner), "{}").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let res: CleanUpProgressResponse = serde_json::from_str(&body).unwrap();
        assert_eq!(res.books, 0);
    }
}
