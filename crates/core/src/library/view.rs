use std::path::{Component, Path, PathBuf};

use anyhow::anyhow;
use bytes::Bytes;
use shelf_proto::{
    api::{
        CreateDirectoryRequest, CreateDirectoryResponse, DeleteItemsRequest, DeleteItemsResponse,
        GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse, ListBooksRequest,
        ListBooksResponse, ListBooksResponseItem, ListDirectoriesRequest, ListDirectoriesResponse,
        SetProgressRequest, SetStateRequest, UploadEbookRequest, UploadEbookResponse,
    },
    bookkeeping::{BookProgress, EpubProgress, book_file_type},
};
use tracing::{info, warn};

use crate::{
    ebook_ingest::sha1_hex,
    library::{
        LibraryError,
        book_library::BookLibrary,
        locations::{location, remove_from_index, under},
        names::{entry_name, folder_path},
        scope::Scope,
        user::validate_user,
    },
    routines::index_sync::IndexSync,
    storage::{FileContent, LibraryStorage, is_epub},
};

/// Names tried after a taken one: `name (1)` to `name (99)`.
const MAX_RENAMES: u32 = 99;

/// The library as one user sees it: paths from and to the client are relative to the scope's
/// root, and books it hides act as if they didn't exist (404s, missing from listings, never
/// touched). Like Go's `fs.Sub`, plus a filter.
pub struct LibraryView<'a> {
    lib: &'a BookLibrary,
    scope: &'a Scope,
}

impl BookLibrary {
    pub fn view<'a>(&'a self, scope: &'a Scope) -> LibraryView<'a> {
        LibraryView { lib: self, scope }
    }
}

