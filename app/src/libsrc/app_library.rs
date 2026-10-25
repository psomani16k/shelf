use crate::libsrc::LibrarySource;

pub trait AppLibrary {
    fn get_name(&self) -> String;
    /// Enough to reopen this library after a restart; it's what `AppData` stores.
    fn get_builder(&self) -> LibrarySource;
}
