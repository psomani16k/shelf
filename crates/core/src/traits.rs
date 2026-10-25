use std::{future::Future, path::PathBuf};

use shelf_proto::{
    api::{
        GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListDirectoriesRequest, ListDirectoriesResponse,
        SetProgressRequest, SetStateRequest,
    },
    config::Config,
};

#[derive(Debug)]
pub enum LibraryError {
    NotFound,
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for LibraryError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e)
    }
}

pub trait Library {
    fn list_books(&self, req: ListBooksRequest) -> impl Future<Output = ListBooksResponse> + Send;

    fn list_sub_directories(
        &self,
        req: ListDirectoriesRequest,
    ) -> impl Future<Output = ListDirectoriesResponse> + Send;

    /// Where the book's thumbnail JPEG is, so callers can stream it.
    fn get_cover(&self, id: &str) -> impl Future<Output = Result<PathBuf, LibraryError>> + Send;

    /// Where the book's file is, so callers can stream it.
    fn get_book(&self, id: &str) -> impl Future<Output = Result<PathBuf, LibraryError>> + Send;

    fn get_progress(
        &self,
        req: GetProgressRequest,
    ) -> impl Future<Output = Result<GetProgressResponse, LibraryError>> + Send;

    fn set_progress(
        &self,
        req: SetProgressRequest,
    ) -> impl Future<Output = Result<(), LibraryError>> + Send;

    /// A `last_book_id` that's no longer in the library comes back unset.
    fn get_state(
        &self,
        req: GetStateRequest,
    ) -> impl Future<Output = Result<GetStateResponse, LibraryError>> + Send;

    fn set_state(
        &self,
        req: SetStateRequest,
    ) -> impl Future<Output = Result<(), LibraryError>> + Send;

    fn get_config(&self) -> impl Future<Output = Result<Config, LibraryError>> + Send;

    /// Changes only the fields set in `patch`; returns the whole config.
    fn set_config(
        &self,
        patch: Config,
    ) -> impl Future<Output = Result<Config, LibraryError>> + Send;
}
