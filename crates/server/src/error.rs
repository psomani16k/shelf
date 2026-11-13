use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use shelf_core::library::LibraryError;
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
                (StatusCode::NOT_FOUND, "not found".to_owned())
            }
            LibraryError::Invalid(message) => {
                debug!(error = %message, "invalid request");
                (StatusCode::BAD_REQUEST, message)
            }
            LibraryError::Conflict(message) => {
                debug!(error = %message, "conflict");
                (StatusCode::CONFLICT, message)
            }
            LibraryError::Unauthorized(message) => {
                debug!(error = %message, "unauthorized");
                (StatusCode::UNAUTHORIZED, message)
            }
            LibraryError::Forbidden(message) => {
                debug!(error = %message, "forbidden");
                (StatusCode::FORBIDDEN, message)
            }
            LibraryError::TooManyRequests(message) => {
                debug!(error = %message, "too many requests");
                (StatusCode::TOO_MANY_REQUESTS, message)
            }
            LibraryError::Internal(e) => {
                error!(error = %format!("{e:#}"), "request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_owned())
            }
        };
        let body = ApiError { message };
        (status, Json(body)).into_response()
    }
}
