use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, anyhow};

use rayon::iter::{IntoParallelIterator, ParallelIterator};
use shelf_proto::{
    api::{
        GetProgressRequest, GetProgressResponse, GetStateRequest, GetStateResponse,
        ListBooksRequest, ListBooksResponse, ListBooksResponseItem, ListDirectoriesRequest,
        ListDirectoriesResponse, SetProgressRequest, SetStateRequest,
    },
    bookkeeping::{BookMetadata, BookProgress, EpubProgress, book_file_type},
    config::Config,
};
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

use crate::{
    bookkeeper::BookKeeper,
    config::{read_config, update_config},
    routines::extractor::{extractor, sync_library},
    state::{clean_up_temp_files, read_state, write_state},
    traits::{Library, LibraryError},
    watcher::LibraryWatcher,
};

pub struct BookLibrary {
    keeper: BookKeeper,
    metadata: Arc<RwLock<Vec<BookMetadata>>>,
    // Two concurrent sets would each read the old file, and one would lose the other's fields.
    config_lock: Mutex<()>,
    // Dropping it closes the extractor's channel, which ends the extractor task.
    _watcher: LibraryWatcher,
}

impl BookLibrary {
    /// Starts the watcher, syncs `.shelf/` with the folder, loads every book's metadata, then
    /// starts the extractor, which calls `on_change` after each book it syncs. Must run inside tokio.
    pub async fn open(
        root: PathBuf,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> anyhow::Result<Self> {
        if let Err(e) = clean_up_temp_files(&root).await {
            warn!(error = %e, "couldn't remove leftover temp files");
        }
        let (mut watcher, recv) = LibraryWatcher::initialize(&root)?;
        watcher.watch().await?;
        // Events that arrive during the sync wait in `recv` until the extractor starts.
        let (root, keeper, metadata) = tokio::task::spawn_blocking(move || {
            sync_library(&root)?;
            let keeper = BookKeeper::new(&root)?;
            let metadata = load_metadata(&keeper)?;
            anyhow::Ok((root, keeper, metadata))
        })
        .await??;
        info!(root = %root.display(), books = metadata.len(), "library opened");

        let metadata = Arc::new(RwLock::new(metadata));
        tokio::spawn(extractor(root, recv, metadata.clone(), on_change));
        Ok(Self {
            keeper,
            metadata,
            config_lock: Mutex::new(()),
            _watcher: watcher,
        })
    }

    /// Ids index into `.shelf/books/`, so only known ones may reach the disk (no `../` tricks).
    async fn has_book(&self, id: &str) -> bool {
        self.metadata.read().await.iter().any(|m| m.id == id)
    }

    /// Runs blocking `BookKeeper` file I/O off the async worker threads.
    async fn with_keeper<T: Send + 'static>(
        &self,
        f: impl FnOnce(BookKeeper) -> anyhow::Result<T> + Send + 'static,
    ) -> anyhow::Result<T> {
        let keeper = self.keeper.clone();
        tokio::task::spawn_blocking(move || f(keeper)).await?
    }
}

/// Reads every `.shelf/books/*/metadata.json`. Blocks: call it inside `spawn_blocking`.
fn load_metadata(keeper: &BookKeeper) -> anyhow::Result<Vec<BookMetadata>> {
    Ok(keeper
        .all_book_hashes()?
        .into_par_iter()
        .filter_map(|hash| match keeper.book_metadata(&hash) {
            Ok(meta) => meta,
            Err(e) => {
                warn!(id = %hash, error = %e, "skipped unreadable metadata");
                None
            }
        })
        .collect())
}

/// Returns the folder with slashes trimmed (`""` is the root) and the prefix its contents' paths start with.
fn folder_prefix(path: &str) -> (&str, String) {
    let dir = path.trim_matches('/');
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    (dir, prefix)
}

impl Library for BookLibrary {
    async fn list_books(&self, req: ListBooksRequest) -> ListBooksResponse {
        let (dir, prefix) = folder_prefix(&req.directory_path);

        let mut books: Vec<_> = self
            .metadata
            .read()
            .await
            .iter()
            .filter_map(|m| {
                let file_name = m.path.strip_prefix(&prefix)?;
                // Books in subfolders belong to that subfolder's listing.
                if file_name.contains('/') {
                    return None;
                }
                Some(ListBooksResponseItem {
                    id: m.id.clone(),
                    file_name: file_name.to_owned(),
                    title: m.title.clone(),
                    authors: m.authors.clone(),
                })
            })
            .collect();
        books.sort_by_cached_key(|b| b.title.as_deref().unwrap_or(&b.file_name).to_lowercase());

        ListBooksResponse {
            current_directory: dir.to_owned(),
            books,
        }
    }

