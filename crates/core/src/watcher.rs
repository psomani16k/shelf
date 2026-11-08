use std::path::{Component, Path, PathBuf};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher, recommended_watcher};
use tokio::sync::mpsc::{self, UnboundedReceiver};

use crate::bookkeeper::is_epub;

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
            if let Ok(r) = res
                && (r.kind.is_create() || r.kind.is_remove() || r.kind.is_modify())
            {
                let paths = r.paths;
                let paths: Vec<PathBuf> = paths
                    .into_iter()
                    .filter(|p| Self::is_relevant(p, root_len))
                    .collect();
                for path in paths {
                    let _ = sender.send(path);
                }
            }
        })?;
        anyhow::Ok((Self { inner, root }, receiver))
    }

    pub async fn watch(&mut self) -> anyhow::Result<()> {
        self.inner.watch(&self.root, RecursiveMode::Recursive)?;
        Ok(())
    }

    /// `.epub` files and folders below the root that aren't hidden or inside a dot-folder such as
    /// `.shelf/`. A gone path can't be checked, so it passes too: it may have been a folder.
    fn is_relevant(path: &Path, root_len: usize) -> bool {
        let below_root = path.components().count() > root_len;
        let all_visible = path.components().skip(root_len).all(|c| match c {
            Component::Normal(name) => !name.to_string_lossy().starts_with('.'),
            _ => false,
        });
        below_root && all_visible && (is_epub(path) || !path.is_file())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};

    use super::*;

    #[test]
    fn passes_books_folders_and_gone_paths_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        File::create(root.join("dune.epub")).unwrap();
        File::create(root.join("notes.txt")).unwrap();
        fs::create_dir(root.join("scifi")).unwrap();
        let root_len = root.components().count();
        let relevant = |rel: &str| LibraryWatcher::is_relevant(&root.join(rel), root_len);

        assert!(relevant("dune.epub"));
        assert!(relevant("scifi"));
        assert!(relevant("gone"));
        assert!(!relevant("notes.txt"));
        assert!(!relevant(".shelf/books"));
        assert!(!relevant(".hidden/dune.epub"));
        assert!(!LibraryWatcher::is_relevant(&root, root_len));
    }
}
