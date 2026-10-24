use std::path::{Component, Path, PathBuf};

use anyhow::Ok;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher, recommended_watcher};
use tokio::sync::mpsc::{self, UnboundedReceiver};

pub struct LibraryWatcher {
    inner: RecommendedWatcher,
    root: PathBuf,
}

impl LibraryWatcher {
    /// Must be called from inside a tokio runtime.
    pub fn initialize(root: &Path) -> anyhow::Result<(Self, UnboundedReceiver<PathBuf>)> {
        let root = root.canonicalize()?;
        let root_len = root.components().count();
        let (sender, receiver) = mpsc::unbounded_channel();
        let inner = recommended_watcher(move |res: Result<Event, notify::Error>| {
            if res.is_err() {
                return;
            }
            let res = res.unwrap();
            if !(res.kind.is_create() || res.kind.is_remove() || res.kind.is_modify()) {
                return;
            }
            // TODO: add granular controls to the modification kind here
            let paths = res.paths;
            let paths: Vec<PathBuf> = paths.into_iter().filter(|p| Self::is_relevant(p, root_len)).collect();
            for path in paths {
                let _ = sender.send(path);
            }
        })?;
        Ok((Self { inner, root }, receiver))
    }

    pub async fn watch(&mut self) -> anyhow::Result<()> {
        self.inner.watch(&self.root, RecursiveMode::Recursive)?;
        Ok(())
    }

    /// `.epub` files that aren't hidden or inside a dot-folder such as `.shelf/`.
    fn is_relevant(path: &Path, root_len: usize) -> bool {
        let all_visible = path.components().skip(root_len).all(|c| match c {
            Component::Normal(name) => !name.to_string_lossy().starts_with('.'),
            _ => false,
        });
        let is_epub = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("epub"));
        all_visible && is_epub
    }
}
