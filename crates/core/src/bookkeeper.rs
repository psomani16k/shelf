use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::Ok;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use shelf_proto::bookkeeping::{BookMetadata, BookProgress};
use std::fs::{self};

#[derive(Debug, Clone)]
pub struct BookKeeper {
    root: PathBuf,
}

impl BookKeeper {
    pub fn new(root: &Path) -> anyhow::Result<Self> {
        let root = root.canonicalize()?;
        Ok(Self { root })
    }

    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    pub fn books(&self) -> PathBuf {
        self.root.join(".shelf/books")
    }

    pub fn book_dir(&self, hash: &str) -> PathBuf {
        self.root.join(format!(".shelf/books/{}", hash))
    }

    pub fn all_book_hashes(&self) -> anyhow::Result<HashSet<String>> {
        let book_dir = self.books();
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

    pub fn all_book_file_paths_internal(dir: &Path) -> anyhow::Result<HashSet<PathBuf>> {
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

    pub fn set_book_metadata(&self, hash: &str, metadata: BookMetadata) -> anyhow::Result<()> {
        let book_dir = self.book_dir(hash);
        let meta_file = book_dir.join("metadata.json");
        fs::write(meta_file, serde_json::to_string_pretty(&metadata)?)?;
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
        let progress_file = book_dir.join("metadata.json");
        fs::write(progress_file, serde_json::to_string_pretty(&progress)?)?;
        Ok(())
    }

    pub fn book_thumbnail(&self, hash: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let book_dir = self.book_dir(hash);
        let thumbnail_file = book_dir.join("thumbnail.jpeg");
        if thumbnail_file.exists() {
            let t = fs::read(&thumbnail_file)?;
            return Ok(Some(t));
        }
        Ok(None)
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
            let _ = fs::remove_dir(p);
        });
        Ok(())
    }
}
