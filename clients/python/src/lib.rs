//! Python bindings for sentry-options using PyO3.

use std::collections::HashMap;
use std::sync::OnceLock;

use ::sentry_options::{
    ContextValue as RustContextValue, FeatureChecker as RustFeatureChecker,
    FeatureContext as RustFeatureContext, Options as RustOptions, OptionsError as RustOptionsError,
};
use pyo3::exceptions::{PyException, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString};
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
        Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                let py_val = json_to_py(py, v)?;
                dict.set_item(k, py_val)?;
            }
            Ok(dict.into_any().unbind())
        }
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
    } else if let Ok(dict) = obj.cast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            let key: String = k
                .extract()
                .map_err(|_| PyValueError::new_err("Dict keys must be strings"))?;
            map.insert(key, py_to_json(&v)?);
        }
        Ok(Value::Object(map))
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

/// Convert a Python value to a Rust ContextValue.
fn py_to_context_value(py: Python<'_>, value: &Py<PyAny>) -> PyResult<RustContextValue> {
    let bound = value.bind(py);
    // Check bool before int: Python bool is a subclass of int
    if bound.is_instance_of::<PyBool>() {
        let b: bool = bound.extract()?;
        return Ok(RustContextValue::Bool(b));
    }
    if bound.is_instance_of::<PyInt>() {
        let i: i64 = bound.extract()?;
        return Ok(RustContextValue::Int(i));
    }
    if bound.is_instance_of::<PyFloat>() {
        let f: f64 = bound.extract()?;
        return Ok(RustContextValue::Float(f));
    }
    if bound.is_instance_of::<PyString>() {
        let s: String = bound.extract()?;
        return Ok(RustContextValue::String(s));
    }
    if let Ok(list) = bound.cast::<PyList>() {
        if list.is_empty() {
            return Ok(RustContextValue::StringList(vec![]));
        }
        let first = list.get_item(0)?;
        if first.is_instance_of::<PyBool>() {
            let bools: Vec<bool> = list.extract()?;
            return Ok(RustContextValue::BoolList(bools));
        }
        if first.is_instance_of::<PyInt>() {
            let ints: Vec<i64> = list.extract()?;
            return Ok(RustContextValue::IntList(ints));
        }
        if first.is_instance_of::<PyFloat>() {
            let floats: Vec<f64> = list.extract()?;
            return Ok(RustContextValue::FloatList(floats));
        }
        if first.is_instance_of::<PyString>() {
            let strings: Vec<String> = list.extract()?;
            return Ok(RustContextValue::StringList(strings));
        }
        return Err(PyValueError::new_err(
            "Unsupported list element type in FeatureContext",
        ));
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
            let cv = py_to_context_value(py, val)?;
            ctx.insert(key, cv);
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
    let previous = ::sentry_options::testing::get_override(&namespace, &key);
    ::sentry_options::testing::set_override(&namespace, &key, json_value);
    match previous {
        Some(v) => json_to_py(py, &v),
        None => Ok(py.None()),
    }
}

/// Clear a thread-local override.
#[pyfunction]
fn _clear_override(namespace: String, key: String) {
    ::sentry_options::testing::clear_override(&namespace, &key);
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
    // Testing utilities (only called by testing.py)
    m.add_function(wrap_pyfunction!(_set_override, m)?)?;
    m.add_function(wrap_pyfunction!(_clear_override, m)?)?;
    m.add_function(wrap_pyfunction!(_validate_option, m)?)?;
    Ok(())
}
