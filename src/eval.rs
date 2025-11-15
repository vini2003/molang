use indexmap::IndexMap;
use std::collections::HashMap;
use std::fmt;

/// Namespace qualifiers supported by Molang (`temp`, `variable`, `context`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Namespace {
    Temp,
    Variable,
    Context,
    Query,
}

impl Namespace {
    fn split_parts(parts: &[String]) -> (Self, Vec<String>) {
        let mut iter = parts.iter();
        let first = iter.next().cloned().unwrap_or_default();
        if let Some(ns) = Namespace::from_prefix(&first) {
            (ns, iter.cloned().collect())
        } else {
            (
                Namespace::Variable,
                std::iter::once(first).chain(iter.cloned()).collect(),
            )
        }
    }

    fn from_prefix(segment: &str) -> Option<Self> {
        match segment.to_ascii_lowercase().as_str() {
            "temp" | "t" => Some(Namespace::Temp),
            "variable" | "var" | "v" => Some(Namespace::Variable),
            "context" | "c" => Some(Namespace::Context),
            "query" | "q" => Some(Namespace::Query),
            _ => None,
        }
    }

    fn prefix(&self) -> &'static str {
        match self {
            Namespace::Temp => "temp",
            Namespace::Variable => "variable",
            Namespace::Context => "context",
            Namespace::Query => "query",
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
    pub fn new(namespace: Namespace, key: String) -> Self {
        Self { namespace, key }
    }

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

    pub fn segments(&self) -> Vec<String> {
        if self.key.is_empty() {
            Vec::new()
        } else {
            self.key.split('.').map(|s| s.to_string()).collect()
        }
    }

    pub fn to_path(&self) -> Vec<String> {
        let mut parts = vec![self.namespace.prefix().to_string()];
        if !self.key.is_empty() {
            parts.extend(self.key.split('.').map(|segment| segment.to_string()));
        }
        parts
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
    Struct(IndexMap<String, Value>),
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
            Value::String(_) | Value::Null | Value::Struct(_) => 0.0,
            Value::Array(values) => values.len() as f64,
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::Number(value) => *value != 0.0,
            Value::String(text) => !text.is_empty(),
            Value::Array(values) => !values.is_empty(),
            Value::Struct(map) => !map.is_empty(),
            Value::Null => false,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(values) => Some(values),
            _ => None,
        }
    }

