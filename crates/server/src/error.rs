use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use shelf_core::traits::LibraryError;
use shelf_proto::api::ApiError;
use tracing::{debug, error};

/// Turns a `LibraryError` into a JSON `ApiError` response. This is where request errors get logged.
pub struct ApiErrorResponse(pub LibraryError);

impl From<LibraryError> for ApiErrorResponse {
    fn from(e: LibraryError) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiErrorResponse {
    fn into_response(self) -> Response {
        let (status, message) = match self.0 {
            LibraryError::NotFound => {
                debug!("not found");
                (StatusCode::NOT_FOUND, "not found")
            }
            LibraryError::Internal(e) => {
                error!(error = %format!("{e:#}"), "request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error")
            }
        };
        let body = ApiError {
            message: message.to_owned(),
        };
        (status, Json(body)).into_response()
    }
}