impl LibraryView<'_> {
    pub async fn list_books(&self, req: ListBooksRequest) -> ListBooksResponse {
        let dir = req.directory_path.trim_matches('/').to_owned();
        let mut books = Vec::new();
        if let Some(folder) = self.folder(&dir) {
            let index = self.lib.metadata.read().await;
            if self.scope.shows_folder(&index, &folder) {
                let prefix = if folder.is_empty() { String::new() } else { format!("{folder}/") };
                for book in index.iter() {
                    for copy in &book.locations {
                        let Some(file_name) = copy.path.strip_prefix(&prefix) else {
                            continue;
                        };
                        // Books in subfolders belong to that subfolder's listing.
                        if file_name.contains('/') || !self.scope.allows(&copy.path) {
                            continue;
                        }
                        books.push(ListBooksResponseItem {
                            id: book.id.clone(),
                            file_name: file_name.to_owned(),
                            title: book.title.clone(),
                            authors: book.authors.clone(),
                        });
                    }
                }
            }
        }
        books.sort_by_cached_key(|b| {
            let title = b.title.as_deref().unwrap_or(&b.file_name).to_lowercase();
            (title, b.file_name.clone())
        });
        ListBooksResponse {
            current_directory: dir,
            books,
        }
    }

    /// Read from disk rather than the index, so folders without books show too.
    pub async fn list_sub_directories(&self, req: ListDirectoriesRequest) -> ListDirectoriesResponse {
        let dir = req.directory_path.trim_matches('/').to_owned();
        let mut sub_directories = match self.folder(&dir) {
            Some(folder) => {
                let path = PathBuf::from(&folder);
                match self.lib.with_storage(move |s| s.sub_folders(&path)).await {
                    Ok(names) => {
                        let index = self.lib.metadata.read().await;
                        if self.scope.shows_folder(&index, &folder) {
                            names
                                .into_iter()
                                .filter(|n| self.scope.shows_folder(&index, &join(&folder, n)))
                                .collect()
                        } else {
                            Vec::new()
                        }
                    }
                    Err(e) => {
                        warn!(dir, error = %format!("{e:#}"), "couldn't list folders");
                        Vec::new()
                    }
                }
            }
            None => {
                warn!(dir, "couldn't list folders: unsafe folder path");
                Vec::new()
            }
        };
        sub_directories.sort_by_cached_key(|d| d.to_lowercase());
        ListDirectoriesResponse {
            current_directory: dir,
            sub_directories,
        }
    }

    pub async fn get_cover(&self, id: &str) -> Result<FileContent, LibraryError> {
        if !self.has_book(id).await {
            return Err(LibraryError::NotFound);
        }
        // A book without a cover image has no thumbnail.
        self.lib
            .storage
            .book_thumbnail(id)
            .await?
            .ok_or(LibraryError::NotFound)
    }

    /// The first visible copy that's still there.
    pub async fn get_book(&self, id: &str) -> Result<FileContent, LibraryError> {
        // Copied out, so a slow fetch doesn't hold up index writes.
        let paths: Vec<String> = self
            .lib
            .metadata
            .read()
            .await
            .iter()
            .filter(|m| m.id == id)
            .flat_map(|m| &m.locations)
            .map(|l| l.path.clone())
            .filter(|p| self.scope.allows(p))
            .collect();
        for path in paths {
            // `path` comes from synced `.shelf/` data: an absolute path or `..` would escape.
            if !is_safe(&path) {
                return Err(anyhow!("book {id} has an unsafe path: {path}").into());
            }
            if let Some(file) = self.lib.storage.book_file(Path::new(&path)).await? {
                return Ok(file);
            }
        }
        Err(LibraryError::NotFound)
    }

    pub async fn get_progress(
        &self,
        req: GetProgressRequest,
    ) -> Result<GetProgressResponse, LibraryError> {
        validate_user(&req.user)?;
        if !self.has_book(&req.id).await {
            return Err(LibraryError::NotFound);
        }

        let (id, user) = (req.id.clone(), req.user);
        let progress = self.lib.with_storage(move |s| s.book_progress(&id, &user)).await;
        let epub_progress = match progress {
            Ok(progress) => progress.and_then(|p| p.epub_progress).map(|e| e.cfi),
            Err(e) => {
                warn!(id = %req.id, error = %e, "unreadable progress file");
                None
            }
        };
        Ok(GetProgressResponse {
            id: req.id,
            epub_progress,
        })
    }

    pub async fn set_progress(&self, req: SetProgressRequest) -> Result<(), LibraryError> {
        validate_user(&req.user)?;
        if !self.has_book(&req.id).await {
            return Err(LibraryError::NotFound);
        }
        let Some(cfi) = req.epub_progress else {
            return Ok(());
        };

        let progress = BookProgress {
            file_type: book_file_type::Value::Epub as i32,
            epub_progress: Some(EpubProgress { cfi }),
            user: req.user.clone(),
        };
        self.lib
            .with_storage(move |s| s.set_book_progress(&req.id, &req.user, progress))
            .await?;
        Ok(())
    }

    /// A last book the user can't see comes back unset.
    pub async fn get_state(&self, req: GetStateRequest) -> Result<GetStateResponse, LibraryError> {
        validate_user(&req.user)?;
        let state = self.lib.storage.read_state(&req.user).await?;
        let last_book_id = match state.last_book_id {
            Some(id) if self.has_book(&id).await => Some(id),
            _ => None,
        };
        Ok(GetStateResponse { last_book_id })
    }

    pub async fn set_state(&self, req: SetStateRequest) -> Result<(), LibraryError> {
        validate_user(&req.user)?;
        if let Some(id) = &req.last_book_id
            && !self.has_book(id).await
        {
            return Err(LibraryError::NotFound);
        }
        let mut state = self.lib.storage.read_state(&req.user).await?;
        state.last_book_id = req.last_book_id;
        self.lib.storage.write_state(&req.user, &state).await?;
        Ok(())
    }

    /// A copy of a book already in the library is fine: it becomes another location. Names the
    /// scope hides are skipped whether or not a file has them, so the answer never tells that a
    /// hidden file is there; a name taken by something the user can't see moves on to
    /// `name (n).epub`.
    pub async fn upload_book(
        &self,
        req: UploadEbookRequest,
        book: Bytes,
    ) -> Result<UploadEbookResponse, LibraryError> {
        let client_dir = folder_path(&req.parent_directory)?;
        let name = entry_name(&req.file_name)?.to_owned();
        if !is_epub(Path::new(&name)) {
            return Err(LibraryError::Invalid(format!("{name:?} isn't an .epub file")));
        }
        let dir = self.scope.to_library(&client_dir);

        let _guard = self.lib.files_lock.lock().await;
        self.check_parent(&dir, &client_dir).await?;
        let mut picked = None;
        for candidate in candidates(&name, true) {
            let rel = dir.join(&candidate);
            if !self.scope.allows(&rel.to_string_lossy()) {
                continue;
            }
            if !self.is_taken(&rel).await? {
                picked = Some((candidate, rel));
                break;
            }
            if self.shows(&rel).await? {
                return Err(already_exists(&client_dir.join(&candidate)));
            }
        }
        let Some((file_name, rel)) = picked else {
            return Err(LibraryError::Forbidden("books can't be added here".to_owned()));
        };

        let id = {
            let book = book.clone();
            self.lib.with_storage(move |_| Ok(sha1_hex(&book))).await?
        };
        {
            let rel = rel.clone();
            self.lib.with_storage(move |s| s.add_book(&rel, &book)).await?;
        }
        let sync = IndexSync::new(self.lib.storage.clone(), self.lib.metadata.clone());
        let abs = self.lib.storage.library_root().join(&rel);
        let synced = self.lib.with_storage(move |_| sync.sync_path(&abs)).await;
        if let Err(e) = &synced {
            warn!(path = ?rel, error = %format!("{e:#}"), "uploaded book couldn't be extracted");
        }
        // This copy, not just the book: another copy may already be indexed.
        let path = rel.to_string_lossy().into_owned();
        let indexed = self
            .lib
            .metadata
            .read()
            .await
            .iter()
            .any(|m| m.id == id && location(m, &path).is_some());
        if synced.is_err() || !indexed {
            self.lib.with_storage(move |s| s.delete_book_file(&rel)).await?;
            return Err(LibraryError::Invalid(format!("{name} isn't a readable EPUB")));
        }
        info!(path = ?rel, %id, "book uploaded");
        Ok(UploadEbookResponse { id, file_name })
    }

    /// A name taken by something the user can't see moves on to `name (n)`.
    pub async fn create_directory(
        &self,
        req: CreateDirectoryRequest,
    ) -> Result<CreateDirectoryResponse, LibraryError> {
        let client_parent = folder_path(&req.parent_directory)?;
        let name = entry_name(&req.directory_name)?.to_owned();
        let parent = self.scope.to_library(&client_parent);

        let _guard = self.lib.files_lock.lock().await;
        self.check_parent(&parent, &client_parent).await?;
        for candidate in candidates(&name, false) {
            let rel = parent.join(&candidate);
            if !self.is_taken(&rel).await? {
                {
                    let rel = rel.clone();
                    self.lib.with_storage(move |s| s.create_folder(&rel)).await?;
                }
                info!(path = ?rel, "folder created");
                return Ok(CreateDirectoryResponse {
                    directory_name: candidate,
                });
            }
            if self.shows(&rel).await? {
                return Err(already_exists(&client_parent.join(&candidate)));
            }
        }
        Err(already_exists(&client_parent.join(&name)))
    }

    /// Books are deleted one copy each. A folder goes with everything in it, unless it holds
    /// books the user can't see: then only the visible ones are deleted.
    pub async fn delete_items(
        &self,
        req: DeleteItemsRequest,
    ) -> Result<DeleteItemsResponse, LibraryError> {
        // All checked first, so a bad request deletes nothing.
        let mut dirs = Vec::new();
        for dir in &req.directory_paths {
            let path = folder_path(dir)?;
            if path.as_os_str().is_empty() {
                return Err(LibraryError::Invalid("can't delete the whole library".to_owned()));
            }
            dirs.push(self.scope.to_library(&path).to_string_lossy().into_owned());
        }
        let mut copies = Vec::new();
        for copy in &req.books {
            let path = folder_path(&copy.directory_path)?.join(entry_name(&copy.file_name)?);
            let path = self.scope.to_library(&path).to_string_lossy().into_owned();
            copies.push((copy.id.as_str(), path));
        }

        let _guard = self.lib.files_lock.lock().await;
        let (mut books, dirs) = {
            let index = self.lib.metadata.read().await;
            let mut books: Vec<String> = Vec::new();
            for (id, path) in copies {
                let indexed = index.iter().any(|m| m.id == id && location(m, &path).is_some());
                if indexed && self.scope.allows(&path) {
                    books.push(path);
                }
            }
            // A hidden folder acts as missing, which deletes nothing.
            let dirs: Vec<String> = dirs
                .into_iter()
                .filter(|d| self.scope.shows_folder(&index, d))
                .collect();
            (books, dirs)
        };
        books.sort();
        books.dedup();
        // Paths come from synced `.shelf/` data, as in `get_book`.
        if let Some(bad) = books.iter().find(|p| !is_safe(p)) {
            return Err(anyhow!("refusing to delete the unsafe path {bad}").into());
        }
        let deleted_books = books.len();

        // Checked on disk, not in the index: a book not indexed yet must be kept too.
        let mut whole_dirs = Vec::new();
        for dir in dirs {
            let inside = if self.scope.has_patterns() {
                let path = PathBuf::from(&dir);
                self.lib
                    .with_storage(move |s| {
                        if s.is_folder(&path)? {
                            s.book_file_paths_under(&path)
                        } else {
                            Ok(Default::default())
                        }
                    })
                    .await?
            } else {
                Default::default()
            };
            let inside: Vec<String> = inside
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            if inside.iter().all(|p| self.scope.allows(p)) {
                whole_dirs.push(dir);
            } else {
                books.extend(inside.into_iter().filter(|p| self.scope.allows(p)));
            }
        }

        // Each success is kept even if a later delete fails, so the index matches the disk.
        let (gone_books, gone_dirs, failure) = self
            .lib
            .with_storage(move |s| {
                let mut failure = None;
                let gone_books: Vec<String> = books
                    .into_iter()
                    .filter(|b| match s.delete_book_file(Path::new(b)) {
                        Ok(()) => true,
                        Err(e) => {
                            failure.get_or_insert(e);
                            false
                        }
                    })
                    .collect();
                let gone_dirs: Vec<String> = whole_dirs
                    .into_iter()
                    .filter(|d| match s.delete_folder(Path::new(d)) {
                        Ok(()) => true,
                        Err(e) => {
                            failure.get_or_insert(e);
                            false
                        }
                    })
                    .collect();
                Ok((gone_books, gone_dirs, failure))
            })
            .await?;

        // Now rather than when the watcher's events arrive.
        let in_gone_dir: Vec<_> = gone_dirs.iter().map(|d| under(d)).collect();
        let gone = |p: &str| gone_books.iter().any(|b| b == p) || in_gone_dir.iter().any(|f| f(p));
        remove_from_index(&mut *self.lib.metadata.write().await, gone);
        info!(books = gone_books.len(), folders = gone_dirs.len(), "deleted");

        if let Some(e) = failure {
            return Err(e.into());
        }
        Ok(DeleteItemsResponse {
            books: deleted_books as u32,
            directories: req.directory_paths.len() as u32,
        })
    }

    async fn has_book(&self, id: &str) -> bool {
        self.lib
            .metadata
            .read()
            .await
            .iter()
            .any(|m| m.id == id && self.scope.allows_book(m))
    }

    /// A folder path from the client as a library path; `None` if it isn't a valid one.
    fn folder(&self, client: &str) -> Option<String> {
        let path = folder_path(client).ok()?;
        Some(self.scope.to_library(&path).to_string_lossy().into_owned())
    }

    /// Whether `client`, a folder path relative to the user's root, is a folder they can see.
    pub async fn has_folder(&self, client: &str) -> Result<bool, LibraryError> {
        let dir = self.scope.to_library(&folder_path(client)?);
        self.shows_dir(&dir).await
    }

    /// Fails unless `dir` is a folder the user can see.
    async fn check_parent(&self, dir: &Path, client: &Path) -> Result<(), LibraryError> {
        if !self.shows_dir(dir).await? {
            return Err(LibraryError::Invalid(format!(
                "there's no folder {:?}",
                client.display()
            )));
        }
        Ok(())
    }

    async fn shows_dir(&self, dir: &Path) -> Result<bool, LibraryError> {
        let folder = {
            let dir = dir.to_owned();
            self.lib.with_storage(move |s| s.is_folder(&dir)).await?
        };
        let index = self.lib.metadata.read().await;
        Ok(folder && self.scope.shows_folder(&index, &dir.to_string_lossy()))
    }

    /// Whether anything is at `rel` (`book_modified` sees folders too).
    async fn is_taken(&self, rel: &Path) -> Result<bool, LibraryError> {
        let rel = rel.to_owned();
        Ok(self
            .lib
            .with_storage(move |s| Ok(s.book_modified(&rel)?.is_some()))
            .await?)
    }

    /// Whether `rel` is a book or folder the user can see.
    async fn shows(&self, rel: &Path) -> Result<bool, LibraryError> {
        let path = rel.to_string_lossy().into_owned();
        {
            let index = self.lib.metadata.read().await;
            if index.iter().any(|m| location(m, &path).is_some()) {
                return Ok(self.scope.allows(&path));
            }
        }
        let folder = {
            let rel = rel.to_owned();
            self.lib.with_storage(move |s| s.is_folder(&rel)).await?
        };
        Ok(folder && self.scope.shows_folder(&self.lib.metadata.read().await, &path))
    }
}