    pub fn as_struct(&self) -> Option<&IndexMap<String, Value>> {
        match self {
            Value::Struct(map) => Some(map),
            _ => None,
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
        let (namespace, raw_segments) = Namespace::split_parts(parts);
        if namespace == Namespace::Query {
            return;
        }
        let segments: Vec<String> = raw_segments
            .into_iter()
            .map(|segment| segment.to_ascii_lowercase())
            .collect();
        if segments.is_empty() {
            return;
        }
        self.assign_nested(namespace, &segments, value);
    }

    pub fn set_number_for_path(&mut self, parts: &[String], value: f64) {
        self.set_value_for_path(parts, Value::number(value));
    }

    pub fn get_number(&self, name: &QualifiedName) -> Option<f64> {
        self.lookup_namespace_path(name.namespace.clone(), &name.segments())
            .map(|value| value.as_number())
    }

    pub fn get_or_default_number(&self, name: &QualifiedName) -> f64 {
        self.get_number(name).unwrap_or(0.0)
    }

    pub fn get_number_canonical(&self, canonical: &str) -> Option<f64> {
        let (namespace, segments) = parse_canonical_path(canonical)?;
        self.lookup_namespace_path(namespace, &segments)
            .map(|value| value.as_number())
    }

    pub fn get_value_canonical(&self, canonical: &str) -> Option<Value> {
        let (namespace, segments) = parse_canonical_path(canonical)?;
        self.lookup_namespace_path(namespace, &segments)
    }

    pub fn set_number_canonical(&mut self, canonical: &str, value: f64) {
        if let Some((namespace, segments)) = parse_canonical_path(canonical) {
            if namespace == Namespace::Query || segments.is_empty() {
                return;
            }
            let lower = segments
                .into_iter()
                .map(|segment| segment.to_ascii_lowercase())
                .collect::<Vec<_>>();
            self.assign_nested(namespace, &lower, Value::number(value));
        }
    }

    pub fn set_value_canonical(&mut self, canonical: &str, value: Value) {
        if let Some((namespace, segments)) = parse_canonical_path(canonical) {
            if namespace == Namespace::Query || segments.is_empty() {
                return;
            }
            let lower = segments
                .into_iter()
                .map(|segment| segment.to_ascii_lowercase())
                .collect::<Vec<_>>();
            self.assign_nested(namespace, &lower, value);
        }
    }

    pub fn clear_value_canonical(&mut self, canonical: &str) {
        if let Some((namespace, segments)) = parse_canonical_path(canonical) {
            let lower = segments
                .into_iter()
                .map(|segment| segment.to_ascii_lowercase())
                .collect::<Vec<_>>();
            let key = lower.join(".");
            let prefix = if key.is_empty() {
                String::new()
            } else {
                format!("{key}.")
            };
            self.values.retain(|name, _| {
                if name.namespace() != &namespace {
                    return true;
                }
                let current = name.key();
                if current == key {
                    return false;
                }
                if prefix.is_empty() {
                    true
                } else {
                    !current.starts_with(&prefix)
                }
            });
        }
    }

    pub fn copy_value_canonical(&mut self, dest: &str, src: &str) {
        if let Some(value) = self.get_value_canonical(src) {
            self.set_value_canonical(dest, value);
        } else {
            self.clear_value_canonical(dest);
        }
    }

    pub fn array_push_number_canonical(&mut self, canonical: &str, value: f64) {
        self.array_push_value_canonical(canonical, Value::number(value));
    }

    pub fn array_push_value_canonical(&mut self, canonical: &str, value: Value) {
        let mut values = match self.get_value_canonical(canonical) {
            Some(Value::Array(existing)) => existing,
            _ => Vec::new(),
        };
        values.push(value);
        self.set_value_canonical(canonical, Value::Array(values));
    }

    pub fn array_push_string_canonical(&mut self, canonical: &str, value: &str) {
        self.array_push_value_canonical(canonical, Value::string(value));
    }

    pub fn array_get_number_canonical(&self, canonical: &str, index: f64) -> f64 {
        self.array_get_value_canonical(canonical, index)
            .map(|value| value.as_number())
            .unwrap_or(0.0)
    }

    pub fn array_length_canonical(&self, canonical: &str) -> i64 {
        match self.get_value_canonical(canonical) {
            Some(Value::Array(values)) => values.len() as i64,
            _ => 0,
        }
    }

    pub fn array_copy_element_canonical(&mut self, canonical: &str, index: i64, dest: &str) {
        if let Some(value) = self.array_get_value_by_index(canonical, index) {
            self.set_value_canonical(dest, value);
        } else {
            self.clear_value_canonical(dest);
        }
    }

    fn array_get_value_canonical(&self, canonical: &str, index: f64) -> Option<Value> {
        let idx = index as i64;
        self.array_get_value_by_index(canonical, idx)
    }

    fn array_get_value_by_index(&self, canonical: &str, index: i64) -> Option<Value> {
        match self.get_value_canonical(canonical) {
            Some(Value::Array(values)) => {
                if values.is_empty() {
                    return None;
                }
                let mut idx = index;
                if idx < 0 {
                    idx = 0;
                }
                let len = values.len() as i64;
                if len == 0 {
                    return None;
                }
                let wrapped = (idx % len + len) % len;
                values.get(wrapped as usize).cloned()
            }
            _ => None,
        }
    }

    pub fn get_value_for_path(&self, parts: &[String]) -> Option<Value> {
        let (namespace, raw_segments) = Namespace::split_parts(parts);
        let segments: Vec<String> = raw_segments
            .into_iter()
            .map(|segment| segment.to_ascii_lowercase())
            .collect();
        self.lookup_namespace_path(namespace, &segments)
    }

    pub fn with_query(mut self, name: impl Into<String>, value: f64) -> Self {
        self.set_query_value(name, value);
        self
    }

    pub fn set_query_value(&mut self, name: impl Into<String>, value: f64) {
        let key = name.into().to_ascii_lowercase();
        self.values.insert(
            QualifiedName {
                namespace: Namespace::Query,
                key,
            },
            Value::number(value),
        );
    }

    fn assign_nested(&mut self, namespace: Namespace, segments: &[String], value: Value) {
        let key = segments.join(".");
        let mut current = value;
        self.values
            .insert(QualifiedName::new(namespace.clone(), key), current.clone());

        for depth in (1..segments.len()).rev() {
            let parent_key = segments[..depth].join(".");
            let field = segments[depth].clone();
            let existing = self
                .values
                .get(&QualifiedName::new(namespace.clone(), parent_key.clone()))
                .cloned();
            let mut map = match existing {
                Some(Value::Struct(map)) => map,
                _ => IndexMap::new(),
            };
            map.insert(field, current.clone());
            current = Value::Struct(map.clone());
            self.values.insert(
                QualifiedName::new(namespace.clone(), parent_key),
                Value::Struct(map),
            );
        }
    }

    fn lookup_namespace_path(&self, namespace: Namespace, segments: &[String]) -> Option<Value> {
        let key = segments.join(".");
        if let Some(value) = self
            .values
            .get(&QualifiedName::new(namespace.clone(), key.clone()))
        {
            return Some(value.clone());
        }

        for depth in (1..=segments.len()).rev() {
            let prefix = segments[..depth].join(".");
            if let Some(value) = self
                .values
                .get(&QualifiedName::new(namespace.clone(), prefix.clone()))
            {
                if depth == segments.len() {
                    return Some(value.clone());
                }
                if let Some(found) = lookup_nested_value(value, &segments[depth..]) {
                    return Some(found);
                }
            }
        }

        None
    }
}

fn lookup_nested_value(value: &Value, tail: &[String]) -> Option<Value> {
    if tail.is_empty() {
        return Some(value.clone());
    }
    match value {
        Value::Struct(map) => {
            let key = &tail[0];
            map.get(key)
                .and_then(|child| lookup_nested_value(child, &tail[1..]))
        }
        Value::Array(values) => {
            if tail.len() == 1 && tail[0] == "length" {
                Some(Value::number(values.len() as f64))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_canonical_path(canonical: &str) -> Option<(Namespace, Vec<String>)> {
    let mut iter = canonical.split('.');
    let ns = iter.next()?;
    let namespace = Namespace::from_prefix(ns)?;
    let segments = iter.map(|segment| segment.to_string()).collect();
    Some((namespace, segments))
}
