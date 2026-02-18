//! Python bindings for sentry-options using PyO3.

use std::sync::OnceLock;

use ::sentry_options::{
    ContextValue, FeatureChecker, FeatureContext, Options as RustOptions,
    OptionsError as RustOptionsError,
};
use pyo3::exceptions::{PyException, PyRuntimeError, PyTypeError, PyValueError};
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
        Value::Object(_) => Err(PyValueError::new_err("Objects not yet supported")),
    }
}

/// Convert a Python value to a ContextValue.
///
/// bool must be checked before int because Python's bool is a subclass of int.
fn py_to_context_value(val: &Bound<'_, PyAny>) -> PyResult<ContextValue> {
    if val.is_instance_of::<PyBool>() {
        return Ok(ContextValue::Bool(val.extract::<bool>()?));
    }
    if val.is_instance_of::<PyInt>() {
        return Ok(ContextValue::Int(val.extract::<i64>()?));
    }
    if val.is_instance_of::<PyFloat>() {
        return Ok(ContextValue::Float(val.extract::<f64>()?));
    }
    if val.is_instance_of::<PyString>() {
        return Ok(ContextValue::String(val.extract::<String>()?));
    }
    if val.is_instance_of::<PyList>() {
        let list = val.extract::<Bound<'_, PyList>>()?;
        return py_list_to_context_value(&list);
    }
    Err(PyTypeError::new_err(format!(
        "unsupported context value type: {}",
        val.get_type().name()?
    )))
}

/// Convert a Python list to a typed ContextValue list variant.
///
/// Type is determined by the first element; empty lists are not supported.
fn py_list_to_context_value(list: &Bound<'_, PyList>) -> PyResult<ContextValue> {
    if list.is_empty() {
        return Err(PyTypeError::new_err(
            "empty lists are not supported as context values",
        ));
    }
    let first = list.get_item(0)?;
    if first.is_instance_of::<PyBool>() {
        let mut out = Vec::with_capacity(list.len());
        for item in list.iter() {
            out.push(item.extract::<bool>()?);
        }
        return Ok(ContextValue::BoolList(out));
    }
    if first.is_instance_of::<PyInt>() {
        let mut out = Vec::with_capacity(list.len());
        for item in list.iter() {
            out.push(item.extract::<i64>()?);
        }
        return Ok(ContextValue::IntList(out));
    }
    if first.is_instance_of::<PyFloat>() {
        let mut out = Vec::with_capacity(list.len());
        for item in list.iter() {
            out.push(item.extract::<f64>()?);
        }
        return Ok(ContextValue::FloatList(out));
    }
    if first.is_instance_of::<PyString>() {
        let mut out = Vec::with_capacity(list.len());
        for item in list.iter() {
            out.push(item.extract::<String>()?);
        }
        return Ok(ContextValue::StringList(out));
    }
    Err(PyTypeError::new_err(format!(
        "unsupported list element type: {}",
        first.get_type().name()?
    )))
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

/// Get a feature checker for evaluating feature flags in the given namespace.
///
/// Raises RuntimeError if init() has not been called.
#[pyfunction]
fn features(namespace: String) -> PyResult<PyFeatureChecker> {
    let opts = GLOBAL_OPTIONS
        .get()
        .ok_or_else(|| PyRuntimeError::new_err("Options not initialized - call init() first"))?;
    Ok(PyFeatureChecker {
        inner: FeatureChecker::new(namespace, opts),
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

/// Arbitrary application data passed to feature flag evaluation.
#[pyclass(name = "FeatureContext")]
struct PyFeatureContext {
    inner: FeatureContext,
}

#[pymethods]
impl PyFeatureContext {
    #[new]
    #[pyo3(signature = (data, *, identity_fields=None))]
    fn new(data: &Bound<'_, PyDict>, identity_fields: Option<Vec<String>>) -> PyResult<Self> {
        let mut ctx = FeatureContext::new();
        for (key, value) in data.iter() {
            let key: String = key.extract()?;
            let cv = py_to_context_value(&value)?;
            ctx.insert(key, cv);
        }
        if let Some(fields) = identity_fields {
            ctx.identity_fields(fields.iter().map(|s| s.as_str()).collect());
        }
        Ok(PyFeatureContext { inner: ctx })
    }
}

/// Handle for evaluating feature flags within a specific namespace.
#[pyclass(name = "FeatureChecker")]
struct PyFeatureChecker {
    inner: FeatureChecker,
}

#[pymethods]
impl PyFeatureChecker {
    /// Check whether `feature_name` is enabled for the given context.
    fn has(&self, feature_name: &str, context: &PyFeatureContext) -> bool {
        self.inner.has(feature_name, &context.inner)
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
