//! The built-in store: one SQLite file, with SQLite's full-text search (FTS5)
//! for BM25 keyword ranking.
//!
//! Tables:
//!
//! | Table             | Holds                                                     |
//! |-------------------|-----------------------------------------------------------|
//! | `collections`     | the collections and where their files come from          |
//! | `documents`       | each document: text, structure (as JSON), version, hash   |
//! | `document_access` | who may see each document (no rows = everyone)            |
//! | `chunks`          | each chunk, with its positions and heading path           |
//! | `chunks_search`   | the full-text index of the chunks (same row ids)          |

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use foliq_core::{
    Chunk, Collection, Document, DocumentInfo, Error, Filter, IndexedVersion, Result, SearchHit, SearchRequest, Store,
};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter};

use crate::markdown::page_marker_number;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS collections (
        name TEXT PRIMARY KEY,
        source TEXT NOT NULL,
        location TEXT NOT NULL,
        access TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS documents (
        id TEXT PRIMARY KEY,
        collection TEXT NOT NULL,
        path TEXT NOT NULL,
        uri TEXT NOT NULL,
        title TEXT NOT NULL,
        text TEXT NOT NULL,
        elements TEXT NOT NULL,
        metadata TEXT NOT NULL,
        access TEXT NOT NULL,
        version TEXT NOT NULL,
        content_hash TEXT NOT NULL,
        fingerprint TEXT NOT NULL,
        token_count INTEGER NOT NULL,
        UNIQUE (collection, path)
    );
    CREATE TABLE IF NOT EXISTS document_access (
        document_id TEXT NOT NULL,
        principal TEXT NOT NULL,
        PRIMARY KEY (document_id, principal)
    );
    CREATE TABLE IF NOT EXISTS chunks (
        row_id INTEGER PRIMARY KEY,
        id TEXT NOT NULL UNIQUE,
        document_id TEXT NOT NULL,
        ordinal INTEGER NOT NULL,
        start_byte INTEGER NOT NULL,
        end_byte INTEGER NOT NULL,
        line_start INTEGER NOT NULL,
        line_end INTEGER NOT NULL,
        page_start INTEGER,
        page_end INTEGER,
        section TEXT NOT NULL,
        heading_path TEXT NOT NULL,
        token_count INTEGER NOT NULL,
        text TEXT NOT NULL,
        content_key TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS chunks_by_document ON chunks (document_id, ordinal);
    CREATE VIRTUAL TABLE IF NOT EXISTS chunks_search USING fts5 (
        title, headings, body, tokenize = 'porter unicode61'
    );
";

/// How much a match counts in each searched column: headings weigh the most.
const COLUMN_WEIGHTS: &str = "bm25(chunks_search, 2.0, 3.0, 1.0)";

/// The chunk columns, in the order [`chunk_from_row`] reads them.
const CHUNK_COLUMNS: &str = "c.id, c.document_id, c.ordinal, c.start_byte, c.end_byte, c.line_start, c.line_end, \
     c.page_start, c.page_end, c.section, c.heading_path, c.token_count, c.text, c.content_key";

pub struct SqliteStore {
    /// SQLite connections can't be used from two threads at once: the `Mutex`
    /// (a lock) makes each caller wait for its turn.
    connection: Mutex<Connection>,
}

impl SqliteStore {
    /// Opens (or creates) the database file at `path`.
    pub fn open(path: &Path) -> Result<SqliteStore> {
        SqliteStore::setup(Connection::open(path).map_err(store_error)?)
    }

    /// A database that lives only in memory, for tests.
    pub fn in_memory() -> Result<SqliteStore> {
        SqliteStore::setup(Connection::open_in_memory().map_err(store_error)?)
    }

    fn setup(connection: Connection) -> Result<SqliteStore> {
        // WAL lets readers (an agent searching) work while a sync writes.
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(store_error)?;
        connection
            .pragma_update(None, "synchronous", "NORMAL")
            .map_err(store_error)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(10))
            .map_err(store_error)?;
        connection.execute_batch(SCHEMA).map_err(store_error)?;
        Ok(SqliteStore {
            connection: Mutex::new(connection),
        })
    }

    fn connection(&self) -> MutexGuard<'_, Connection> {
        // A poisoned lock only means another thread panicked; the connection itself is fine.
        self.connection.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Store for SqliteStore {
    fn save_collection(&self, collection: &Collection) -> Result<()> {
        self.connection()
            .execute(
                "INSERT OR REPLACE INTO collections (name, source, location, access) VALUES (?1, ?2, ?3, ?4)",
                params![
                    collection.name,
                    collection.source,
                    collection.location,
                    to_json(&collection.access)
                ],
            )
            .map_err(store_error)?;
        Ok(())
    }

    fn remove_collection(&self, name: &str) -> Result<()> {
        let ids: Vec<String> = {
            let connection = self.connection();
            let mut statement = connection
                .prepare("SELECT id FROM documents WHERE collection = ?1")
                .map_err(store_error)?;
            statement
                .query_map([name], |row| row.get(0))
                .map_err(store_error)?
                .collect::<rusqlite::Result<_>>()
                .map_err(store_error)?
        };
        for id in ids {
            self.delete_document(&id)?;
        }
        self.connection()
            .execute("DELETE FROM collections WHERE name = ?1", [name])
            .map_err(store_error)?;
        Ok(())
    }

    fn collections(&self) -> Result<Vec<Collection>> {
        let connection = self.connection();
        let mut statement = connection
            .prepare("SELECT name, source, location, access FROM collections ORDER BY name")
            .map_err(store_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(Collection {
                    name: row.get(0)?,
                    source: row.get(1)?,
                    location: row.get(2)?,
                    access: from_json(&row.get::<_, String>(3)?),
                })
            })
            .map_err(store_error)?;
        rows.collect::<rusqlite::Result<_>>().map_err(store_error)
    }

    fn indexed_versions(&self, collection: &str) -> Result<HashMap<String, IndexedVersion>> {
        let connection = self.connection();
        let mut statement = connection
            .prepare("SELECT path, id, version, content_hash, fingerprint FROM documents WHERE collection = ?1")
            .map_err(store_error)?;
        let rows = statement
            .query_map([collection], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    IndexedVersion {
                        document_id: row.get(1)?,
                        version: row.get(2)?,
                        content_hash: row.get(3)?,
                        fingerprint: row.get(4)?,
                    },
                ))
            })
            .map_err(store_error)?;
        rows.collect::<rusqlite::Result<_>>().map_err(store_error)
    }

    fn write_document(&self, document: &Document, chunks: &[Chunk], fingerprint: &str) -> Result<()> {
        let mut connection = self.connection();
        // A transaction: either everything below is saved, or nothing is.
        let transaction = connection.transaction().map_err(store_error)?;
        delete_document_rows(&transaction, &document.id)?;

        let token_count: usize = chunks.iter().map(|chunk| chunk.token_count).sum();
        transaction
            .execute(
                "INSERT INTO documents (id, collection, path, uri, title, text, elements, metadata, access, version, \
                 content_hash, fingerprint, token_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    document.id,
                    document.collection,
                    document.path,
                    document.uri,
                    document.title,
                    document.text,
                    to_json(&document.elements),
                    to_json(&document.metadata),
                    to_json(&document.access),
                    document.version,
                    document.content_hash,
                    fingerprint,
                    token_count as i64,
                ],
            )
            .map_err(store_error)?;

        for principal in &document.access {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO document_access (document_id, principal) VALUES (?1, ?2)",
                    params![document.id, principal],
                )
                .map_err(store_error)?;
        }

        for chunk in chunks {
            transaction
                .execute(
                    "INSERT INTO chunks (id, document_id, ordinal, start_byte, end_byte, line_start, line_end, page_start, \
                     page_end, section, heading_path, token_count, text, content_key) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    params![
                        chunk.id,
                        chunk.document_id,
                        chunk.ordinal as i64,
                        chunk.start as i64,
                        chunk.end as i64,
                        chunk.line_start as i64,
                        chunk.line_end as i64,
                        chunk.page_start,
                        chunk.page_end,
                        chunk.section,
                        to_json(&chunk.heading_path),
                        chunk.token_count as i64,
                        chunk.text,
                        chunk.content_key,
                    ],
                )
                .map_err(store_error)?;
            let row_id = transaction.last_insert_rowid();
            transaction
                .execute(
                    "INSERT INTO chunks_search (rowid, title, headings, body) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        row_id,
                        document.title,
                        chunk.heading_path.join(" > "),
                        searchable_text(&chunk.text),
                    ],
                )
                .map_err(store_error)?;
        }
        transaction.commit().map_err(store_error)
    }

    fn update_version(&self, document_id: &str, version: &str) -> Result<()> {
        self.connection()
            .execute(
                "UPDATE documents SET version = ?1 WHERE id = ?2",
                params![version, document_id],
            )
            .map_err(store_error)?;
        Ok(())
    }

    fn delete_document(&self, document_id: &str) -> Result<()> {
        let mut connection = self.connection();
        let transaction = connection.transaction().map_err(store_error)?;
        delete_document_rows(&transaction, document_id)?;
        transaction.commit().map_err(store_error)
    }

    fn document(&self, collection: &str, path: &str) -> Result<Option<Document>> {
        let connection = self.connection();
        connection
            .query_row(
                "SELECT id, collection, path, uri, title, text, elements, metadata, access, version, content_hash \
                 FROM documents WHERE collection = ?1 AND path = ?2",
                params![collection, path],
                |row| {
                    Ok(Document {
                        id: row.get(0)?,
                        collection: row.get(1)?,
                        path: row.get(2)?,
                        uri: row.get(3)?,
                        title: row.get(4)?,
                        text: row.get(5)?,
                        elements: from_json(&row.get::<_, String>(6)?),
                        metadata: from_json(&row.get::<_, String>(7)?),
                        access: from_json(&row.get::<_, String>(8)?),
                        version: row.get(9)?,
                        content_hash: row.get(10)?,
                    })
                },
            )
            .optional()
            .map_err(store_error)
    }

    fn documents(&self, filter: &Filter) -> Result<Vec<DocumentInfo>> {
        let mut parameters = Vec::new();
        let conditions = filter_conditions(filter, &mut parameters);
        let sql = format!(
            "SELECT d.id, d.collection, d.path, d.title, d.token_count, d.metadata, \
             (SELECT count(*) FROM chunks c WHERE c.document_id = d.id) \
             FROM documents d WHERE 1 = 1 {conditions} ORDER BY d.collection, d.path"
        );
        let connection = self.connection();
        let mut statement = connection.prepare(&sql).map_err(store_error)?;
        let rows = statement
            .query_map(params_from_iter(parameters), |row| {
                Ok(DocumentInfo {
                    id: row.get(0)?,
                    collection: row.get(1)?,
                    path: row.get(2)?,
                    title: row.get(3)?,
                    token_count: row.get::<_, i64>(4)? as usize,
                    metadata: from_json(&row.get::<_, String>(5)?),
                    chunk_count: row.get::<_, i64>(6)? as usize,
                })
            })
            .map_err(store_error)?;
        rows.collect::<rusqlite::Result<_>>().map_err(store_error)
    }

    fn chunks(&self, filter: &Filter) -> Result<Vec<Chunk>> {
        let mut parameters = Vec::new();
        let conditions = filter_conditions(filter, &mut parameters);
        let sql = format!(
            "SELECT {CHUNK_COLUMNS} FROM chunks c JOIN documents d ON d.id = c.document_id \
             WHERE 1 = 1 {conditions} ORDER BY d.collection, d.path, c.ordinal"
        );
        let connection = self.connection();
        let mut statement = connection.prepare(&sql).map_err(store_error)?;
        let rows = statement
            .query_map(params_from_iter(parameters), chunk_from_row)
            .map_err(store_error)?;
        rows.collect::<rusqlite::Result<_>>().map_err(store_error)
    }

    fn search(&self, request: &SearchRequest) -> Result<Vec<SearchHit>> {
        let Some(match_query) = fts_query(&request.query) else {
            return Ok(Vec::new());
        };
        let mut parameters = vec![Value::Text(match_query)];
        let conditions = filter_conditions(&request.filter, &mut parameters);
        parameters.push(Value::Integer(request.limit.max(1) as i64));
        // The filter is part of the same query, so ranking only ever sees allowed documents.
        let sql = format!(
            "SELECT {CHUNK_COLUMNS}, d.collection, d.path, d.title, {COLUMN_WEIGHTS} AS rank \
             FROM chunks_search \
             JOIN chunks c ON c.row_id = chunks_search.rowid \
             JOIN documents d ON d.id = c.document_id \
             WHERE chunks_search MATCH ? {conditions} \
             ORDER BY rank LIMIT ?"
        );
        let connection = self.connection();
        let mut statement = connection.prepare(&sql).map_err(store_error)?;
        let rows = statement
            .query_map(params_from_iter(parameters), |row| {
                Ok(SearchHit {
                    chunk: chunk_from_row(row)?,
                    collection: row.get(14)?,
                    path: row.get(15)?,
                    title: row.get(16)?,
                    // SQLite's bm25() is lower-is-better; flip it so higher is better.
                    score: -row.get::<_, f64>(17)?,
                })
            })
            .map_err(store_error)?;
        rows.collect::<rusqlite::Result<_>>().map_err(store_error)
    }
}

