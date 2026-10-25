use std::path::{Path, PathBuf};

use anyhow::Ok;
use fast_image_resize::Resizer;
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
    // The largest a thumbnail gets; it keeps the cover's own shape inside this box.
    const THUMBNAIL_WIDTH: u32 = 400;
    const THUMBNAIL_HEIGHT: u32 = 600;
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
            let path = self.relative_path.to_string_lossy();
            if meta.path != path || meta.id.is_empty() {
                meta.path = path.into_owned();
                meta.id = hash.to_string();
                self.book_keeper.set_book_metadata(hash, &meta)?;
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
            id: hash.to_string(),
        };
        self.book_keeper.set_book_metadata(hash, &metadata)?;

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

    /// Scales the whole cover down to fit the thumbnail box, never cropping or enlarging it.
    /// CPU-heavy: call it inside `spawn_blocking`.
    fn resize_image_to_thumbnail(&self, image: &[u8]) -> anyhow::Result<Vec<u8>> {
        let src = DynamicImage::ImageRgb8(image::load_from_memory(image)?.into_rgb8());
        let (width, height) = (src.width() as f64, src.height() as f64);
        let scale = (Self::THUMBNAIL_WIDTH as f64 / width)
            .min(Self::THUMBNAIL_HEIGHT as f64 / height)
            .min(1.0);
        let size = |side: f64| ((side * scale).round() as u32).max(1);
        let mut dst = DynamicImage::new_rgb8(size(width), size(height));
        Resizer::new().resize(&src, &mut dst, None)?;

        let mut jpeg = Vec::new();
        dst.write_with_encoder(JpegEncoder::new_with_quality(
            &mut jpeg,
            Self::THUMBNAIL_JPEG_QUALITY,
        ))?;
        Ok(jpeg)
    }
}
