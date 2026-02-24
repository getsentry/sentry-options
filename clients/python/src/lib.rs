//! Python bindings for sentry-options using PyO3.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use ::sentry_options::{Options as RustOptions, OptionsError as RustOptionsError};
use pyo3::exceptions::{PyException, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PyList, PyString};
use serde_json::Value;

// Global options instance
static GLOBAL_OPTIONS: OnceLock<RustOptions> = OnceLock::new();

// Thread-local override storage for testing
thread_local! {
    static OVERRIDES: RefCell<HashMap<String, HashMap<String, Value>>> = RefCell::new(HashMap::new());
}

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
        Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                let py_item = json_to_py(py, item)?; // parse child recursively
                list.append(py_item)?;
            }
            Ok(list.into_any().unbind())
        }
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
    } else if let Ok(list) = obj.cast::<PyList>() {
        let items: Result<Vec<Value>, _> = list.iter().map(|item| py_to_json(&item)).collect();
        Ok(Value::Array(items?))
    } else {
        Err(PyValueError::new_err("Unsupported type for override"))
    }
}

/// Check thread-local overrides for a value.
fn get_override(namespace: &str, key: &str) -> Option<Value> {
    OVERRIDES.with(|o| {
        o.borrow()
            .get(namespace)
            .and_then(|ns| ns.get(key).cloned())
    })
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
        if let Some(value) = get_override(&self.namespace, key) {
            return json_to_py(py, &value);
        }

        // Normal path - get from values/defaults
        let value = self
            .options
            .get(&self.namespace, key)
            .map_err(options_err)?;
        json_to_py(py, &value)
    }

    /// Check if an option has a defined value.
    fn isset(&self, key: &str) -> PyResult<bool> {
        self.options
            .isset(&self.namespace, key)
            .map_err(options_err)
    }

    fn __repr__(&self) -> String {
        format!("NamespaceOptions({:?})", self.namespace)
    }
}

// Testing utilities - override storage and validation

/// Set a thread-local override. Returns the previous value (as a Python object) or None.
#[pyfunction]
fn _set_override(
    py: Python<'_>,
    namespace: String,
    key: String,
    value: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let json_value = py_to_json(value)?;
    let previous = OVERRIDES.with(|o| {
        o.borrow_mut()
            .entry(namespace)
            .or_default()
            .insert(key, json_value)
    });
    match previous {
        Some(v) => json_to_py(py, &v),
        None => Ok(py.None()),
    }
}

/// Clear a thread-local override.
#[pyfunction]
fn _clear_override(namespace: String, key: String) {
    OVERRIDES.with(|o| {
        if let Some(ns_map) = o.borrow_mut().get_mut(&namespace) {
            ns_map.remove(&key);
        }
    });
}

/// Validate an option value against the schema (used by testing.py).
#[pyfunction]
fn _validate_option(namespace: String, key: String, value: &Bound<'_, PyAny>) -> PyResult<()> {
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
    // Testing utilities (only called by testing.py)
    m.add_function(wrap_pyfunction!(_set_override, m)?)?;
    m.add_function(wrap_pyfunction!(_clear_override, m)?)?;
    m.add_function(wrap_pyfunction!(_validate_option, m)?)?;
    Ok(())
}
