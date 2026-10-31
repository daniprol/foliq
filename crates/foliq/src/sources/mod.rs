//! The built-in sources. Today: folders. Later: S3, Azure Blob, Google Cloud Storage.

mod folder;

pub use folder::FolderSource;
use foliq_core::{Collection, Error, Result, Source};

/// Creates the source a collection describes.
///
/// `extensions` are the file extensions the registered parsers can read.
pub fn source_for(collection: &Collection, extensions: Vec<String>) -> Result<Box<dyn Source>> {
    match collection.source.as_str() {
        "folder" => Ok(Box::new(FolderSource::new(&collection.location, extensions))),
        other => Err(Error::Invalid(format!(
            "unknown source kind {other:?} in collection {}",
            collection.name
        ))),
    }
}
