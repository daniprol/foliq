//! The built-in stores. Today: SQLite. Later: Postgres with pgvector.

mod sqlite;

pub use sqlite::SqliteStore;
