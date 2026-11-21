use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

use anyhow::anyhow;
use bytes::Bytes;
use shelf_proto::{
    api::{
        AddAnnotationRequest, BookAnnotationsEntry, DeleteAnnotationRequest,
        ListAnnotationsRequest, ListAnnotationsResponse, UpdateAnnotationRequest,
        BookProgressEntry, CreateDirectoryRequest, CreateDirectoryResponse, DeleteItemsRequest,
        DeleteItemsResponse, ExportUserDataRequest, GetProgressRequest, GetProgressResponse,
        GetStateRequest, GetStateResponse, ImportUserDataRequest, ImportUserDataResponse,
        ListBooksRequest, ListBooksResponse, ListBooksResponseItem, ListDirectoriesRequest,
        ListDirectoriesResponse, SetProgressRequest, SetStateRequest, UploadEbookRequest,
        UploadEbookResponse, UserData,
    },
    bookkeeping::{Annotation, BookAnnotations, BookProgress, EpubProgress, book_file_type},
};
use shelf_utility::timeutil::Time;
use tracing::{info, warn};

use crate::{
    ebook_ingest::sha1_hex,
    library::{
        LibraryError, annotations,
        book_library::BookLibrary,
        locations::{has_copy, location, remove_from_index, under},
        names::{entry_name, folder_path},
        scope::Scope,
        user::validate_user,
    },
    routines::index_sync::IndexSync,
    storage::{FileContent, LibraryStorage, LocalLibraryStorage, is_epub},
    themes,
};

/// `UserData.version` written by exports and accepted by imports.
const USER_DATA_VERSION: u32 = 1;

/// Names tried after a taken one: `name (1)` to `name (99)`.
const MAX_RENAMES: u32 = 99;

