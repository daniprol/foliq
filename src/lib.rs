//! The Python extension of foliq, importable as `foliq._core`.
//!
//! The engine lives in the `foliq` crate. This crate only translates between
//! Python and Rust:
//!
//! | Module           | What it does                                              |
//! |------------------|-----------------------------------------------------------|
//! | `python_index`   | `foliq.Index`: the engine's methods, with Python arguments |
//! | `python_plugins` | Python classes used as parsers, splitters and sinks       |
//! | `errors`         | The Python exceptions (`FoliqError`, `NotFoundError`...)   |
//! | `json_convert`   | Converts JSON values to Python objects and back            |

use pyo3::prelude::*;

mod errors;
mod json_convert;
mod python_index;
mod python_plugins;

/// Runs the `foliq` command line with these arguments and returns its exit code.
///
/// The `foliq` console script calls it, so `uvx foliq` and the Rust binary behave the same.
#[pyfunction]
fn run_cli(py: Python<'_>, argv: Vec<String>) -> i32 {
    py.detach(|| foliq_cli::run(argv))
}

/// The compiled Python module, importable as `foliq._core`.
/// The name must match `lib.name` in `Cargo.toml`.
#[pymodule]
fn _core(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<python_index::PythonIndex>()?;
    module.add_function(wrap_pyfunction!(run_cli, module)?)?;
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add("SKILL", foliq_cli::SKILL)?;
    errors::register(module)?;
    Ok(())
}