fn delete_document_rows(connection: &Connection, document_id: &str) -> Result<()> {
    connection
        .execute(
            "DELETE FROM chunks_search WHERE rowid IN (SELECT row_id FROM chunks WHERE document_id = ?1)",
            [document_id],
        )
        .map_err(store_error)?;
    connection
        .execute("DELETE FROM chunks WHERE document_id = ?1", [document_id])
        .map_err(store_error)?;
    connection
        .execute("DELETE FROM document_access WHERE document_id = ?1", [document_id])
        .map_err(store_error)?;
    connection
        .execute("DELETE FROM documents WHERE id = ?1", [document_id])
        .map_err(store_error)?;
    Ok(())
}

/// SQL conditions (`AND ...`) for a filter, on the documents table `d`.
/// The values go into `parameters`, never into the SQL text itself.
fn filter_conditions(filter: &Filter, parameters: &mut Vec<Value>) -> String {
    let mut conditions = String::new();
    if !filter.collections.is_empty() {
        let placeholders = vec!["?"; filter.collections.len()].join(", ");
        conditions.push_str(&format!(" AND d.collection IN ({placeholders})"));
        parameters.extend(filter.collections.iter().cloned().map(Value::Text));
    }
    if let Some(prefix) = &filter.path_prefix {
        conditions.push_str(" AND substr(d.path, 1, ?) = ?");
        parameters.push(Value::Integer(prefix.chars().count() as i64));
        parameters.push(Value::Text(prefix.clone()));
    }
    if let Some(principals) = &filter.principals {
        // Visible when the document has no access rules, or one of them matches the caller.
        conditions.push_str(" AND (NOT EXISTS (SELECT 1 FROM document_access a WHERE a.document_id = d.id)");
        if !principals.is_empty() {
            let placeholders = vec!["?"; principals.len()].join(", ");
            conditions.push_str(&format!(
                " OR EXISTS (SELECT 1 FROM document_access a WHERE a.document_id = d.id AND a.principal IN ({placeholders}))"
            ));
            parameters.extend(principals.iter().cloned().map(Value::Text));
        }
        conditions.push(')');
    }
    conditions
}