/// The library as one user sees it: paths from and to the client are relative to the scope's
/// root, and books it hides act as if they didn't exist (404s, missing from listings, never
/// touched).
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
    /// A missing or hidden folder lists as empty; an unsafe path is `Invalid`.
    pub async fn list_books(&self, req: ListBooksRequest) -> Result<ListBooksResponse, LibraryError> {
        let dir = req.directory_path.trim_matches('/').to_owned();
        let folder = self.folder(&dir)?;
        let mut books = Vec::new();
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
        drop(index);
        books.sort_by_cached_key(|b| {
            let title = b.title.as_deref().unwrap_or(&b.file_name).to_lowercase();
            (title, b.file_name.clone())
        });
        Ok(ListBooksResponse {
            current_directory: dir,
            books,
        })
    }

    /// Read from disk rather than the index, so folders without books show too. A missing or
    /// hidden folder lists as empty; an unsafe path is `Invalid`.
    pub async fn list_sub_directories(
        &self,
        req: ListDirectoriesRequest,
    ) -> Result<ListDirectoriesResponse, LibraryError> {
        let dir = req.directory_path.trim_matches('/').to_owned();
        let folder = self.folder(&dir)?;
        let path = PathBuf::from(&folder);
        let names = self.lib.with_storage(move |s| s.sub_folders(&path)).await?;
        let index = self.lib.metadata.read().await;
        let mut sub_directories: Vec<String> = if self.scope.shows_folder(&index, &folder) {
            names
                .into_iter()
                .filter(|n| self.scope.shows_folder(&index, &join(&folder, n)))
                .collect()
        } else {
            Vec::new()
        };
        drop(index);
        sub_directories.sort_by_cached_key(|d| d.to_lowercase());
        Ok(ListDirectoriesResponse {
            current_directory: dir,
            sub_directories,
        })
    }

    pub async fn get_cover(&self, id: &str) -> Result<FileContent, LibraryError> {
        self.require_book(id).await?;
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
        self.require_book(&req.id).await?;

        let (id, user) = (req.id.clone(), req.user);
        let epub_progress = self.lib.with_storage(move |s| Ok(read_cfi(&s, &id, &user))).await?;
        Ok(GetProgressResponse {
            id: req.id,
            epub_progress,
        })
    }

    pub async fn set_progress(&self, req: SetProgressRequest) -> Result<(), LibraryError> {
        validate_user(&req.user)?;
        self.require_book(&req.id).await?;
        let Some(cfi) = req.epub_progress else {
            return Ok(());
        };

        let progress = epub_progress(cfi, req.user.clone());
        self.lib
            .with_storage(move |s| s.set_book_progress(&req.id, &req.user, progress))
            .await?;
        Ok(())
    }

    pub async fn list_annotations(
        &self,
        req: ListAnnotationsRequest,
    ) -> Result<ListAnnotationsResponse, LibraryError> {
        validate_user(&req.user)?;
        self.require_book(&req.id).await?;
        let annotations = self
            .lib
            .with_storage(move |s| read_annotations(&s, &req.id, &req.user))
            .await?;
        Ok(ListAnnotationsResponse { annotations })
    }

    pub async fn add_annotation(&self, req: AddAnnotationRequest) -> Result<Annotation, LibraryError> {
        validate_user(&req.user)?;
        self.require_book(&req.id).await?;
        let (id, user, now) = (req.id.clone(), req.user.clone(), Time::now());
        self.change_annotations(id, user, move |list| Ok((annotations::add(list, req, now)?, true)))
            .await
    }

    pub async fn update_annotation(
        &self,
        req: UpdateAnnotationRequest,
    ) -> Result<Annotation, LibraryError> {
        validate_user(&req.user)?;
        self.require_book(&req.id).await?;
        let (id, user, now) = (req.id.clone(), req.user.clone(), Time::now());
        self.change_annotations(id, user, move |list| {
            Ok((annotations::update(list, req, now)?, true))
        })
        .await
    }

    pub async fn delete_annotation(&self, req: DeleteAnnotationRequest) -> Result<(), LibraryError> {
        validate_user(&req.user)?;
        self.require_book(&req.id).await?;
        let annotation_id = req.annotation_id;
        self.change_annotations(req.id, req.user, move |list| {
            Ok(((), annotations::remove(list, &annotation_id)))
        })
        .await
    }

    /// Runs `change` on the user's annotations of the book and writes them back if it says they
    /// changed. A file that can't be read fails the change, so it's never written over.
    async fn change_annotations<T: Send + 'static>(
        &self,
        id: String,
        user: String,
        change: impl FnOnce(&mut Vec<Annotation>) -> Result<(T, bool), LibraryError> + Send + 'static,
    ) -> Result<T, LibraryError> {
        let _guard = self.lib.annotations_lock.lock().await;
        self.lib
            .with_storage(move |s| {
                let mut list = read_annotations(&s, &id, &user)?;
                let (out, changed) = match change(&mut list) {
                    Ok(done) => done,
                    Err(e) => return Ok(Err(e)),
                };
                if changed {
                    let file = BookAnnotations { annotations: list, user: user.clone() };
                    s.set_book_annotations(&id, &user, &file)?;
                }
                Ok(Ok(out))
            })
            .await?
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
        self.set_last_book(&req.user, req.last_book_id).await
    }

    /// The user's progress and annotations for every book they can see, their last book, their
    /// settings and their own themes, without their name, so it can be imported as anyone. A
    /// book whose annotations file can't be read is exported without them.
    pub async fn export_user_data(
        &self,
        req: ExportUserDataRequest,
    ) -> Result<UserData, LibraryError> {
        validate_user(&req.user)?;
        let mut ids: Vec<String> = self.visible_ids().await.into_iter().collect();
        ids.sort();
        let user = req.user.clone();
        let (progress, annotations) = self
            .lib
            .with_storage(move |s| {
                let progress = ids
                    .iter()
                    .filter_map(|id| {
                        let epub_progress = read_cfi(&s, id, &user)?;
                        Some(BookProgressEntry { id: id.clone(), epub_progress })
                    })
                    .collect();
                let annotations = ids
                    .into_iter()
                    .filter_map(|id| match read_annotations(&s, &id, &user) {
                        Ok(annotations) if annotations.is_empty() => None,
                        Ok(annotations) => Some(BookAnnotationsEntry { id, annotations }),
                        Err(e) => {
                            warn!(%id, error = %format!("{e:#}"), "left unreadable annotations out of an export");
                            None
                        }
                    })
                    .collect();
                Ok((progress, annotations))
            })
            .await?;
        let state = GetStateRequest {
            user: req.user.clone(),
        };
        let last_book_id = self.get_state(state).await?.last_book_id;
        let config = self.lib.storage.read_config(&req.user).await?;
        let themes = themes::export_themes(&self.lib.storage, &req.user).await?;
        Ok(UserData {
            version: USER_DATA_VERSION,
            progress,
            last_book_id,
            config: Some(config),
            themes,
            annotations,
        })
    }

    /// Imported progress replaces the user's own and imported annotations are merged into theirs;
    /// books this view doesn't have are skipped.
    /// Only ids in the index are written, so none from the file becomes a path.
    pub async fn import_user_data(
        &self,
        req: ImportUserDataRequest,
    ) -> Result<ImportUserDataResponse, LibraryError> {
        validate_user(&req.user)?;
        let data = req.data.unwrap_or_default();
        if data.version != USER_DATA_VERSION {
            return Err(LibraryError::Invalid("this isn't a Shelf data export".to_owned()));
        }
        let visible = self.visible_ids().await;
        let total = data.progress.len();
        let entries: Vec<BookProgressEntry> = data
            .progress
            .into_iter()
            .filter(|e| !e.epub_progress.is_empty() && visible.contains(&e.id))
            .collect();
        let books = entries.len();

        let user = req.user.clone();
        self.lib
            .with_storage(move |s| {
                for entry in entries {
                    let progress = epub_progress(entry.epub_progress, user.clone());
                    s.set_book_progress(&entry.id, &user, progress)?;
                }
                Ok(())
            })
            .await?;
        let mut merged = 0;
        for entry in data.annotations.into_iter().filter(|e| visible.contains(&e.id)) {
            let user = req.user.clone();
            merged += self
                .change_annotations(entry.id, user, move |list| {
                    let written = annotations::merge(list, entry.annotations);
                    Ok((written as u32, written > 0))
                })
                .await?;
        }
        if let Some(id) = data.last_book_id
            && visible.contains(&id)
        {
            self.set_last_book(&req.user, Some(id)).await?;
        }
        let themes = {
            let _guard = self.lib.config_lock.lock().await;
            if let Some(config) = data.config {
                self.lib.storage.update_config(&req.user, &config).await?;
            }
            themes::import_themes(&self.lib.storage, &req.user, data.themes).await?
        };

        let skipped = total - books;
        info!(books, skipped, themes, annotations = merged, "imported user data");
        Ok(ImportUserDataResponse {
            books: books as u32,
            skipped: skipped as u32,
            themes,
            annotations: merged,
        })
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
            let rel = rel.clone();
            self.lib
                .with_storage(move |s| {
                    let id = sha1_hex(&book);
                    s.add_book(&rel, &book)?;
                    Ok(id)
                })
                .await?
        };
        let sync = IndexSync::new(self.lib.storage.clone(), self.lib.metadata.clone());
        let abs = self.lib.storage.library_root().join(&rel);
        let synced = self.lib.with_storage(move |_| sync.sync_path(&abs)).await;
        if let Err(e) = &synced {
            warn!(path = ?rel, error = %format!("{e:#}"), "uploaded book couldn't be extracted");
        }
        // This copy, not just the book: another copy may already be indexed.
        let path = rel.to_string_lossy().into_owned();
        let indexed = has_copy(&self.lib.metadata.read().await, &id, &path);
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
            dirs.push(self.library_string(&path));
        }
        let mut copies = Vec::new();
        for copy in &req.books {
            let path = folder_path(&copy.directory_path)?.join(entry_name(&copy.file_name)?);
            copies.push((copy.id.as_str(), self.library_string(&path)));
        }

        let _guard = self.lib.files_lock.lock().await;
        let (mut books, dirs) = {
            let index = self.lib.metadata.read().await;
            let mut books: Vec<String> = Vec::new();
            for (id, path) in copies {
                if has_copy(&index, id, &path) && self.scope.allows(&path) {
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

        let whole_dirs = self.split_partly_hidden(dirs, &mut books).await?;

        // Each success is kept even if a later delete fails, so the index matches the disk.
        let (gone_books, gone_dirs, failure) = self
            .lib
            .with_storage(move |s| {
                let mut failure = None;
                let gone_books = delete_each(books, |b| s.delete_book_file(b), &mut failure);
                let gone_dirs = delete_each(whole_dirs, |d| s.delete_folder(d), &mut failure);
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

    /// The folders to delete whole. One holding books the user can't see is kept, and only its
    /// visible books are added to `books`. Checked on disk, not in the index: a book not indexed
    /// yet must be kept too.
    async fn split_partly_hidden(
        &self,
        dirs: Vec<String>,
        books: &mut Vec<String>,
    ) -> Result<Vec<String>, LibraryError> {
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
        Ok(whole_dirs)
    }

    async fn set_last_book(&self, user: &str, id: Option<String>) -> Result<(), LibraryError> {
        let mut state = self.lib.storage.read_state(user).await?;
        state.last_book_id = id;
        self.lib.storage.write_state(user, &state).await?;
        Ok(())
    }

    async fn visible_ids(&self) -> HashSet<String> {
        let index = self.lib.metadata.read().await;
        index
            .iter()
            .filter(|b| self.scope.allows_book(b))
            .map(|b| b.id.clone())
            .collect()
    }

    async fn has_book(&self, id: &str) -> bool {
        self.lib
            .metadata
            .read()
            .await
            .iter()
            .any(|m| m.id == id && self.scope.allows_book(m))
    }

    async fn require_book(&self, id: &str) -> Result<(), LibraryError> {
        if self.has_book(id).await {
            Ok(())
        } else {
            Err(LibraryError::NotFound)
        }
    }

    /// A path relative to the user's root as a library path string.
    fn library_string(&self, client: &Path) -> String {
        self.scope.to_library(client).to_string_lossy().into_owned()
    }

    /// A folder path from the client as a library path; `Invalid` if it isn't a valid one.
    fn folder(&self, client: &str) -> Result<String, LibraryError> {
        Ok(self.library_string(&folder_path(client)?))
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
        self.shows_dir(rel).await
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

fn epub_progress(cfi: String, user: String) -> BookProgress {
    BookProgress {
        file_type: book_file_type::Value::Epub as i32,
        epub_progress: Some(EpubProgress { cfi }),
        user,
    }
}

/// The user's annotations in the book; an unreadable file is an error, unlike progress.
fn read_annotations(
    storage: &LocalLibraryStorage,
    id: &str,
    user: &str,
) -> anyhow::Result<Vec<Annotation>> {
    Ok(storage.book_annotations(id, user)?.map(|a| a.annotations).unwrap_or_default())
}

/// The book's saved position; an unreadable progress file counts as none.
fn read_cfi(storage: &LocalLibraryStorage, id: &str, user: &str) -> Option<String> {
    match storage.book_progress(id, user) {
        Ok(progress) => progress.and_then(|p| p.epub_progress).map(|e| e.cfi),
        Err(e) => {
            warn!(%id, error = %e, "unreadable progress file");
            None
        }
    }
}

/// Deletes each path with `delete`, keeping the first error, and returns the ones deleted.
fn delete_each(
    paths: Vec<String>,
    delete: impl Fn(&Path) -> anyhow::Result<()>,
    failure: &mut Option<anyhow::Error>,
) -> Vec<String> {
    paths
        .into_iter()
        .filter(|p| match delete(Path::new(p)) {
            Ok(()) => true,
            Err(e) => {
                failure.get_or_insert(e);
                false
            }
        })
        .collect()
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
    use crate::testutil::{scope, temp_library};
    use crate::{
        library::{Library, scope::EVERYTHING},
        storage::LocalLibraryStorage,
    };

    /// Book files whose `metadata.json` is already in `.shelf/`, so they're refreshed rather
    /// than parsed (they aren't real EPUBs). The contents give each book its id.
    async fn library(books: &[(&str, &str)]) -> (tempfile::TempDir, BookLibrary) {
        let (root, storage) = temp_library();
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

    async fn names(view: &LibraryView<'_>, dir: &str) -> (Vec<String>, Vec<String>) {
        let books = view
            .list_books(ListBooksRequest {
                directory_path: dir.into(),
            })
            .await
            .unwrap();
        let folders = view
            .list_sub_directories(ListDirectoriesRequest {
                directory_path: dir.into(),
            })
            .await
            .unwrap();
        let books = books.books.into_iter().map(|b| b.file_name).collect();
        (books, folders.sub_directories)
    }

    async fn save_progress(view: &LibraryView<'_>, user: &str, contents: &str, cfi: &str) {
        let req = SetProgressRequest {
            id: sha1_hex(contents.as_bytes()),
            epub_progress: Some(cfi.into()),
            user: user.into(),
        };
        view.set_progress(req).await.unwrap();
    }

    async fn progress_of(view: &LibraryView<'_>, user: &str, contents: &str) -> Option<String> {
        let req = GetProgressRequest {
            id: sha1_hex(contents.as_bytes()),
            user: user.into(),
        };
        view.get_progress(req).await.unwrap().epub_progress
    }

    fn export(user: &str) -> ExportUserDataRequest {
        ExportUserDataRequest { user: user.into() }
    }

    fn import(user: &str, data: UserData) -> ImportUserDataRequest {
        ImportUserDataRequest {
            user: user.into(),
            data: Some(data),
        }
    }

    #[tokio::test]
    async fn user_data_moves_to_another_library_as_another_user() {
        let (_from_root, from) = library(&[("a.epub", "a"), ("b.epub", "b")]).await;
        let view = from.view(&EVERYTHING);
        save_progress(&view, "parth", "a", "cfi-a").await;
        save_progress(&view, "parth", "b", "cfi-b").await;
        let state = SetStateRequest {
            last_book_id: Some(sha1_hex(b"a")),
            user: "parth".into(),
        };
        view.set_state(state).await.unwrap();
        let patch = serde_json::from_str(r#"{ "ui": { "theme": "DARK" } }"#).unwrap();
        let config = shelf_proto::api::SetConfigRequest {
            user: "parth".into(),
            patch: Some(patch),
        };
        from.set_config(config).await.unwrap();
        let spec = serde_json::from_str(r#"{ "name": "Calm", "layers": [{ "invert": {} }] }"#);
        let theme = shelf_proto::api::SetThemeRequest {
            user: "parth".into(),
            spec: Some(spec.unwrap()),
        };
        from.set_theme(theme).await.unwrap();

        let data = view.export_user_data(export("parth")).await.unwrap();
        assert_eq!(data.progress.len(), 2);
        assert_eq!(data.themes.len(), 1);
        let json = serde_json::to_string(&data).unwrap();
        assert!(!json.contains("parth"), "{json}");

        // Same `a`, a different `c`, and `kid` already has progress in `a`.
        let (_to_root, to) = library(&[("other/a.epub", "a"), ("c.epub", "c")]).await;
        let view = to.view(&EVERYTHING);
        save_progress(&view, "kid", "a", "old").await;
        let res = view.import_user_data(import("kid", data)).await.unwrap();
        assert_eq!((res.books, res.skipped, res.themes), (1, 1, 1));

        assert_eq!(progress_of(&view, "kid", "a").await.as_deref(), Some("cfi-a"));
        let state = GetStateRequest { user: "kid".into() };
        assert_eq!(view.get_state(state).await.unwrap().last_book_id, Some(sha1_hex(b"a")));
        let config = to.get_config(shelf_proto::api::GetConfigRequest { user: "kid".into() });
        let config = serde_json::to_value(config.await.unwrap()).unwrap();
        assert_eq!(config["ui"]["theme"], "DARK");
        let themes = to.list_themes(shelf_proto::api::ListThemesRequest { user: "kid".into() });
        assert!(themes.await.unwrap().themes.iter().any(|t| t.id == "calm"));
    }

    #[tokio::test]
    async fn renaming_the_chosen_theme_moves_the_config() {
        use shelf_proto::api::{GetConfigRequest, RenameThemeRequest, SetConfigRequest, SetThemeRequest};
        let (_root, lib) = library(&[]).await;
        let spec = serde_json::from_str(r#"{ "name": "Calm" }"#).unwrap();
        let user = || "parth".to_owned();
        lib.set_theme(SetThemeRequest { user: user(), spec: Some(spec) }).await.unwrap();
        let patch = serde_json::from_str(r#"{ "reader": { "theme": "calm" } }"#).unwrap();
        lib.set_config(SetConfigRequest { user: user(), patch: Some(patch) }).await.unwrap();

        let rename = RenameThemeRequest { user: user(), from: "Calm".into(), to: "Very Calm".into() };
        let list = lib.rename_theme(rename).await.unwrap();
        assert!(list.themes.iter().any(|t| t.id == "very-calm"));
        let config = lib.get_config(GetConfigRequest { user: user() }).await.unwrap();
        assert_eq!(config.reader.unwrap().theme.as_deref(), Some("very-calm"));
    }

    #[tokio::test]
    async fn user_data_follows_the_scope_both_ways() {
        let (_root, lib) = library(&[("x.epub", "x"), ("hidden/y.epub", "y")]).await;
        let everything = lib.view(&EVERYTHING);
        save_progress(&everything, "a", "x", "cfi-x").await;
        save_progress(&everything, "a", "y", "cfi-y").await;
        let scoped = scope("", &["!hidden/**"]);
        let view = lib.view(&scoped);

        let data = view.export_user_data(export("a")).await.unwrap();
        let ids: Vec<_> = data.progress.iter().map(|e| e.id.clone()).collect();
        assert_eq!(ids, [sha1_hex(b"x")]);

        let mut data = everything.export_user_data(export("a")).await.unwrap();
        data.config = None;
        let res = view.import_user_data(import("b", data)).await.unwrap();
        assert_eq!((res.books, res.skipped), (1, 1));
        assert_eq!(progress_of(&everything, "b", "y").await, None);
    }

    #[tokio::test]
    async fn only_version_1_user_data_is_imported() {
        let (_root, lib) = library(&[("x.epub", "x")]).await;
        let view = lib.view(&EVERYTHING);
        let data = UserData::default();
        let res = view.import_user_data(import("a", data)).await;
        assert!(matches!(res, Err(LibraryError::Invalid(_))));
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
        let unsafe_dir = ListBooksRequest {
            directory_path: "../other".into(),
        };
        assert!(matches!(view.list_books(unsafe_dir).await, Err(LibraryError::Invalid(_))));
        let unsafe_dir = ListDirectoriesRequest {
            directory_path: "a/../..".into(),
        };
        let folders = view.list_sub_directories(unsafe_dir).await;
        assert!(matches!(folders, Err(LibraryError::Invalid(_))));

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
        let (_root, lib) = library(&[("a.epub", "a")]).await;
        let storage = &lib.storage;
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

    fn highlight_in(user: &str, contents: &str, cfi: &str) -> AddAnnotationRequest {
        AddAnnotationRequest {
            id: sha1_hex(contents.as_bytes()),
            cfi: cfi.into(),
            text: "text".into(),
            user: user.into(),
            ..Default::default()
        }
    }

    async fn annotations_of(view: &LibraryView<'_>, user: &str, contents: &str) -> Vec<Annotation> {
        let req = ListAnnotationsRequest {
            id: sha1_hex(contents.as_bytes()),
            user: user.into(),
        };
        view.list_annotations(req).await.unwrap().annotations
    }

    #[tokio::test]
    async fn annotations_are_per_user_and_change_one_at_a_time() {
        let (_root, lib) = library(&[("a.epub", "a")]).await;
        let view = lib.view(&EVERYTHING);
        let first = view.add_annotation(highlight_in("parth", "a", "epubcfi(/6/2)")).await.unwrap();
        let second = view.add_annotation(highlight_in("parth", "a", "epubcfi(/6/4)")).await.unwrap();
        view.add_annotation(highlight_in("kid", "a", "epubcfi(/6/6)")).await.unwrap();

        let update = UpdateAnnotationRequest {
            id: sha1_hex(b"a"),
            annotation_id: first.id.clone(),
            note: Some("why".into()),
            color: None,
            user: "parth".into(),
        };
        let noted = view.update_annotation(update.clone()).await.unwrap();
        assert_eq!(noted.note, "why");
        let not_theirs = UpdateAnnotationRequest { user: "kid".into(), ..update };
        assert!(matches!(view.update_annotation(not_theirs).await, Err(LibraryError::NotFound)));

        let delete = DeleteAnnotationRequest {
            id: sha1_hex(b"a"),
            annotation_id: second.id,
            user: "parth".into(),
        };
        view.delete_annotation(delete.clone()).await.unwrap();
        view.delete_annotation(delete).await.unwrap();

        assert_eq!(annotations_of(&view, "parth", "a").await, [noted]);
        assert_eq!(annotations_of(&view, "kid", "a").await.len(), 1);
        assert!(annotations_of(&view, "nobody", "a").await.is_empty());
    }

    #[tokio::test]
    async fn annotations_of_a_hidden_book_are_not_found() {
        let (_root, lib) = library(&[("x.epub", "x"), ("hidden/y.epub", "y")]).await;
        let everything = lib.view(&EVERYTHING);
        let stored = everything.add_annotation(highlight_in("a", "y", "epubcfi(/6/2)")).await;
        let stored = stored.unwrap();
        let scoped = scope("", &["!hidden/**"]);
        let view = lib.view(&scoped);
        let id = sha1_hex(b"y");

        let list = ListAnnotationsRequest { id: id.clone(), user: "a".into() };
        assert!(matches!(view.list_annotations(list).await, Err(LibraryError::NotFound)));
        let add = view.add_annotation(highlight_in("a", "y", "epubcfi(/6/4)")).await;
        assert!(matches!(add, Err(LibraryError::NotFound)));
        let update = UpdateAnnotationRequest {
            id: id.clone(),
            annotation_id: stored.id.clone(),
            note: Some("x".into()),
            color: None,
            user: "a".into(),
        };
        assert!(matches!(view.update_annotation(update).await, Err(LibraryError::NotFound)));
        let delete = DeleteAnnotationRequest {
            id,
            annotation_id: stored.id.clone(),
            user: "a".into(),
        };
        assert!(matches!(view.delete_annotation(delete).await, Err(LibraryError::NotFound)));
        assert_eq!(annotations_of(&everything, "a", "y").await, [stored]);
    }

    #[tokio::test]
    async fn an_unreadable_annotations_file_is_never_written_over() {
        let (_root, lib) = library(&[("a.epub", "a")]).await;
        let view = lib.view(&EVERYTHING);
        let file = lib.storage.book_dir_by_id(&sha1_hex(b"a")).join("annotations/parth.json");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "{ not json").unwrap();

        let list = ListAnnotationsRequest { id: sha1_hex(b"a"), user: "parth".into() };
        assert!(matches!(view.list_annotations(list).await, Err(LibraryError::Internal(_))));
        let add = view.add_annotation(highlight_in("parth", "a", "epubcfi(/6/2)")).await;
        assert!(matches!(add, Err(LibraryError::Internal(_))));
        let data = view.export_user_data(export("parth")).await.unwrap();
        assert!(data.annotations.is_empty());
        assert_eq!(fs::read_to_string(&file).unwrap(), "{ not json");
    }

    #[tokio::test]
    async fn annotations_move_with_user_data_and_merge() {
        let (_from_root, from) = library(&[("a.epub", "a")]).await;
        let view = from.view(&EVERYTHING);
        let theirs = view.add_annotation(highlight_in("parth", "a", "epubcfi(/6/2)")).await.unwrap();
        let data = view.export_user_data(export("parth")).await.unwrap();
        assert_eq!(data.annotations.len(), 1);
        assert_eq!(data.annotations[0].annotations, [theirs.clone()]);

        let (_to_root, to) = library(&[("other/a.epub", "a")]).await;
        let view = to.view(&EVERYTHING);
        let own = view.add_annotation(highlight_in("kid", "a", "epubcfi(/6/4)")).await.unwrap();
        let res = view.import_user_data(import("kid", data.clone())).await.unwrap();
        assert_eq!(res.annotations, 1);
        let again = view.import_user_data(import("kid", data)).await.unwrap();
        assert_eq!(again.annotations, 0);

        let merged = annotations_of(&view, "kid", "a").await;
        assert_eq!(merged.len(), 2);
        assert!(merged.contains(&theirs) && merged.contains(&own));
    }
}
