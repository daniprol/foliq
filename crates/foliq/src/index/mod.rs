//! The `Index`: the one object that ties sources, parsers, the splitter, the
//! store and the sinks together.
//!
//! ```no_run
//! use foliq::Index;
//!
//! let index = Index::open(".foliq")?;
//! index.add_folder("docs".as_ref(), None, Vec::new())?;
//! index.sync()?;
//! for result in index.search("refund policy", &Default::default())? {
//!     println!("{} {}", result.citation, result.title);
//! }
//! # Ok::<(), foliq::Error>(())
//! ```
//!
//! The work is split into files by job:
//!
//! | File          | Job                                                  |
//! |---------------|------------------------------------------------------|
//! | `mod.rs`      | opening an index, collections, status                |
//! | `sync.rs`     | bringing the store up to date with the sources       |
//! | `reading.rs`  | references, `outline` and `read`                     |
//! | `search.rs`   | `search` and `context` (passages packed in a budget) |
//! | `citation.rs` | the `collection/path#section (p3, L10-22)` format     |

mod citation;
mod reading;
mod search;
mod sync;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use foliq_core::{Change, Collection, DocumentInfo, Error, Filter, Parser, Result, Sink, Splitter, Store};
use serde::Serialize;

pub use citation::citation;
pub use reading::{Outline, OutlineSection, Passage};
pub use search::{Context, SearchOptions, SearchResult};
pub use sync::{SyncFailure, SyncReport, document_id};

use crate::parsers::builtin_parsers;
use crate::sinks::TextMirror;
use crate::splitter::StructureSplitter;
use crate::store::SqliteStore;

/// The name of the SQLite file inside the index folder.
const STORE_FILE: &str = "index.sqlite";
/// The name of the text mirror folder inside the index folder.
const MIRROR_FOLDER: &str = "text";

/// A searchable index of documents, saved in one folder (`.foliq` by default).
pub struct Index {
    folder: PathBuf,
    store: Arc<dyn Store>,
    /// Your parsers first, then the built-in ones. The first that reads an extension wins.
    parsers: Vec<Arc<dyn Parser>>,
    splitter: Arc<dyn Splitter>,
    sinks: Vec<Arc<dyn Sink>>,
}

/// Collects the plugins of an index before opening it.
///
/// Every method takes `self` and returns it, so calls can be chained:
/// `Index::builder(".foliq").parser(my_parser).mirror(false).open()`.
pub struct IndexBuilder {
    folder: PathBuf,
    store: Option<Arc<dyn Store>>,
    parsers: Vec<Arc<dyn Parser>>,
    splitter: Option<Arc<dyn Splitter>>,
    sinks: Vec<Arc<dyn Sink>>,
    mirror: bool,
}

impl IndexBuilder {
    /// Adds a parser. It takes priority over the built-in parsers for its extensions.
    pub fn parser(mut self, parser: Arc<dyn Parser>) -> IndexBuilder {
        self.parsers.push(parser);
        self
    }

    /// Replaces the default splitter ([`StructureSplitter`] with 400 tokens).
    pub fn splitter(mut self, splitter: Arc<dyn Splitter>) -> IndexBuilder {
        self.splitter = Some(splitter);
        self
    }

    /// Adds a sink that receives every change after a sync.
    pub fn sink(mut self, sink: Arc<dyn Sink>) -> IndexBuilder {
        self.sinks.push(sink);
        self
    }

    /// Replaces the default store (a SQLite file in the index folder).
    pub fn store(mut self, store: Arc<dyn Store>) -> IndexBuilder {
        self.store = Some(store);
        self
    }

    /// Turns the text mirror (`.foliq/text`) on or off. On by default.
    pub fn mirror(mut self, enabled: bool) -> IndexBuilder {
        self.mirror = enabled;
        self
    }

    /// Opens the index, creating its folder if needed.
    pub fn open(self) -> Result<Index> {
        std::fs::create_dir_all(&self.folder)?;
        let store: Arc<dyn Store> = match self.store {
            Some(store) => store,
            None => Arc::new(SqliteStore::open(&self.folder.join(STORE_FILE))?),
        };
        let mut parsers = self.parsers;
        parsers.extend(builtin_parsers());
        let mut sinks = self.sinks;
        if self.mirror {
            sinks.insert(0, Arc::new(TextMirror::new(self.folder.join(MIRROR_FOLDER))));
        }
        Ok(Index {
            folder: self.folder,
            store,
            parsers,
            splitter: self.splitter.unwrap_or_else(|| Arc::new(StructureSplitter::default())),
            sinks,
        })
    }
}