/// `name`, then `name (1)` to `name (99)`, keeping the extension of a file name.
fn candidates(name: &str, keep_extension: bool) -> impl Iterator<Item = String> + '_ {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if keep_extension && !stem.is_empty() => (stem, Some(ext)),
        _ => (name, None),
    };
    std::iter::once(name.to_owned()).chain((1..=MAX_RENAMES).map(move |n| match ext {
        Some(ext) => format!("{stem} ({n}).{ext}"),
        None => format!("{stem} ({n})"),
    }))
}

fn already_exists(client: &Path) -> LibraryError {
    LibraryError::Conflict(format!("{} already exists", client.display()))
}

fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

fn is_safe(path: &str) -> bool {
    Path::new(path)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use shelf_proto::{api::BookCopy, bookkeeping::BookMetadata};

    use super::*;
    use crate::{
        library::{Library, scope::EVERYTHING},
        storage::LocalLibraryStorage,
    };

    /// Book files whose `metadata.json` is already in `.shelf/`, so they're refreshed rather
    /// than parsed (they aren't real EPUBs). The contents give each book its id.
    async fn library(books: &[(&str, &str)]) -> (tempfile::TempDir, BookLibrary) {
        let root = tempfile::tempdir().unwrap();
        let storage = LocalLibraryStorage::new(root.path()).unwrap();
        for (path, contents) in books {
            let file = root.path().join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, contents).unwrap();
            known(&storage, contents);
        }
        let library = BookLibrary::open(root.path().to_owned(), || {}).await.unwrap();
        (root, library)
    }

    fn known(storage: &LocalLibraryStorage, contents: &str) -> String {
        let id = sha1_hex(contents.as_bytes());
        fs::create_dir_all(storage.book_dir_by_id(&id)).unwrap();
        let meta = BookMetadata {
            id: id.clone(),
            title: Some(contents.into()),
            ..Default::default()
        };
        storage.set_book_metadata(&id, &meta).unwrap();
        id
    }

    fn scope(root: &str, patterns: &[&str]) -> Scope {
        let patterns: Vec<String> = patterns.iter().map(|p| p.to_string()).collect();
        Scope::new(root, &patterns).unwrap()
    }

    async fn names(view: &LibraryView<'_>, dir: &str) -> (Vec<String>, Vec<String>) {
        let books = view
            .list_books(ListBooksRequest {
                directory_path: dir.into(),
            })
            .await;
        let folders = view
            .list_sub_directories(ListDirectoriesRequest {
                directory_path: dir.into(),
            })
            .await;
        let books = books.books.into_iter().map(|b| b.file_name).collect();
        (books, folders.sub_directories)
    }

    fn upload(dir: &str, name: &str) -> UploadEbookRequest {
        UploadEbookRequest {
            parent_directory: dir.into(),
            file_name: name.into(),
        }
    }

    #[tokio::test]
    async fn a_scoped_user_sees_only_their_books_from_their_root() {
        let books = [
            ("kid/a.epub", "a"),
            ("kid/scary/b.epub", "b"),
            ("kid/sub/c.epub", "c"),
            ("other/d.epub", "d"),
        ];
        let (_root, lib) = library(&books).await;
        let kid = scope("kid", &["**/*.epub", "!scary/**"]);
        let view = lib.view(&kid);

        assert_eq!(names(&view, "").await, (vec!["a.epub".into()], vec!["sub".into()]));
        assert_eq!(names(&view, "sub").await, (vec!["c.epub".into()], vec![]));
        assert_eq!(names(&view, "scary").await, (vec![], vec![]));

        let hidden = sha1_hex(b"b");
        assert!(matches!(view.get_book(&hidden).await, Err(LibraryError::NotFound)));
        assert!(matches!(view.get_cover(&hidden).await, Err(LibraryError::NotFound)));
        let progress = GetProgressRequest {
            id: sha1_hex(b"d"),
            user: "kid".into(),
        };
        assert!(matches!(view.get_progress(progress).await, Err(LibraryError::NotFound)));
        assert!(view.get_book(&sha1_hex(b"a")).await.is_ok());
        let folder = CreateDirectoryRequest {
            parent_directory: "scary".into(),
            directory_name: "x".into(),
        };
        assert!(matches!(view.create_directory(folder).await, Err(LibraryError::Invalid(_))));
    }

    #[tokio::test]
    async fn a_copy_of_a_known_book_uploads_as_another_location() {
        let (root, lib) = library(&[("a.epub", "a")]).await;
        fs::create_dir(root.path().join("b")).unwrap();

        let res = lib.upload_book(upload("b", "a.epub"), Bytes::from("a")).await.unwrap();

        assert_eq!((res.id, res.file_name), (sha1_hex(b"a"), "a.epub".to_owned()));
        assert_eq!(names(&lib.view(&EVERYTHING), "b").await.0, ["a.epub"]);
        assert_eq!(names(&lib.view(&EVERYTHING), "").await.0, ["a.epub"]);
    }

    #[tokio::test]
    async fn an_upload_clashing_with_a_hidden_file_gets_a_new_name() {
        let (root, lib) = library(&[("a.epub", "a"), ("kid/dune.epub", "b")]).await;
        // Not a readable EPUB and not known: on disk, but in no one's listing.
        fs::write(root.path().join("kid/junk.epub"), "junk").unwrap();
        let kid = scope("kid", &["**/*.epub", "!dune.epub"]);
        let view = lib.view(&kid);

        let renamed = view.upload_book(upload("", "dune.epub"), Bytes::from("a")).await.unwrap();
        assert_eq!(renamed.file_name, "dune (1).epub");
        let junk = view.upload_book(upload("", "junk.epub"), Bytes::from("a")).await.unwrap();
        assert_eq!(junk.file_name, "junk (1).epub");
        let visible = view.upload_book(upload("", "dune (1).epub"), Bytes::from("a")).await;
        assert!(matches!(visible, Err(LibraryError::Conflict(m)) if m == "dune (1).epub already exists"));
        assert_eq!(fs::read(root.path().join("kid/dune.epub")).unwrap(), b"b");
    }

    #[tokio::test]
    async fn deleting_a_folder_keeps_the_books_the_user_cant_see() {
        let (root, lib) = library(&[("f/a.epub", "a"), ("f/secret.epub", "b")]).await;
        let s = scope("", &["**/*.epub", "!**/secret.epub"]);
        let req = DeleteItemsRequest {
            directory_paths: vec!["f".into()],
            ..Default::default()
        };

        let res = lib.view(&s).delete_items(req).await.unwrap();

        assert_eq!((res.books, res.directories), (0, 1));
        assert!(!root.path().join("f/a.epub").exists());
        assert!(root.path().join("f/secret.epub").exists());
        assert_eq!(names(&lib.view(&EVERYTHING), "f").await.0, ["secret.epub"]);
    }

    #[tokio::test]
    async fn a_copy_is_deleted_alone_and_a_hidden_one_is_skipped() {
        let books = [("a.epub", "a"), ("b/a.epub", "a"), ("scary/a.epub", "a")];
        let (root, lib) = library(&books).await;
        let s = scope("", &["**/*.epub", "!scary/**"]);
        let view = lib.view(&s);
        let id = sha1_hex(b"a");

        let delete = |dir: &str| DeleteItemsRequest {
            books: vec![BookCopy {
                id: id.clone(),
                directory_path: dir.into(),
                file_name: "a.epub".into(),
            }],
            ..Default::default()
        };

        assert_eq!(view.delete_items(delete("")).await.unwrap().books, 1);
        assert!(!root.path().join("a.epub").exists());
        assert!(root.path().join("b/a.epub").exists());

        assert_eq!(view.delete_items(delete("scary")).await.unwrap().books, 0);
        assert!(root.path().join("scary/a.epub").exists());
        // The other copies keep the book, and everyone's progress with it.
        assert!(lib.get_book(&id).await.is_ok());
    }

    #[tokio::test]
    async fn clean_up_progress_deletes_only_removed_books() {
        let (root, lib) = library(&[("a.epub", "a")]).await;
        let storage = LocalLibraryStorage::new(root.path()).unwrap();
        let (kept, gone) = (sha1_hex(b"a"), sha1_hex(b"gone"));
        for id in [&kept, &gone] {
            let progress = storage.book_dir_by_id(id).join("progress/default.json");
            fs::create_dir_all(progress.parent().unwrap()).unwrap();
            fs::write(progress, "{}").unwrap();
        }
        // As another device's startup sync could, while the book is still here.
        fs::remove_file(storage.book_dir_by_id(&kept).join("metadata.json")).unwrap();

        assert_eq!(lib.clean_up_progress().await.unwrap().books, 1);
        assert!(!storage.book_dir_by_id(&gone).exists());
        assert!(storage.book_dir_by_id(&kept).join("progress/default.json").exists());
    }
}
