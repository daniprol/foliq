//! The exceptions users can catch in Python, and how Rust errors become them.
//!
//! ```text
//! FoliqError          every foliq error
//! ├── NotFoundError   no such document, section, collection or folder
//! └── PluginError     a parser, splitter or sink (yours or built-in) failed
//! ```

use foliq::Error;
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyOSError, PyValueError};
use pyo3::prelude::*;

create_exception!(foliq, FoliqError, PyException, "Base class for all foliq errors.");
create_exception!(
    foliq,
    NotFoundError,
    FoliqError,
    "No such document, section, collection or folder."
);
create_exception!(foliq, PluginError, FoliqError, "A parser, splitter or sink failed.");

/// Turns a foliq error into the matching Python exception.
pub fn to_python_error(error: Error) -> PyErr {
    let message = error.to_string();
    match error {
        Error::NotFound(_) => NotFoundError::new_err(message),
        Error::Plugin { .. } | Error::Parse { .. } => PluginError::new_err(message),
        Error::Invalid(_) => PyValueError::new_err(message),
        Error::Io(_) => PyOSError::new_err(message),
        Error::Store(_) => FoliqError::new_err(message),
    }
}

/// Adds the exception classes to the module.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = module.py();
    module.add("FoliqError", py.get_type::<FoliqError>())?;
    module.add("NotFoundError", py.get_type::<NotFoundError>())?;
    module.add("PluginError", py.get_type::<PluginError>())?;
    Ok(())
}
