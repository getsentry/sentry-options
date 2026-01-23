//! Python bindings for sentry-options using PyO3.

use std::sync::OnceLock;

use ::sentry_options::{Options as RustOptions, OptionsError as RustOptionsError};
use pyo3::exceptions::{PyException, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PyString};
use serde_json::Value;

// Global options instance
static GLOBAL_OPTIONS: OnceLock<RustOptions> = OnceLock::new();

// Custom exception hierarchy
pyo3::create_exception!(
    sentry_options,
    OptionsError,
    PyException,
    "Base exception for sentry-options errors."
);
pyo3::create_exception!(
    sentry_options,
    SchemaError,
    OptionsError,
    "Raised when schema loading or validation fails."
);
pyo3::create_exception!(
    sentry_options,
    UnknownNamespaceError,
    OptionsError,
    "Raised when accessing an unknown namespace."
);
pyo3::create_exception!(
    sentry_options,
    UnknownOptionError,
    OptionsError,
    "Raised when accessing an unknown option."
);
pyo3::create_exception!(
    sentry_options,
    InitializationError,
    OptionsError,
    "Raised when options are already initialized."
);

/// Convert serde_json::Value to Python object.
fn json_to_py(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Bool(b) => Ok(PyBool::new(py, *b).to_owned().into_any().unbind()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(PyInt::new(py, i).into_any().unbind())
            } else if let Some(f) = n.as_f64() {
                Ok(PyFloat::new(py, f).into_any().unbind())
            } else {
                Err(PyValueError::new_err("Invalid number"))
            }
        }
        Value::String(s) => Ok(PyString::new(py, s).into_any().unbind()),
        Value::Array(_) => Err(PyValueError::new_err("Arrays not yet supported")),
        Value::Object(_) => Err(PyValueError::new_err("Objects not yet supported")),
    }
}

/// Convert Python object to serde_json::Value.
fn py_to_json(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    if obj.is_none() {
        Ok(Value::Null)
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(Value::Bool(b))
    } else if let Ok(i) = obj.extract::<i64>() {
        Ok(Value::Number(i.into()))
    } else if let Ok(f) = obj.extract::<f64>() {
        Ok(Value::Number(serde_json::Number::from_f64(f).ok_or_else(
            || PyValueError::new_err("Cannot convert NaN or Infinity to JSON"),
        )?))
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(Value::String(s))
    } else {
        Err(PyValueError::new_err("Unsupported type for override"))
    }
}

fn options_err(err: RustOptionsError) -> PyErr {
    match err {
        RustOptionsError::UnknownNamespace(ns) => {
            UnknownNamespaceError::new_err(format!("Unknown namespace: {}", ns))
        }
        RustOptionsError::UnknownOption { namespace, key } => UnknownOptionError::new_err(format!(
            "Unknown option '{}' in namespace '{}'",
            key, namespace
        )),
        RustOptionsError::Schema(e) => SchemaError::new_err(e.to_string()),
        RustOptionsError::AlreadyInitialized => {
            InitializationError::new_err("Options already initialized")
        }
    }
}

/// Initialize global options using fallback chain: SENTRY_OPTIONS_DIR env var,
/// then /etc/sentry-options if it exists, otherwise sentry-options/.
#[pyfunction]
fn init() -> PyResult<()> {
    let opts = RustOptions::new().map_err(options_err)?;
    GLOBAL_OPTIONS
        .set(opts)
        .map_err(|_| InitializationError::new_err("Options already initialized"))?;
    Ok(())
}

/// Get a namespace handle for accessing options.
///
/// Raises RuntimeError if init() has not been called.
#[pyfunction]
fn options(namespace: String) -> PyResult<NamespaceOptions> {
    let opts = GLOBAL_OPTIONS
        .get()
        .ok_or_else(|| PyRuntimeError::new_err("Options not initialized - call init() first"))?;
    Ok(NamespaceOptions {
        namespace,
        options: opts,
    })
}

/// Handle for accessing options within a specific namespace.
#[pyclass]
struct NamespaceOptions {
    namespace: String,
    options: &'static RustOptions,
}

#[pymethods]
impl NamespaceOptions {
    /// Get an option value, returning the schema default if not set.
    fn get(&self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        let value = self
            .options
            .get(&self.namespace, key)
            .map_err(options_err)?;
        json_to_py(py, &value)
    }

    fn __repr__(&self) -> String {
        format!("NamespaceOptions({:?})", self.namespace)
    }
}

// Testing utilities - override primitives

/// Set an override value for testing.
#[pyfunction]
fn set_override(namespace: String, key: String, value: &Bound<'_, PyAny>) -> PyResult<()> {
    let json_value = py_to_json(value)?;
    ::sentry_options::testing::set_override(&namespace, &key, json_value);
    Ok(())
}

/// Get an override value if one exists.
#[pyfunction]
fn get_override(py: Python<'_>, namespace: String, key: String) -> PyResult<Option<Py<PyAny>>> {
    match ::sentry_options::testing::get_override(&namespace, &key) {
        Some(v) => Ok(Some(json_to_py(py, &v)?)),
        None => Ok(None),
    }
}

/// Clear an override for a specific namespace and key.
#[pyfunction]
fn clear_override(namespace: String, key: String) {
    ::sentry_options::testing::clear_override(&namespace, &key);
}

/// Validate an override value against the schema.
#[pyfunction]
fn validate_override(namespace: String, key: String, value: &Bound<'_, PyAny>) -> PyResult<()> {
    let json_value = py_to_json(value)?;
    let opts = GLOBAL_OPTIONS
        .get()
        .ok_or_else(|| PyRuntimeError::new_err("Options not initialized - call init() first"))?;
    opts.validate_override(&namespace, &key, &json_value)
        .map_err(options_err)?;
    Ok(())
}

/// Python module definition.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Functions
    m.add_function(wrap_pyfunction!(init, m)?)?;
    m.add_function(wrap_pyfunction!(options, m)?)?;
    // Classes
    m.add_class::<NamespaceOptions>()?;
    // Exceptions
    m.add("OptionsError", m.py().get_type::<OptionsError>())?;
    m.add("SchemaError", m.py().get_type::<SchemaError>())?;
    m.add(
        "UnknownNamespaceError",
        m.py().get_type::<UnknownNamespaceError>(),
    )?;
    m.add(
        "UnknownOptionError",
        m.py().get_type::<UnknownOptionError>(),
    )?;
    m.add(
        "InitializationError",
        m.py().get_type::<InitializationError>(),
    )?;
    // Testing utilities
    m.add_function(wrap_pyfunction!(set_override, m)?)?;
    m.add_function(wrap_pyfunction!(get_override, m)?)?;
    m.add_function(wrap_pyfunction!(clear_override, m)?)?;
    m.add_function(wrap_pyfunction!(validate_override, m)?)?;
    Ok(())
}
