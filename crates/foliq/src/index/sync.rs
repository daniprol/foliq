//! Sync: bring the store up to date with the sources.
//!
//! For each collection:
//!
//! 1. List the files. Each comes with a cheap *version* (modified time and size).
//! 2. Skip files whose version and fingerprint (parser + splitter settings) are unchanged.
//! 3. Read the others in parallel. If the bytes hash the same as before, only
//!    record the new version (a `touch` costs nothing). Otherwise parse and split.
//! 4. Save the results, delete documents whose files are gone, and send all
//!    changes to the sinks.
//!
//! If a source can't be listed (a folder was unmounted), the collection is
//! skipped: nothing is deleted because of a failed listing.

use std::collections::HashSet;
use std::path::Path;

use foliq_core::{Change, Chunk, Collection, Document, IndexedVersion, Parsed, Result, Source, SourceEntry};
use rayon::prelude::*;
use serde::Serialize;

use super::Index;
use crate::markdown::read_structure;
use crate::sources::source_for;

/// What a sync did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncReport {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub removed: usize,
    /// Files (or whole collections, with an empty `path`) that could not be indexed.
    pub failed: Vec<SyncFailure>,
}

impl SyncReport {
    /// Whether the sync changed anything in the store.
    pub fn changed(&self) -> bool {
        self.added + self.updated + self.removed > 0
    }
}

/// A file or collection that could not be indexed, and why.
#[derive(Debug, Clone, Serialize)]
pub struct SyncFailure {
    pub collection: String,
    /// Empty when the whole collection failed (for example, a missing folder).
    pub path: String,
    pub message: String,
}

/// What happened to one file that needed a look.
enum FileOutcome {
    /// New version, same bytes: only the version is recorded.
    SameContent {
        document_id: String,
        version: String,
    },
    /// New or changed content, parsed and split.
    Changed {
        document: Box<Document>,
        chunks: Vec<Chunk>,
        fingerprint: String,
        is_new: bool,
    },
    Failed {
        path: String,
        message: String,
    },
}

impl Index {
    /// Syncs every collection.
    pub fn sync(&self) -> Result<SyncReport> {
        let mut report = SyncReport::default();
        for collection in self.collections()? {
            self.sync_collection(&collection, &mut report)?;
        }
        Ok(report)
    }

    /// Syncs one collection, adding what happened to `report`.
    pub fn sync_collection(&self, collection: &Collection, report: &mut SyncReport) -> Result<()> {
        let fail = |report: &mut SyncReport, message: String| {
            report.failed.push(SyncFailure {
                collection: collection.name.clone(),
                path: String::new(),
                message,
            });
        };
        let source = match source_for(collection, self.extensions()) {
            Ok(source) => source,
            Err(error) => {
                fail(report, error.to_string());
                return Ok(());
            }
        };
        let entries = match source.list() {
            Ok(entries) => entries,
            Err(error) => {
                fail(report, error.to_string());
                return Ok(());
            }
        };
        let known = self.store.indexed_versions(&collection.name)?;

        // Find the files that need a look (new, touched, or processed with other settings).
        let mut to_check = Vec::new();
        for entry in &entries {
            let Some(parser) = self.parser_for(&entry.path) else {
                continue;
            };
            let fingerprint = format!("{}+{}", parser.fingerprint(), self.splitter.fingerprint());
            let previous = known.get(&entry.path);
            match previous {
                Some(previous) if previous.version == entry.version && previous.fingerprint == fingerprint => {
                    report.unchanged += 1;
                }
                _ => to_check.push((entry, previous, fingerprint)),
            }
        }

        // Read, parse and split in parallel, one file per CPU core.
        let outcomes: Vec<FileOutcome> = to_check
            .par_iter()
            .map(|(entry, previous, fingerprint)| {
                self.process_file(collection, source.as_ref(), entry, *previous, fingerprint)
                    .unwrap_or_else(|error| FileOutcome::Failed {
                        path: entry.path.clone(),
                        message: error.to_string(),
                    })
            })
            .collect();

        // Save the results one by one (the store is the single writer).
        let mut changes = Vec::new();
        for outcome in outcomes {
            match outcome {
                FileOutcome::SameContent { document_id, version } => {
                    self.store.update_version(&document_id, &version)?;
                    report.unchanged += 1;
                }
                FileOutcome::Changed {
                    document,
                    chunks,
                    fingerprint,
                    is_new,
                } => {
                    self.store.write_document(&document, &chunks, &fingerprint)?;
                    if is_new {
                        report.added += 1;
                    } else {
                        report.updated += 1;
                    }
                    changes.push(Change::Upsert {
                        document: *document,
                        chunks,
                    });
                }
                FileOutcome::Failed { path, message } => {
                    report.failed.push(SyncFailure {
                        collection: collection.name.clone(),
                        path,
                        message,
                    });
                }
            }
        }

        // Delete the documents whose files are gone.
        let listed: HashSet<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
        let mut gone: Vec<(&String, &IndexedVersion)> = known
            .iter()
            .filter(|(path, _)| !listed.contains(path.as_str()))
            .collect();
        gone.sort_by(|a, b| a.0.cmp(b.0));
        for (path, indexed) in gone {
            self.store.delete_document(&indexed.document_id)?;
            report.removed += 1;
            changes.push(Change::Delete {
                document_id: indexed.document_id.clone(),
                collection: collection.name.clone(),
                path: path.clone(),
            });
        }

        // Tell the sinks. A failing sink doesn't undo the sync; it is reported.
        if !changes.is_empty() {
            for sink in &self.sinks {
                if let Err(error) = sink.apply(&changes) {
                    fail(report, format!("sink {}: {error}", sink.name()));
                }
            }
        }
        Ok(())
    }

