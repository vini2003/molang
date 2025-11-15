use std::collections::HashMap;
use std::fmt;

/// Namespace qualifiers supported by Molang (`temp`, `variable`, `context`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Namespace {
    Temp,
    Variable,
    Context,
}

impl Namespace {
    fn from_prefix(segment: &str) -> Option<Self> {
        match segment.to_ascii_lowercase().as_str() {
            "temp" | "t" => Some(Namespace::Temp),
            "variable" | "var" | "v" => Some(Namespace::Variable),
            "context" | "c" => Some(Namespace::Context),
            _ => None,
        }
    }

    fn prefix(&self) -> &'static str {
        match self {
            Namespace::Temp => "temp",
            Namespace::Variable => "variable",
            Namespace::Context => "context",
        }
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.prefix())
    }
}

/// Canonicalized path (namespace + lowercased dotted key).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QualifiedName {
    namespace: Namespace,
    key: String,
}

impl QualifiedName {
    pub fn from_parts(parts: &[String]) -> Self {
        let mut iter = parts.iter();
        let first = iter.next().cloned().unwrap_or_default();
        let (namespace, use_rest) = if let Some(ns) = Namespace::from_prefix(&first) {
            (ns, true)
        } else {
            (Namespace::Variable, false)
        };
        let remaining: Vec<String> = if use_rest {
            iter.cloned().collect()
        } else {
            std::iter::once(first).chain(iter.cloned()).collect()
        };
        let key = if remaining.is_empty() {
            String::from("_")
        } else {
            remaining
                .iter()
                .map(|segment| segment.to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join(".")
        };
        Self { namespace, key }
    }

    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

impl fmt::Display for QualifiedName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.namespace, self.key)
    }
}

/// Primitive value used by the interpreter.
#[derive(Debug, Clone)]
pub enum Value {
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Null,
}

impl Value {
    /// Wraps a numeric literal.
    pub fn number(value: f64) -> Self {
        Value::Number(value)
    }

    pub fn string(value: impl Into<String>) -> Self {
        Value::String(value.into())
    }

    pub fn array(values: Vec<Value>) -> Self {
        Value::Array(values)
    }

    pub fn as_number(&self) -> f64 {
        match self {
            Value::Number(value) => *value,
            Value::String(_) | Value::Null => 0.0,
            Value::Array(values) => values.len() as f64,
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::Number(value) => *value != 0.0,
            Value::String(text) => !text.is_empty(),
            Value::Array(values) => !values.is_empty(),
            Value::Null => false,
        }
    }
}

/// Runtime storage for variables. Acts like Bedrock's mutable variable scopes.
#[derive(Debug, Clone, Default)]
pub struct RuntimeContext {
    values: HashMap<QualifiedName, Value>,
}

impl RuntimeContext {
    pub fn with_number(
        mut self,
        namespace: Namespace,
        name: impl Into<String>,
        value: f64,
    ) -> Self {
        self.insert(namespace, name, Value::number(value));
        self
    }

    pub fn insert(&mut self, namespace: Namespace, name: impl Into<String>, value: Value) {
        let key = QualifiedName {
            namespace,
            key: name.into().to_ascii_lowercase(),
        };
        self.values.insert(key, value);
    }

    pub fn set_value_with_name(&mut self, name: QualifiedName, value: Value) {
        self.values.insert(name, value);
    }

    /// Convenience setter for string path segments.
    pub fn set_value_for_path(&mut self, parts: &[String], value: Value) {
        let name = QualifiedName::from_parts(parts);
        self.set_value_with_name(name, value);
    }

    pub fn set_number_for_path(&mut self, parts: &[String], value: f64) {
        self.set_value_for_path(parts, Value::number(value));
    }

    pub fn get_value(&self, name: &QualifiedName) -> Option<&Value> {
        self.values.get(name)
    }

    pub fn get_number(&self, name: &QualifiedName) -> Option<f64> {
        self.get_value(name).map(Value::as_number)
    }

    pub fn get_or_default_number(&self, name: &QualifiedName) -> f64 {
        self.get_number(name).unwrap_or(0.0)
    }
}