/// Common words that only add noise to a keyword search.
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "can", "do", "does", "for", "from", "how", "i", "if", "in", "is",
    "it", "of", "on", "or", "that", "the", "this", "to", "was", "we", "what", "when", "where", "which", "who", "why",
    "will", "with", "you",
];

/// Turns a free-text question into an FTS5 query: every meaningful word, any of them may match.
///
/// Words are quoted, so characters with a meaning in FTS5 (`-`, `:`, `*`...) are never interpreted.
fn fts_query(query: &str) -> Option<String> {
    let words: Vec<String> = query
        .split(|character: char| !character.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|word| !word.is_empty())
        .collect();
    let meaningful: Vec<&String> = words
        .iter()
        .filter(|word| !STOP_WORDS.contains(&word.as_str()))
        .collect();
    // A question made only of stop words still searches for them.
    let chosen: Vec<&String> = if meaningful.is_empty() {
        words.iter().collect()
    } else {
        meaningful
    };
    if chosen.is_empty() {
        return None;
    }
    Some(
        chosen
            .iter()
            .map(|word| format!("\"{word}\""))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

/// The chunk text without page markers, for the search index.
fn searchable_text(text: &str) -> String {
    text.lines()
        .filter(|line| page_marker_number(line).is_none())
        .collect::<Vec<_>>()
        .join("\n")
}

fn chunk_from_row(row: &Row) -> rusqlite::Result<Chunk> {
    Ok(Chunk {
        id: row.get(0)?,
        document_id: row.get(1)?,
        ordinal: row.get::<_, i64>(2)? as usize,
        start: row.get::<_, i64>(3)? as usize,
        end: row.get::<_, i64>(4)? as usize,
        line_start: row.get::<_, i64>(5)? as usize,
        line_end: row.get::<_, i64>(6)? as usize,
        page_start: row.get(7)?,
        page_end: row.get(8)?,
        section: row.get(9)?,
        heading_path: from_json(&row.get::<_, String>(10)?),
        token_count: row.get::<_, i64>(11)? as usize,
        text: row.get(12)?,
        content_key: row.get(13)?,
    })
}

fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("foliq types always serialize")
}

/// Reads JSON written by [`to_json`]. Unreadable values become the type's default.
fn from_json<T: serde::de::DeserializeOwned + Default>(json: &str) -> T {
    serde_json::from_str(json).unwrap_or_default()
}

fn store_error(error: rusqlite::Error) -> Error {
    Error::Store(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_queries_quote_words_and_drop_stop_words() {
        assert_eq!(
            fts_query("How long do refunds take?").unwrap(),
            "\"long\" OR \"refunds\" OR \"take\""
        );
        assert_eq!(fts_query("what is it").unwrap(), "\"what\" OR \"is\" OR \"it\"");
        assert_eq!(
            fts_query("C-3PO: NOT*"),
            Some("\"c\" OR \"3po\" OR \"not\"".to_string())
        );
        assert_eq!(fts_query("  ?! "), None);
    }
}