    async fn list_sub_directories(&self, req: ListDirectoriesRequest) -> ListDirectoriesResponse {
        let (dir, prefix) = folder_prefix(&req.directory_path);

        let metadata = self.metadata.read().await;
        let subfolders: BTreeSet<&str> = metadata
            .iter()
            .filter_map(|m| m.path.strip_prefix(&prefix)?.split_once('/'))
            .map(|(subfolder, _)| subfolder)
            .collect();
        let mut sub_directories: Vec<String> = subfolders.into_iter().map(String::from).collect();
        sub_directories.sort_by_cached_key(|d| d.to_lowercase());

        ListDirectoriesResponse {
            current_directory: dir.to_owned(),
            sub_directories,
        }
    }

    async fn get_cover(&self, id: &str) -> Result<PathBuf, LibraryError> {
        if !self.has_book(id).await {
            return Err(LibraryError::NotFound);
        }
        let path = self.keeper.book_thumbnail_path(id);
        let exists = tokio::fs::try_exists(&path)
            .await
            .with_context(|| format!("couldn't check {}", path.display()))?;
        // A book without a cover image has no thumbnail.
        if exists {
            Ok(path)
        } else {
            Err(LibraryError::NotFound)
        }
    }

    async fn get_book(&self, id: &str) -> Result<PathBuf, LibraryError> {
        let metadata = self.metadata.read().await;
        let book = metadata
            .iter()
            .find(|m| m.id == id)
            .ok_or(LibraryError::NotFound)?;
        // `path` comes from synced `.shelf/` data: an absolute path or `..` would escape the library.
        let rel = Path::new(&book.path);
        if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
            return Err(anyhow!("book {id} has an unsafe path: {}", book.path).into());
        }
        Ok(self.keeper.root().join(rel))
    }

    async fn get_progress(
        &self,
        req: GetProgressRequest,
    ) -> Result<GetProgressResponse, LibraryError> {
        if !self.has_book(&req.id).await {
            return Err(LibraryError::NotFound);
        }

        let id = req.id.clone();
        let epub_progress = match self.with_keeper(move |k| k.book_progress(&id)).await {
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

    async fn set_progress(&self, req: SetProgressRequest) -> Result<(), LibraryError> {
        if !self.has_book(&req.id).await {
            return Err(LibraryError::NotFound);
        }
        let Some(cfi) = req.epub_progress else {
            return Ok(());
        };

        let progress = BookProgress {
            file_type: book_file_type::Value::Epub as i32,
            epub_progress: Some(EpubProgress { cfi }),
        };
        self.with_keeper(move |k| k.set_book_progress(&req.id, progress))
            .await?;
        Ok(())
    }

    async fn get_state(&self, _req: GetStateRequest) -> Result<GetStateResponse, LibraryError> {
        let state = read_state(self.keeper.root()).await?;
        let last_book_id = match state.last_book_id {
            Some(id) if self.has_book(&id).await => Some(id),
            _ => None,
        };
        Ok(GetStateResponse { last_book_id })
    }

    async fn set_state(&self, req: SetStateRequest) -> Result<(), LibraryError> {
        if let Some(id) = &req.last_book_id
            && !self.has_book(id).await
        {
            return Err(LibraryError::NotFound);
        }
        // Read first, so fields this server doesn't know about survive the write.
        let mut state = read_state(self.keeper.root()).await?;
        state.last_book_id = req.last_book_id;
        write_state(self.keeper.root(), &state).await?;
        Ok(())
    }

    async fn get_config(&self) -> Result<Config, LibraryError> {
        Ok(read_config(self.keeper.root()).await?)
    }

    async fn set_config(&self, patch: Config) -> Result<Config, LibraryError> {
        let _guard = self.config_lock.lock().await;
        Ok(update_config(self.keeper.root(), &patch).await?)
    }
}
