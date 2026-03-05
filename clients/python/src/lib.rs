//! Python bindings for sentry-options using PyO3.

use std::collections::HashMap;
use std::sync::OnceLock;

use ::sentry_options::{
    FeatureChecker as RustFeatureChecker, FeatureContext as RustFeatureContext,
    Options as RustOptions, OptionsError as RustOptionsError,
};
use pyo3::exceptions::{PyException, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PyList, PyString};
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

/// Convert a Python value to a serde_json::Value.
fn py_to_json(py: Python<'_>, value: &Py<PyAny>) -> PyResult<Value> {
    let bound = value.bind(py);
    // Check bool before int: Python bool is a subclass of int
    if bound.is_instance_of::<PyBool>() {
        let b: bool = bound.extract()?;
        return Ok(Value::Bool(b));
    }
    if bound.is_instance_of::<PyInt>() {
        let i: i64 = bound.extract()?;
        return Ok(Value::from(i));
    }
    if bound.is_instance_of::<PyFloat>() {
        let f: f64 = bound.extract()?;
        return Ok(Value::from(f));
    }
    if bound.is_instance_of::<PyString>() {
        let s: String = bound.extract()?;
        return Ok(Value::from(s));
    }
    if let Ok(list) = bound.cast::<PyList>() {
        let items: PyResult<Vec<Value>> = list.iter().map(|item| {
            let py_any: Py<PyAny> = item.unbind();
            py_to_json(py, &py_any)
        }).collect();
        return Ok(Value::Array(items?));
    }
    Err(PyValueError::new_err(format!(
        "Unsupported FeatureContext value type: {}",
        bound.get_type().name()?
    )))
}

/// Feature evaluation context holding arbitrary key-value data.
///
/// Pass a dict of context data and optional identity_fields to control
/// rollout bucketing. Identity fields are sorted and hashed to produce
/// a stable identifier used for percentage rollouts.
#[pyclass(name = "FeatureContext", unsendable)]
struct PyFeatureContext {
    inner: RustFeatureContext,
}

#[pymethods]
impl PyFeatureContext {
    #[new]
    #[pyo3(signature = (data, *, identity_fields=None))]
    fn new(
        data: HashMap<String, Py<PyAny>>,
        identity_fields: Option<Vec<String>>,
        py: Python<'_>,
    ) -> PyResult<Self> {
        let mut ctx = RustFeatureContext::new();
        for (key, val) in &data {
            let json_val = py_to_json(py, val)?;
            ctx.insert(key, json_val);
        }
        if let Some(fields) = identity_fields {
            let field_refs: Vec<&str> = fields.iter().map(|s| s.as_str()).collect();
            ctx.identity_fields(field_refs);
        }
        Ok(PyFeatureContext { inner: ctx })
    }

    fn __repr__(&self) -> String {
        "FeatureContext(...)".to_string()
    }
}

/// Handle for evaluating feature flags within a specific namespace.
#[pyclass(name = "FeatureChecker")]
struct PyFeatureChecker {
    inner: RustFeatureChecker,
}

#[pymethods]
impl PyFeatureChecker {
    /// Check whether a feature flag is enabled for a given context.
    ///
    /// Returns false if the feature is not defined, not enabled, or conditions don't match.
    fn has(&self, feature_name: &str, context: PyRef<'_, PyFeatureContext>) -> bool {
        self.inner.has(feature_name, &context.inner)
    }

    fn __repr__(&self) -> String {
        "FeatureChecker(...)".to_string()
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

/// Get a feature checker for a namespace.
///
/// Raises RuntimeError if init() has not been called.
#[pyfunction]
fn features(namespace: String) -> PyResult<PyFeatureChecker> {
    let opts = GLOBAL_OPTIONS
        .get()
        .ok_or_else(|| PyRuntimeError::new_err("Options not initialized - call init() first"))?;
    Ok(PyFeatureChecker {
        inner: RustFeatureChecker::new(namespace, opts),
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

/// Python module definition.
#[pymodule]
fn sentry_options(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Functions
    m.add_function(wrap_pyfunction!(init, m)?)?;
    m.add_function(wrap_pyfunction!(options, m)?)?;
    m.add_function(wrap_pyfunction!(features, m)?)?;
    // Classes
    m.add_class::<NamespaceOptions>()?;
    m.add_class::<PyFeatureContext>()?;
    m.add_class::<PyFeatureChecker>()?;
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
    Ok(())
}
