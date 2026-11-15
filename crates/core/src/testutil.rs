use crate::{library::scope::Scope, storage::LocalLibraryStorage};

/// The SHA-1 of an empty file.
pub const EMPTY_SHA1: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
pub const OTHER_ID: &str = "0000000000000000000000000000000000000000";
/// 2025-10-26T04:45:00Z
pub const MTIME: u64 = 1_761_453_900;

pub fn scope(root: &str, patterns: &[&str]) -> Scope {
    let patterns: Vec<String> = patterns.iter().map(|p| p.to_string()).collect();
    Scope::new(root, &patterns).unwrap()
}

/// An empty library in a temp folder, deleted when the `TempDir` is dropped.
pub fn temp_library() -> (tempfile::TempDir, LocalLibraryStorage) {
    let root = tempfile::tempdir().unwrap();
    let storage = LocalLibraryStorage::new(root.path()).unwrap();
    (root, storage)
}
