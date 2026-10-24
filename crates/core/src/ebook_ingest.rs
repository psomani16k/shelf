use std::path::{Path, PathBuf};

use anyhow::Ok;
use fast_image_resize::{ResizeOptions, Resizer};
use image::{DynamicImage, codecs::jpeg::JpegEncoder};
use rbook::Epub;
use sha1::{Digest, Sha1};
use shelf_proto::bookkeeping::{BookMetadata, book_file_type};
use std::fs::{self};

use crate::bookkeeper::BookKeeper;

pub struct EbookExtractor {
    book_keeper: BookKeeper,
    relative_path: PathBuf,
}

impl EbookExtractor {
    // Standard EPUB cover ratio is 1:1.6.
    const THUMBNAIL_WIDTH: u32 = 400;
    const THUMBNAIL_HEIGHT: u32 = 640;
    const THUMBNAIL_JPEG_QUALITY: u8 = 85;

    pub fn new(root: &Path, book: &Path) -> anyhow::Result<Self> {
        let book_keeper = BookKeeper::new(root)?;
        let book = book_keeper.book_cleaned_path(book)?;
        return Ok(Self {
            book_keeper,
            relative_path: book,
        });
    }

    pub fn extract(&self) -> anyhow::Result<String> {
        let book = self.book_keeper.root().join(&self.relative_path);
        let file = fs::read(&book)?;
        let mut hasher = Sha1::new();
        hasher.update(&file);
        let hash: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        self.extract_epub(&hash)?;
        Ok(hash)
    }

    fn extract_epub(&self, hash: &str) -> anyhow::Result<()> {
        let book_dir = self.book_keeper.book_dir(hash);
        if book_dir.exists() {
            let mut meta =
                self.book_keeper
                    .book_metadata(hash)?
                    .ok_or_else(|| -> anyhow::Error {
                        anyhow::format_err!(
                            "Expected metadata to exist for book {:?}",
                            self.relative_path
                        )
                    })?;
            if meta.path != self.relative_path.to_string_lossy().to_string() {
                meta.path = self.relative_path.to_string_lossy().to_string();
                self.book_keeper.set_book_metadata(hash, meta)?;
            }
            return Ok(());
        }
        fs::create_dir_all(book_dir)?;

        let book = Epub::open(self.book_keeper.root().join(&self.relative_path))?;
        let metadata = book.metadata();
        let title = match metadata.title() {
            Some(t) => Some(t.to_string()),
            None => None,
        };
        let authors: Vec<_> = metadata.creators().collect();
        let authors = authors
            .iter()
            .map(|a| -> String { a.to_string() })
            .collect();
        let metadata = BookMetadata {
            authors: authors,
            title: title,
            path: self.relative_path.to_string_lossy().to_string(),
            file_type: book_file_type::Value::Epub as i32,
        };
        self.book_keeper.set_book_metadata(hash, metadata)?;

        // TODO: write a zero progress file for the book

        let cover = book.manifest().cover_image();
        if cover.is_some() {
            let cover = cover.unwrap();
            let cover_bytes = cover.read_bytes()?;
            let thumbnail = self.resize_image_to_thumbnail(&cover_bytes)?;
            self.book_keeper.set_book_thumbnail(hash, &thumbnail)?;
        }

        Ok(())
    }

    /// Center-crops to the cover ratio, then scales. CPU-heavy: call it inside `spawn_blocking`.
    fn resize_image_to_thumbnail(&self, image: &[u8]) -> anyhow::Result<Vec<u8>> {
        let src = DynamicImage::ImageRgb8(image::load_from_memory(image)?.into_rgb8());
        let mut dst = DynamicImage::new_rgb8(Self::THUMBNAIL_WIDTH, Self::THUMBNAIL_HEIGHT);
        let options = ResizeOptions::new().fit_into_destination(Some((0.5, 0.5)));
        Resizer::new().resize(&src, &mut dst, &options)?;

        let mut jpeg = Vec::new();
        dst.write_with_encoder(JpegEncoder::new_with_quality(
            &mut jpeg,
            Self::THUMBNAIL_JPEG_QUALITY,
        ))?;
        Ok(jpeg)
    }
}