/// Numbers about one collection, for `foliq status`.
#[derive(Debug, Clone, Serialize)]
pub struct CollectionStatus {
    pub collection: Collection,
    pub documents: usize,
    pub chunks: usize,
    pub tokens: usize,
}

impl Index {
    /// Opens (or creates) an index with the default plugins.
    pub fn open(folder: impl Into<PathBuf>) -> Result<Index> {
        Index::builder(folder).open()
    }

    /// Starts building an index with custom plugins.
    pub fn builder(folder: impl Into<PathBuf>) -> IndexBuilder {
        IndexBuilder {
            folder: folder.into(),
            store: None,
            parsers: Vec::new(),
            splitter: None,
            sinks: Vec::new(),
            mirror: true,
        }
    }

    /// The folder that holds the index.
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// The store, for advanced uses (the tools never need it).
    pub fn store(&self) -> &dyn Store {
        self.store.as_ref()
    }

    /// Where the text mirror keeps its Markdown copies.
    pub fn mirror_folder(&self) -> PathBuf {
        self.folder.join(MIRROR_FOLDER)
    }

    /// Adds a folder as a collection (or updates it). Call [`Index::sync`] to read its files.
    ///
    /// `name` defaults to the folder's name. `access` lists who may see its
    /// documents (`"group:hr"`); empty means everyone.
    pub fn add_folder(&self, folder: &Path, name: Option<&str>, access: Vec<String>) -> Result<Collection> {
        let location = folder
            .canonicalize()
            .map_err(|_| Error::NotFound(format!("folder {}", folder.display())))?;
        if !location.is_dir() {
            return Err(Error::Invalid(format!("{} is not a folder", folder.display())));
        }
        let name = match name {
            Some(name) => name.to_string(),
            None => location
                .file_name()
                .map_or_else(|| "root".to_string(), |name| name.to_string_lossy().into_owned()),
        };
        if name.is_empty() || name.contains(['/', '#']) {
            return Err(Error::Invalid(format!(
                "collection names can't be empty or contain '/' or '#': {name:?}"
            )));
        }
        let collection = Collection {
            name,
            source: "folder".to_string(),
            location: location.to_string_lossy().into_owned(),
            access,
        };
        self.store.save_collection(&collection)?;
        Ok(collection)
    }

    /// Removes a collection and its documents. Returns `false` if there was none.
    pub fn remove(&self, name: &str) -> Result<bool> {
        if !self.collections()?.iter().any(|collection| collection.name == name) {
            return Ok(false);
        }
        let filter = Filter {
            collections: vec![name.to_string()],
            ..Filter::default()
        };
        let changes: Vec<Change> = self
            .store
            .documents(&filter)?
            .into_iter()
            .map(|document| Change::Delete {
                document_id: document.id,
                collection: document.collection,
                path: document.path,
            })
            .collect();
        self.store.remove_collection(name)?;
        for sink in &self.sinks {
            sink.apply(&changes)?;
        }
        Ok(true)
    }

    /// All collections, sorted by name.
    pub fn collections(&self) -> Result<Vec<Collection>> {
        let mut collections = self.store.collections()?;
        collections.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(collections)
    }

    /// The documents that pass a filter (without their text).
    pub fn documents(&self, filter: &Filter) -> Result<Vec<DocumentInfo>> {
        self.store.documents(filter)
    }

    /// How many documents, chunks and tokens each collection has.
    pub fn status(&self) -> Result<Vec<CollectionStatus>> {
        let documents = self.store.documents(&Filter::default())?;
        let statuses = self
            .collections()?
            .into_iter()
            .map(|collection| {
                let mine: Vec<&DocumentInfo> = documents
                    .iter()
                    .filter(|document| document.collection == collection.name)
                    .collect();
                CollectionStatus {
                    documents: mine.len(),
                    chunks: mine.iter().map(|document| document.chunk_count).sum(),
                    tokens: mine.iter().map(|document| document.token_count).sum(),
                    collection,
                }
            })
            .collect();
        Ok(statuses)
    }

    /// The file extensions that some parser can read, lowercase and without dots.
    pub fn extensions(&self) -> Vec<String> {
        let mut extensions: Vec<String> = self
            .parsers
            .iter()
            .flat_map(|parser| parser.extensions())
            .map(|extension| extension.to_lowercase())
            .collect();
        extensions.sort();
        extensions.dedup();
        extensions
    }

    /// The parser for a path: the first one that lists its extension.
    fn parser_for(&self, path: &str) -> Option<&Arc<dyn Parser>> {
        let extension = Path::new(path).extension()?.to_string_lossy().to_lowercase();
        self.parsers.iter().find(|parser| {
            parser
                .extensions()
                .iter()
                .any(|known| known.eq_ignore_ascii_case(&extension))
        })
    }
}
