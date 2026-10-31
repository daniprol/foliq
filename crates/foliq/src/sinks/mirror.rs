//! The text mirror: a Markdown copy of every PDF, DOCX, PPTX... on disk.
//!
//! Agents are good with `grep`, `cat` and `sed`. The mirror lets them use those
//! tools on any format: `contracts/acme.pdf` becomes
//! `.foliq/text/<collection>/contracts/acme.pdf.md`. Markdown and text files
//! are not copied, since they can already be read directly.

use std::path::{Path, PathBuf};

use foliq_core::{Change, Result, Sink};

/// File extensions that are already plain text, so they are not mirrored.
const PLAIN_TEXT_EXTENSIONS: &[&str] = &["md", "markdown", "mdx", "txt", "text", "rst"];

pub struct TextMirror {
    root: PathBuf,
}

impl TextMirror {
    /// A mirror that writes under `root` (usually `.foliq/text`).
    pub fn new(root: impl Into<PathBuf>) -> TextMirror {
        TextMirror { root: root.into() }
    }

    /// Where the mirror keeps the copy of a document.
    pub fn path_for(&self, collection: &str, path: &str) -> PathBuf {
        self.root.join(collection).join(format!("{path}.md"))
    }
}

impl Sink for TextMirror {
    fn name(&self) -> &str {
        "mirror"
    }

    fn apply(&self, changes: &[Change]) -> Result<()> {
        for change in changes {
            match change {
                Change::Upsert { document, .. } => {
                    if is_plain_text(&document.path) {
                        continue;
                    }
                    let target = self.path_for(&document.collection, &document.path);
                    if let Some(folder) = target.parent() {
                        std::fs::create_dir_all(folder)?;
                    }
                    std::fs::write(target, &document.text)?;
                }
                Change::Delete { collection, path, .. } => {
                    let target = self.path_for(collection, path);
                    if target.exists() {
                        std::fs::remove_file(target)?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn is_plain_text(path: &str) -> bool {
    Path::new(path)
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .is_some_and(|extension| PLAIN_TEXT_EXTENSIONS.contains(&extension.as_str()))
}
