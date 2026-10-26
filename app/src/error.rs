use serde::Serialize;
use shelf_core::traits::LibraryError;
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
