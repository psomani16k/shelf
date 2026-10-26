use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::Ok;
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use shelf_proto::bookkeeping::{BookMetadata, BookProgress};
use std::fs::{self};
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookKeeper {
    root: PathBuf,
}

impl BookKeeper {
    pub fn new(root: &Path) -> anyhow::Result<Self> {
        let root = root.canonicalize()?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn books(&self) -> PathBuf {
        self.root.join(".shelf/books")
    }

    pub fn book_dir(&self, hash: &str) -> PathBuf {
        self.root.join(format!(".shelf/books/{}", hash))
    }

    pub fn all_book_hashes(&self) -> anyhow::Result<HashSet<String>> {
        let book_dir = self.books();
        if !book_dir.exists() {
            return Ok(HashSet::new());
        }
        let contents = fs::read_dir(book_dir)?;
        let mut res = HashSet::new();
        for entry in contents {
            let entry = entry?;
            res.insert(entry.file_name().to_string_lossy().to_string());
        }
        Ok(res)
    }

    pub fn all_book_file_paths(&self) -> anyhow::Result<HashSet<PathBuf>> {
        Self::all_book_file_paths_internal(&self.root)
    }

    fn all_book_file_paths_internal(dir: &Path) -> anyhow::Result<HashSet<PathBuf>> {
        let mut res = HashSet::new();
        let content = std::fs::read_dir(dir)?;
        for p in content {
            let p = p?;
            if p.file_name().to_str().is_some_and(|f| f.starts_with(".")) {
                continue;
            }
            let t = p.file_type()?;
            if t.is_dir() {
                let e = Self::all_book_file_paths_internal(&p.path())?;
                res.extend(e);
            }
            if t.is_file()
                && p.path()
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("epub"))
            {
                res.insert(p.path());
            }
        }
        return Ok(res);
    }

    pub fn book_metadata(&self, hash: &str) -> anyhow::Result<Option<BookMetadata>> {
        let book_dir = self.book_dir(hash);
        let meta_file = book_dir.join("metadata.json");
        if meta_file.exists() {
            let meta = fs::read_to_string(&meta_file)?;
            let meta = serde_json::from_str(&meta)?;
            return Ok(Some(meta));
        }
        Ok(None)
    }

    /// Reads every `.shelf/books/*/metadata.json`, skipping unreadable ones with a warning.
    pub fn all_book_metadata(&self) -> anyhow::Result<Vec<BookMetadata>> {
        Ok(self
            .all_book_hashes()?
            .into_par_iter()
            .filter_map(|hash| match self.book_metadata(&hash) {
                std::result::Result::Ok(meta) => meta,
                Err(e) => {
                    warn!(id = %hash, error = %e, "skipped unreadable metadata");
                    None
                }
            })
            .collect())
    }

    pub fn set_book_metadata(&self, hash: &str, metadata: &BookMetadata) -> anyhow::Result<()> {
        let book_dir = self.book_dir(hash);
        let meta_file = book_dir.join("metadata.json");
        fs::write(meta_file, serde_json::to_string_pretty(metadata)?)?;
        Ok(())
    }

    pub fn book_progress(&self, hash: &str) -> anyhow::Result<Option<BookProgress>> {
        let book_dir = self.book_dir(hash);
        let progress_file = book_dir.join("progress.json");
        if progress_file.exists() {
            let p = fs::read_to_string(&progress_file)?;
            let p = serde_json::from_str(&p)?;
            return Ok(p);
        }
        Ok(None)
    }

    pub fn set_book_progress(&self, hash: &str, progress: BookProgress) -> anyhow::Result<()> {
        let book_dir = self.book_dir(hash);
        let progress_file = book_dir.join("progress.json");
        fs::write(progress_file, serde_json::to_string_pretty(&progress)?)?;
        Ok(())
    }

    pub fn book_thumbnail_path(&self, hash: &str) -> PathBuf {
        self.book_dir(hash).join("thumbnail.jpeg")
    }

    pub fn set_book_thumbnail(&self, hash: &str, thumbnail: &[u8]) -> anyhow::Result<()> {
        let book_dir = self.book_dir(hash);
        let thumbnail_file = book_dir.join("thumbnail.jpeg");
        fs::write(thumbnail_file, thumbnail)?;
        Ok(())
    }

    pub fn book_cleaned_path(&self, book: &Path) -> anyhow::Result<PathBuf> {
        let mut book = book.canonicalize()?;
        if book.starts_with(&self.root) {
            book = book.strip_prefix(&self.root)?.to_path_buf();
        }
        Ok(book)
    }

    pub fn clean_up_hashes(&self, hashes: HashSet<String>) -> anyhow::Result<()> {
        let paths: Vec<_> = hashes.iter().map(|h| self.book_dir(&h)).collect();
        paths.par_iter().for_each(|p| {
            let _ = fs::remove_dir_all(p);
        });
        Ok(())
    }
}
