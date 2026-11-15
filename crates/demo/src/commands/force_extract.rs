use std::path::PathBuf;

use shelf_core::{
    routines::extractor::{SyncReport, sync_library},
    storage::LocalLibraryStorage,
};

command! {
    /// Treats `root` as a library and runs the startup sync with `force`, as *Rebuild index*
    /// does: every book is re-extracted into `<root>/.shelf/`, and the metadata and thumbnails
    /// of books no file produced are deleted (their folders and progress stay).
    pub async fn force_extract(&self, root: PathBuf) -> anyhow::Result<SyncReport> {
        let storage = tokio::task::spawn_blocking(move || LocalLibraryStorage::new(&root)).await??;
        sync_library(&storage, true).await
    }
}
