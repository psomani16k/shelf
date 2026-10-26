use std::{path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use shelf_core::{
    library::BookLibrary,
    traits::{Library, LibraryError},
};
use shelf_proto::{
    api::{
        GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListDirectoriesRequest, ListDirectoriesResponse,
        RebuildIndexRequest, RebuildIndexResponse, SetProgressRequest, SetStateRequest,
    },
    config::Config,
};

use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::error::ApiErrorResponse;

pub type AppState = Arc<BookLibrary>;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/books/list", post(list_books))
        .route("/directories/list", post(list_directories))
        .route("/progress/get", post(get_progress))
        .route("/progress/set", post(set_progress))
        .route("/state/get", post(get_state))
        .route("/state/set", post(set_state))
        .route("/config/get", post(get_config))
        .route("/config/set", post(set_config))
        .route("/library/rebuild", post(rebuild_index))
        .route("/books/{id}/cover", get(cover))
        .route("/books/{id}/file", get(file))
        // Unknown API paths get a JSON 404 instead of falling through to the frontend.
        .fallback(|| async { ApiErrorResponse(LibraryError::NotFound) })
}

async fn list_books(
    State(library): State<AppState>,
    Json(req): Json<ListBooksRequest>,
) -> Json<ListBooksResponse> {
    Json(library.list_books(req).await)
}

async fn list_directories(
    State(library): State<AppState>,
    Json(req): Json<ListDirectoriesRequest>,
) -> Json<ListDirectoriesResponse> {
    Json(library.list_sub_directories(req).await)
}

async fn get_progress(
    State(library): State<AppState>,
    Json(req): Json<GetProgressRequest>,
) -> Result<Json<GetProgressResponse>, ApiErrorResponse> {
    Ok(Json(library.get_progress(req).await?))
}

async fn set_progress(
    State(library): State<AppState>,
    Json(req): Json<SetProgressRequest>,
) -> Result<StatusCode, ApiErrorResponse> {
    library.set_progress(req).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_state(
    State(library): State<AppState>,
    Json(req): Json<GetStateRequest>,
) -> Result<Json<GetStateResponse>, ApiErrorResponse> {
    Ok(Json(library.get_state(req).await?))
}

async fn set_state(
    State(library): State<AppState>,
    Json(req): Json<SetStateRequest>,
) -> Result<StatusCode, ApiErrorResponse> {
    library.set_state(req).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_config(State(library): State<AppState>) -> Result<Json<Config>, ApiErrorResponse> {
    Ok(Json(library.get_config().await?))
}

async fn set_config(
    State(library): State<AppState>,
    Json(patch): Json<Config>,
) -> Result<Json<Config>, ApiErrorResponse> {
    Ok(Json(library.set_config(patch).await?))
}

async fn rebuild_index(
    State(library): State<AppState>,
    Json(_req): Json<RebuildIndexRequest>,
) -> Result<Json<RebuildIndexResponse>, ApiErrorResponse> {
    Ok(Json(library.rebuild_index().await?))
}

async fn cover(
    State(library): State<AppState>,
    Path(id): Path<String>,
    req: Request,
) -> Result<Response, ApiErrorResponse> {
    let path = library.get_cover(&id).await?;
    Ok(serve_file(path, req).await)
}

async fn file(
    State(library): State<AppState>,
    Path(id): Path<String>,
    req: Request,
) -> Result<Response, ApiErrorResponse> {
    let path = library.get_book(&id).await?;
    Ok(serve_file(path, req).await)
}

/// `ServeFile` streams the file and handles `Range`, `Last-Modified` and the content type.
async fn serve_file(path: PathBuf, req: Request) -> Response {
    let Ok(response) = ServeFile::new(path).oneshot(req).await;
    let mut response = response.into_response();
    // No browser caching for now: a regenerated thumbnail keeps its URL.
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
