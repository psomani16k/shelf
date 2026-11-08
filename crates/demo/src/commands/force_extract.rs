use std::path::PathBuf;

use shelf_core::{
    bookkeeper::BookKeeper,
    routines::extractor::{SyncReport, sync_library},
};

command! {
    /// Treats `root` as a library and runs the startup sync with `force`, as *Rebuild index*
    /// does: every book is re-extracted into `<root>/.shelf/`, and folders no book produced
    /// are deleted, progress included.
    pub async fn force_extract(&self, root: PathBuf) -> anyhow::Result<SyncReport> {
        let keeper = tokio::task::spawn_blocking(move || BookKeeper::new(&root)).await??;
        sync_library(&keeper, true).await
    }
}
