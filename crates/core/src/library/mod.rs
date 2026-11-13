pub mod book_library;
pub mod locations;
pub mod names;
pub mod scope;
pub mod user;
pub mod view;
use std::future::Future;

use bytes::Bytes;
use shelf_proto::{
    api::{
        CleanUpProgressResponse, CreateDirectoryRequest, CreateDirectoryResponse, DeleteItemsRequest, DeleteItemsResponse,
        GetConfigRequest, GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListDirectoriesRequest, ListDirectoriesResponse,
        RebuildIndexResponse, SetConfigRequest, SetProgressRequest, SetStateRequest,
        UploadEbookRequest, UploadEbookResponse,
    },
    config::Config,
};

use crate::storage::FileContent;

#[derive(Debug)]
pub enum LibraryError {
    NotFound,
    /// Bad input, such as an unsafe name; the message is safe to show to the client.
    Invalid(String),
    /// The name is taken; safe to show to the client.
    Conflict(String),
    /// No valid session (401). Only the server's auth and a remote library produce the auth
    /// errors; the messages are safe to show to the client.
    Unauthorized(String),
    /// The user's role is too low, or the request is outside what they may do (403).
    Forbidden(String),
    /// Too many failed logins (429).
    TooManyRequests(String),
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

    /// The book's thumbnail JPEG: a local file callers can stream, or its bytes.
    fn get_cover(&self, id: &str)
    -> impl Future<Output = Result<FileContent, LibraryError>> + Send;

    /// The book's EPUB: a local file callers can stream, or its bytes.
    fn get_book(&self, id: &str) -> impl Future<Output = Result<FileContent, LibraryError>> + Send;

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

    /// The user's own settings.
    fn get_config(
        &self,
        req: GetConfigRequest,
    ) -> impl Future<Output = Result<Config, LibraryError>> + Send;

    /// Changes only the fields set in `req.patch`; returns the user's whole config.
    fn set_config(
        &self,
        req: SetConfigRequest,
    ) -> impl Future<Output = Result<Config, LibraryError>> + Send;

    /// Re-hashes every book, ignoring the copies' stamps, and reloads the index. Slow on big
    /// libraries; for when the index looks wrong.
    fn rebuild_index(&self) -> impl Future<Output = Result<RebuildIndexResponse, LibraryError>> + Send;

    /// Deletes the `.shelf/books/<id>/` folders of books no file has any more (no
    /// `metadata.json`, not in the index), every user's progress included.
    fn clean_up_progress(
        &self,
    ) -> impl Future<Output = Result<CleanUpProgressResponse, LibraryError>> + Send;

    /// Adds `book` as `req.file_name` in `req.parent_directory` and indexes it; a book already
    /// in the library gets another location. A taken name is a `Conflict`; a file that isn't a
    /// readable EPUB is `Invalid` and isn't kept.
    fn upload_book(
        &self,
        req: UploadEbookRequest,
        book: Bytes,
    ) -> impl Future<Output = Result<UploadEbookResponse, LibraryError>> + Send;

    fn create_directory(
        &self,
        req: CreateDirectoryRequest,
    ) -> impl Future<Output = Result<CreateDirectoryResponse, LibraryError>> + Send;

    /// Deletes the book copies and the folders, with everything in them. Unknown copies are
    /// skipped.
    fn delete_items(
        &self,
        req: DeleteItemsRequest,
    ) -> impl Future<Output = Result<DeleteItemsResponse, LibraryError>> + Send;
}
