use serde::Serialize;
use shelf_core::library::LibraryError;
use tracing::{debug, error};

/// What a failed command rejects with: the same shape as the server's error responses.
/// Commands return `Result<_, CommandError>`, so this is where their errors get logged.
#[derive(Debug, Serialize)]
pub struct CommandError {
    status: u16,
    message: String,
}

impl CommandError {
    /// Bad input from the user, such as a duplicate library name: a 400, logged at debug.
    pub fn invalid(e: anyhow::Error) -> Self {
        let message = format!("{e:#}");
        debug!(error = %message, "invalid input");
        Self {
            status: 400,
            message,
        }
    }
}

impl CommandError {
    /// A 4xx the server sent, passed on with its message, logged at debug.
    fn client(status: u16, what: &str, message: String) -> Self {
        debug!(error = %message, "{what}");
        Self { status, message }
    }
}

impl From<LibraryError> for CommandError {
    fn from(e: LibraryError) -> Self {
        match e {
            LibraryError::NotFound => {
                debug!("not found");
                Self {
                    status: 404,
                    message: "not found".to_owned(),
                }
            }
            LibraryError::Invalid(message) => {
                debug!(error = %message, "invalid input");
                Self {
                    status: 400,
                    message,
                }
            }
            LibraryError::Conflict(message) => {
                debug!(error = %message, "conflict");
                Self {
                    status: 409,
                    message,
                }
            }
            LibraryError::Unauthorized(message) => Self::client(401, "unauthorized", message),
            LibraryError::Forbidden(message) => Self::client(403, "forbidden", message),
            LibraryError::TooManyRequests(message) => {
                Self::client(429, "too many requests", message)
            }
            LibraryError::Internal(e) => Self::from(e),
        }
    }
}

impl From<anyhow::Error> for CommandError {
    fn from(e: anyhow::Error) -> Self {
        let message = format!("{e:#}");
        error!(error = %message, "command failed");
        // Unlike the server, the message is shown: the user owns this device and its paths.
        Self {
            status: 500,
            message,
        }
    }
}