    /// Reads one file and, if its bytes changed, parses and splits it.
    fn process_file(
        &self,
        collection: &Collection,
        source: &dyn Source,
        entry: &SourceEntry,
        previous: Option<&IndexedVersion>,
        fingerprint: &str,
    ) -> Result<FileOutcome> {
        let bytes = source.read(entry)?;
        let content_hash = blake3::hash(&bytes).to_hex().to_string();
        if let Some(previous) = previous
            && previous.content_hash == content_hash
            && previous.fingerprint == fingerprint
        {
            return Ok(FileOutcome::SameContent {
                document_id: previous.document_id.clone(),
                version: entry.version.clone(),
            });
        }
        let parser = self
            .parser_for(&entry.path)
            .expect("files without a parser are skipped before");
        let parsed = parser.parse(&bytes, entry)?;
        let document = build_document(collection, entry, parsed, content_hash);
        let chunks = self.splitter.split(&document)?;
        Ok(FileOutcome::Changed {
            document: Box::new(document),
            chunks,
            fingerprint: fingerprint.to_string(),
            is_new: previous.is_none(),
        })
    }
}

/// Builds a document from a parser's output: finds the structure, the title and who may see it.
fn build_document(collection: &Collection, entry: &SourceEntry, parsed: Parsed, content_hash: String) -> Document {
    let structure = read_structure(&parsed.text);

    // Metadata from the source, then the front matter, then the parser (later wins).
    let mut metadata = entry.metadata.clone();
    metadata.extend(structure.front_matter);
    metadata.extend(parsed.metadata);

    let title = parsed
        .title
        .filter(|title| !title.trim().is_empty())
        .or(structure.title)
        .unwrap_or_else(|| file_stem(&entry.path));

    // Rules from the file itself (like a cloud object's ACL) win over the collection's.
    let access = if entry.access.is_empty() {
        collection.access.clone()
    } else {
        entry.access.clone()
    };

    Document {
        id: document_id(&collection.name, &entry.path),
        collection: collection.name.clone(),
        path: entry.path.clone(),
        uri: entry.uri.clone(),
        title,
        elements: parsed.elements.unwrap_or(structure.elements),
        text: parsed.text,
        metadata,
        access,
        version: entry.version.clone(),
        content_hash,
    }
}

/// The stable id of a document: a short hash of its collection and path.
pub fn document_id(collection: &str, path: &str) -> String {
    blake3::hash(format!("{collection}\n{path}").as_bytes()).to_hex()[..16].to_string()
}

/// `guides/intro.md` -> `intro`.
fn file_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map_or_else(|| path.to_string(), |stem| stem.to_string_lossy().into_owned())
}
