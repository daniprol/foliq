//! The one error type of foliq.
//!
//! Rust functions that can fail return `Result<T>`: either `Ok(value)` or
//! `Err(error)`. It plays the role of raising an exception in Python. The `?`
//! after a call means "if this failed, return the error to my caller now".

/// Everything that can go wrong in foliq.
///
/// `#[error("...")]` (from the `thiserror` crate) writes the message shown to
/// users, like `__str__` on a Python exception.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Reading or writing a file failed.
    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// A file could not be parsed.
    #[error("could not parse {path}: {message}")]
    Parse { path: String, message: String },

    /// The store (the database) failed.
    #[error("store error: {0}")]
    Store(String),

    /// A document, section or collection does not exist (or the caller may not see it).
    #[error("not found: {0}")]
    NotFound(String),

    /// The caller asked for something invalid (a bad option, a bad reference...).
    #[error("{0}")]
    Invalid(String),

    /// A plugin (Rust or Python) failed.
    #[error("plugin {plugin} failed: {message}")]
    Plugin { plugin: String, message: String },
}

/// Shorthand for "a value of type `T`, or a foliq [`Error`]".
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// A parse error for the file at `path`.
    pub fn parse(path: &str, message: impl ToString) -> Error {
        Error::Parse {
            path: path.to_string(),
            message: message.to_string(),
        }
    }

    /// An error raised by the plugin called `plugin`.
    pub fn plugin(plugin: &str, message: impl ToString) -> Error {
        Error::Plugin {
            plugin: plugin.to_string(),
            message: message.to_string(),
        }
    }
}
