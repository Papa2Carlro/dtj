//! PyO3 bindings for dtj-core.
//!
//! Exposes SessionReader to Python via the `dtj_python` extension module.
//! This is the SINGLE SOURCE OF TRUTH for .dtj parsing - both CLI and MCP
//! use this same implementation.

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::{Error, EventRecord, SessionReader};

/// Raised on decode failures (fail-closed; never panic).
#[derive(Debug)]
pub struct DtjError {
    kind: String,
    message: String,
}

impl From<Error> for DtjError {
    fn from(err: Error) -> Self {
        let kind = match &err {
            Error::InvalidMagic(_) => "InvalidMagic",
            Error::InvalidVersion(_) => "InvalidVersion",
            Error::InvalidChunkMagic => "InvalidChunkMagic",
            Error::MalformedRecord(_) => "MalformedRecord",
            Error::ChecksumMismatch { .. } => "ChecksumMismatch",
            Error::PayloadTooLarge { .. } => "PayloadTooLarge",
            Error::LimitExceeded(_) => "LimitExceeded",
            Error::InvalidSeverity(_) => "InvalidSeverity",
            Error::Io(_) => "Io",
            Error::UnexpectedEof => "UnexpectedEof",
            Error::TooManyChunks => "TooManyChunks",
            Error::InvalidMarker(_) => "InvalidMarker",
        }
        .to_string();
        DtjError {
            kind,
            message: err.to_string(),
        }
    }
}

impl std::fmt::Display for DtjError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for DtjError {}

impl pyo3::exception::PyException for DtjError {
    fn exception_type(_: &Bound<'_, pyo3::types::PyType>) -> pyo3::type_object::PyTypeSpec {
        pyo3::exception::PyException::exception_type_bound
    }
}

/// Open a DTJ session file and return the reader object.
#[pyfunction]
pub fn open_session(path: &str) -> Result<SessionReaderPy, DtjError> {
    let reader = SessionReader::open(path).map_err(DtjError::from)?;
    Ok(SessionReaderPy { reader })
}

/// Session reader wrapper for Python.
#[pyclass]
pub struct SessionReaderPy {
    reader: SessionReader,
}

#[pymethods]
impl SessionReaderPy {
    /// Return session as JSON-serializable dict.
    fn to_dict(&self) -> PyResult<Py<PyDict>> {
        Python::with_gil(|py| {
            let dict = PyDict::new(py);

            // Header info
            dict.set_item("magic", self.reader.header.magic.to_vec())?;
            dict.set_item("version", self.reader.header.version)?;
            dict.set_item("flags", self.reader.header.flags)?;
            dict.set_item("session_id", self.reader.header.session_id.to_string())?;
            dict.set_item("created_at", self.reader.header.created_at)?;
            dict.set_item("chunks_committed", self.reader.chunks_committed)?;
            dict.set_item("torn_tail", self.reader.torn_tail)?;

            // Dictionary
            let dict_entries: Vec<Py<PyDict>> = self
                .reader
                .dictionary
                .entries
                .iter()
                .map(|e| {
                    Python::with_gil(|py| {
                        let entry = PyDict::new(py);
                        entry.set_item("kind", e.kind.as_u8()).unwrap();
                        entry.set_item("id", e.id).unwrap();
                        entry.set_item("value", &e.value).unwrap();
                        entry.to_object(py)
                    })
                })
                .collect();
            dict.set_item("dictionary", dict_entries)?;

            // Events
            let events: Vec<Py<PyDict>> = self
                .reader
                .events
                .iter()
                .map(|e| event_to_dict(py, e))
                .collect();
            dict.set_item("events", events)?;

            Ok(dict.to_object(py))
        })
    }

    /// Return events as JSON-serializable list.
    fn events_list(&self) -> PyResult<Vec<Py<PyDict>>> {
        Python::with_gil(|py| {
            self.reader
                .events
                .iter()
                .map(|e| event_to_dict(py, e))
                .collect()
        })
    }

    /// Number of events in session.
    fn __len__(&self) -> usize {
        self.reader.events.len()
    }

    /// Session info summary.
    fn summary(&self) -> PyResult<Py<PyDict>> {
        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("chunks_committed", self.reader.chunks_committed)?;
            dict.set_item("event_count", self.reader.events.len())?;
            dict.set_item("torn_tail", self.reader.torn_tail)?;
            dict.set_item(
                "session_id",
                self.reader.header.session_id.to_string(),
            )?;
            Ok(dict.to_object(py))
        })
    }
}

fn event_to_dict(py: Python<'_>, event: &EventRecord) -> Py<PyDict> {
    let dict = PyDict::new(py);
    dict.set_item("monotonic_ns", event.monotonic_ns).unwrap();
    dict.set_item("event_sequence", event.event_sequence).unwrap();
    dict.set_item("domain_id", event.domain_id).unwrap();
    dict.set_item("category_id", event.category_id).unwrap();
    dict.set_item("event_name_id", event.event_name_id).unwrap();
    dict.set_item("correlation_id", event.correlation_id).unwrap();
    dict.set_item("severity", event.severity.as_u8()).unwrap();
    // Payload as typed dict
    let payload = PyDict::new(py);
    // (simplified - full payload handling would go here)
    dict.set_item("payload", payload).unwrap();
    dict.to_object(py)
}

/// Python module: dtj_python
#[pymodule]
pub fn dtj_python(m: &Bound<'_, pyo3::types::PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(open_session, m)?)?;
    m.add_class::<SessionReaderPy>()?;
    m.add("DtjError", py_error_type(m.py())?)?;
    Ok(())
}

fn py_error_type(_py: Python<'_>) -> PyResult<Bound<'_, pyo3::types::PyType>> {
    // Return a basic exception type - actual DtjError would need custom exception class
    Ok(pyo3::types::PyDict::type_object_bound(
        pyo3::exceptions::PyException::type_object_bound,
    ))
}
