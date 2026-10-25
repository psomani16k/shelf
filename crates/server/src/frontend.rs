use axum::{
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::{EmbeddedFile, RustEmbed};

#[derive(RustEmbed)]
#[folder = "../../frontend/dist"]
#[allow_missing = true]
struct Dist;

/// Serves the built SPA. Unknown paths get `index.html`, so client-side routes load the app.
pub async fn serve(uri: Uri) -> Response {
    let path = match uri.path().trim_start_matches('/') {
        "" => "index.html",
        path => path,
    };
    if let Some(file) = Dist::get(path) {
        return file_response(path, file);
    }
    match Dist::get("index.html") {
        Some(file) => file_response("index.html", file),
        None => (StatusCode::NOT_FOUND, "the frontend isn't built").into_response(),
    }
}

fn file_response(path: &str, file: EmbeddedFile) -> Response {
    // Vite puts a content hash in every file name under `assets/`.
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        [
            (header::CONTENT_TYPE, file.metadata.mimetype()),
            (header::CACHE_CONTROL, cache),
        ],
        file.data,
    )
        .into_response()
}
