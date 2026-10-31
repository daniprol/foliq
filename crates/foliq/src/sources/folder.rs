//! A folder on disk. Respects `.gitignore` files and skips hidden files.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use foliq_core::{Error, Metadata, Result, Source, SourceEntry};
use ignore::WalkBuilder;

pub struct FolderSource {
    root: PathBuf,
    /// Only files with these extensions are listed (the ones some parser can read).
    extensions: Vec<String>,
}

impl FolderSource {
    pub fn new(root: impl Into<PathBuf>, extensions: Vec<String>) -> FolderSource {
        FolderSource {
            root: root.into(),
            extensions,
        }
    }
}

impl Source for FolderSource {
    fn name(&self) -> &str {
        "folder"
    }

    fn list(&self) -> Result<Vec<SourceEntry>> {
        if !self.root.is_dir() {
            return Err(Error::NotFound(format!("folder {}", self.root.display())));
        }
        let walker = WalkBuilder::new(&self.root)
            .hidden(true) // skip hidden files and folders (like `.git` and `.foliq`)
            .require_git(false) // use `.gitignore` files even outside a git repository
            .build();

        let mut entries = Vec::new();
        for item in walker {
            let item = item.map_err(|error| Error::Io(std::io::Error::other(error)))?;
            let path = item.path();
            if !item.file_type().is_some_and(|kind| kind.is_file()) || !self.has_known_extension(path) {
                continue;
            }
            let metadata = item
                .metadata()
                .map_err(|error| Error::Io(std::io::Error::other(error)))?;
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |duration| duration.as_nanos());
            let relative = path.strip_prefix(&self.root).unwrap_or(path);
            entries.push(SourceEntry {
                path: relative
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
                uri: format!("file://{}", path.display()),
                // A file's version is its modified time and size: cheap to read, changes on every save.
                version: format!("{modified}-{}", metadata.len()),
                size: metadata.len(),
                access: Vec::new(),
                metadata: Metadata::new(),
            });
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    }

    fn read(&self, entry: &SourceEntry) -> Result<Vec<u8>> {
        Ok(std::fs::read(self.root.join(&entry.path))?)
    }
}

impl FolderSource {
    fn has_known_extension(&self, path: &Path) -> bool {
        path.extension()
            .map(|extension| extension.to_string_lossy().to_lowercase())
            .is_some_and(|extension| self.extensions.contains(&extension))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_known_files_and_respects_gitignore() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        std::fs::create_dir_all(root.join("guides")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("guides/a.md"), "# A").unwrap();
        std::fs::write(root.join("b.PDF"), "%PDF").unwrap();
        std::fs::write(root.join("image.png"), "png").unwrap();
        std::fs::write(root.join("secret.md"), "no").unwrap();
        std::fs::write(root.join(".hidden/c.md"), "no").unwrap();
        std::fs::write(root.join(".gitignore"), "secret.md\n").unwrap();

        let source = FolderSource::new(root, vec!["md".into(), "pdf".into()]);
        let paths: Vec<String> = source.list().unwrap().into_iter().map(|entry| entry.path).collect();
        assert_eq!(paths, ["b.PDF", "guides/a.md"]);
    }
}
