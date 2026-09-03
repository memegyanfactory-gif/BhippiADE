//! The Godot text scene format (`.tscn`) and text resource format (`.tres`).
//!
//! # Why the parser is tolerant
//!
//! Godot's `VariantWriter` can emit dozens of constructor forms (`Transform3D`, `Basis`,
//! `Projection`, `PackedFloat32Array`, `Object(InputEventKey, …)`) and gains more with each
//! release. A parser that failed on an unfamiliar one would refuse to open a scene the
//! editor had just saved; a parser that *dropped* it would silently delete the user's work.
//!
//! So every value carries a fallback: [`TscnValue::Raw`] holds the source text verbatim.
//! And the typed parse is **verified** — [`parse_value`] re-serialises whatever it built and
//! compares the result with the source; a mismatch of even one character keeps the raw text
//! instead. The consequence is the guarantee the tests assert: for any input,
//! `serialize(parse(input)) == input`.
//!
//! # Float formatting
//!
//! Godot prints floats in two different ways and both are reproduced here:
//!
//! * A **property** float goes through `VariantWriter`'s `FLOAT` arm, which appends `.0`
//!   when the shortest representation has neither `.` nor `e` — `fov = 70.0`.
//! * A float **inside a constructor** (`Vector3`, `Color`, `Transform3D`, …) goes through
//!   `rtos`, which does not — `position = Vector3(0, 1, 0)`.
//!
//! Anything this rule cannot reproduce (`1e-05`, `inf`, `nan`) fails the verification above
//! and is kept as raw text, so the difference never reaches a file.
//!
//! # Line endings
//!
//! Godot writes LF. A file carrying CRLF is normalised to LF on parse (and emitted as LF),
//! because a mixed-ending scene file is a Git artefact rather than something the editor
//! produced. Round-trip byte-identity is therefore stated for LF input.

use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fmt::Write as _;

/// The `format=` version Bhippi writes and expects. Godot 4 scenes are format 3.
pub const TSCN_FORMAT: u32 = 3;

/// The largest file this parser will accept, in bytes. A `.tscn` is text the editor wrote;
/// anything past this is not a scene and refusing it keeps a mis-click from allocating a
/// gigabyte of `String`.
pub const MAX_TSCN_BYTES: usize = 32 * 1024 * 1024;

// ── values ───────────────────────────────────────────────────────────────────────────

/// One Godot variant as it appears in a text scene.
///
/// The typed variants are the ones Bhippi reads and writes deliberately. Everything else —
/// `Transform3D`, `Basis`, `AABB`, `PackedFloat32Array`, `Object(…)`, typed arrays — parses
/// into [`TscnValue::Raw`] and is written back unchanged.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TscnValue {
    #[serde(alias = "Int")]
    Int(i64),
    #[serde(alias = "Float")]
    Float(f64),
    #[serde(alias = "Bool")]
    Bool(bool),
    #[serde(alias = "Null")]
    Null,
    #[serde(alias = "Str")]
    Str(String),
    #[serde(alias = "Vector2")]
    Vector2(f64, f64),
    #[serde(alias = "Vector3")]
    Vector3(f64, f64, f64),
    #[serde(alias = "Color")]
    Color(f64, f64, f64, f64),
    #[serde(alias = "NodePath")]
    NodePath(String),
    #[serde(alias = "StringName")]
    StringName(String),
    #[serde(alias = "ExtResource")]
    ExtResource(String),
    #[serde(alias = "SubResource")]
    SubResource(String),
    #[serde(alias = "Array")]
    Array(Vec<TscnValue>),
    #[serde(alias = "Dict")]
    Dict(Vec<(String, TscnValue)>),
    /// Source text kept verbatim: either a form this parser does not model, or a typed
    /// value whose re-serialisation would not have reproduced the input byte for byte.
    #[serde(alias = "Raw")]
    Raw(String),
}

impl TscnValue {
    /// A `Str` from anything string-like.
    #[must_use]
    pub fn str(value: impl Into<String>) -> Self {
        Self::Str(value.into())
    }

    /// The value as Godot would print it.
    #[must_use]
    pub fn to_text(&self) -> String {
        match self {
            Self::Int(value) => value.to_string(),
            Self::Float(value) => format_property_float(*value),
            Self::Bool(value) => (if *value { "true" } else { "false" }).to_owned(),
            Self::Null => "null".to_owned(),
            Self::Str(value) => quote(value),
            Self::Vector2(x, y) => {
                format!("Vector2({}, {})", component(*x), component(*y))
            }
            Self::Vector3(x, y, z) => format!(
                "Vector3({}, {}, {})",
                component(*x),
                component(*y),
                component(*z)
            ),
            Self::Color(r, g, b, a) => format!(
                "Color({}, {}, {}, {})",
                component(*r),
                component(*g),
                component(*b),
                component(*a)
            ),
            Self::NodePath(path) => format!("NodePath({})", quote(path)),
            Self::StringName(name) => format!("&{}", quote(name)),
            Self::ExtResource(id) => format!("ExtResource({})", quote(id)),
            Self::SubResource(id) => format!("SubResource({})", quote(id)),
            Self::Array(items) => {
                let rendered: Vec<String> = items.iter().map(Self::to_text).collect();
                format!("[{}]", rendered.join(", "))
            }
            Self::Dict(entries) => {
                if entries.is_empty() {
                    return "{}".to_owned();
                }
                // Godot's dictionary writer opens with "{\n", puts every entry on its own
                // unindented line and closes with "\n}".
                let rendered: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| format!("{}: {}", quote(key), value.to_text()))
                    .collect();
                format!("{{\n{}\n}}", rendered.join(",\n"))
            }
            Self::Raw(text) => text.clone(),
        }
    }

    /// The string behind a `Str`, `NodePath`, `StringName`, `ExtResource` or `SubResource`.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(value)
            | Self::NodePath(value)
            | Self::StringName(value)
            | Self::ExtResource(value)
            | Self::SubResource(value) => Some(value),
            _ => None,
        }
    }

    /// The resource id behind `ExtResource("…")` / `SubResource("…")`.
    #[must_use]
    pub fn as_resource_id(&self) -> Option<&str> {
        match self {
            Self::ExtResource(id) | Self::SubResource(id) => Some(id),
            _ => None,
        }
    }

    /// The strings of a `["a", "b"]` array, ignoring non-string entries.
    #[must_use]
    pub fn as_string_list(&self) -> Vec<String> {
        match self {
            Self::Array(items) => items
                .iter()
                .filter_map(|item| match item {
                    Self::Str(value) => Some(value.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// Godot's `rtos`: the shortest representation, with no `.0` appended.
fn component(value: f64) -> String {
    format!("{value}")
}

/// Godot's `VariantWriter` FLOAT arm: `rtos`, then `.0` when the result looks like an int.
fn format_property_float(value: f64) -> String {
    let text = component(value);
    if text.contains('.') || text.contains('e') || text.contains('E') || !value.is_finite() {
        text
    } else {
        format!("{text}.0")
    }
}

/// [`TscnValue::Float`] formatting as a free function, for callers building raw text
/// (`project.godot` writes an input action's deadzone this way).
#[must_use]
pub fn float_text(value: f64) -> String {
    format_property_float(value)
}

/// Godot's `c_escape` quoting as a free function, for callers building raw text.
#[must_use]
pub fn quote_text(value: &str) -> String {
    quote(value)
}

/// Godot's `String::c_escape` for the escapes the text formats actually use.
fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Parse one value, falling back to [`TscnValue::Raw`] whenever the typed parse would not
/// reproduce the source exactly. See the module docs for why that fallback is the point.
#[must_use]
pub fn parse_value(source: &str) -> TscnValue {
    let trimmed = source.trim();
    if let Some(value) = typed_value(trimmed) {
        if value.to_text() == trimmed {
            return value;
        }
    }
    TscnValue::Raw(trimmed.to_owned())
}

fn typed_value(text: &str) -> Option<TscnValue> {
    let mut cursor = Cursor::new(text);
    let value = cursor.value()?;
    cursor.skip_ws();
    if cursor.done() {
        Some(value)
    } else {
        None
    }
}

struct Cursor<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, at: 0 }
    }

    fn done(&self) -> bool {
        self.at >= self.text.len()
    }

    fn rest(&self) -> &'a str {
        self.text.get(self.at..).unwrap_or("")
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self, character: char) -> bool {
        if self.peek() == Some(character) {
            self.at += character.len_utf8();
            true
        } else {
            false
        }
    }

    fn skip_ws(&mut self) {
        while let Some(character) = self.peek() {
            if character.is_whitespace() {
                self.at += character.len_utf8();
            } else {
                break;
            }
        }
    }

    fn value(&mut self) -> Option<TscnValue> {
        self.skip_ws();
        match self.peek()? {
            '"' => self.string().map(TscnValue::Str),
            '&' => {
                self.at += 1;
                self.string().map(TscnValue::StringName)
            }
            '[' => self.array(),
            '{' => self.dict(),
            '-' | '0'..='9' => self.number(),
            character if character.is_ascii_alphabetic() || character == '_' => self.word(),
            _ => None,
        }
    }

    fn string(&mut self) -> Option<String> {
        if !self.bump('"') {
            return None;
        }
        let mut out = String::new();
        loop {
            let character = self.peek()?;
            self.at += character.len_utf8();
            match character {
                '"' => return Some(out),
                '\\' => {
                    let escaped = self.peek()?;
                    self.at += escaped.len_utf8();
                    match escaped {
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        '\\' => out.push('\\'),
                        '"' => out.push('"'),
                        '\'' => out.push('\''),
                        // \uXXXX and friends are not reproduced by `quote`, so refusing
                        // here sends the whole value down the raw path where it belongs.
                        _ => return None,
                    }
                }
                other => out.push(other),
            }
        }
    }

    fn number(&mut self) -> Option<TscnValue> {
        let start = self.at;
        if self.peek() == Some('-') {
            self.at += 1;
        }
        let mut digits = 0usize;
        let mut fractional = false;
        while let Some(character) = self.peek() {
            match character {
                '0'..='9' => {
                    digits += 1;
                    self.at += 1;
                }
                '.' if !fractional => {
                    fractional = true;
                    self.at += 1;
                }
                'e' | 'E' => {
                    fractional = true;
                    self.at += 1;
                    if matches!(self.peek(), Some('+' | '-')) {
                        self.at += 1;
                    }
                }
                _ => break,
            }
        }
        if digits == 0 {
            return None;
        }
        let literal = self.text.get(start..self.at)?;
        if fractional {
            literal.parse::<f64>().ok().map(TscnValue::Float)
        } else {
            literal.parse::<i64>().ok().map(TscnValue::Int)
        }
    }

    fn word(&mut self) -> Option<TscnValue> {
        let start = self.at;
        while let Some(character) = self.peek() {
            if character.is_ascii_alphanumeric() || character == '_' {
                self.at += character.len_utf8();
            } else {
                break;
            }
        }
        let word = self.text.get(start..self.at)?;
        match word {
            "true" => return Some(TscnValue::Bool(true)),
            "false" => return Some(TscnValue::Bool(false)),
            "null" => return Some(TscnValue::Null),
            _ => {}
        }
        if self.peek() != Some('(') {
            return None;
        }
        self.at += 1;
        let value = self.constructor(word)?;
        self.skip_ws();
        if self.bump(')') {
            Some(value)
        } else {
            None
        }
    }

    fn constructor(&mut self, name: &str) -> Option<TscnValue> {
        match name {
            "Vector2" => {
                let x = self.float_argument()?;
                self.comma()?;
                let y = self.float_argument()?;
                Some(TscnValue::Vector2(x, y))
            }
            "Vector3" => {
                let x = self.float_argument()?;
                self.comma()?;
                let y = self.float_argument()?;
                self.comma()?;
                let z = self.float_argument()?;
                Some(TscnValue::Vector3(x, y, z))
            }
            "Color" => {
                let r = self.float_argument()?;
                self.comma()?;
                let g = self.float_argument()?;
                self.comma()?;
                let b = self.float_argument()?;
                self.comma()?;
                let a = self.float_argument()?;
                Some(TscnValue::Color(r, g, b, a))
            }
            "NodePath" => {
                self.skip_ws();
                self.string().map(TscnValue::NodePath)
            }
            "ExtResource" => {
                self.skip_ws();
                self.string().map(TscnValue::ExtResource)
            }
            "SubResource" => {
                self.skip_ws();
                self.string().map(TscnValue::SubResource)
            }
            _ => None,
        }
    }

    fn comma(&mut self) -> Option<()> {
        self.skip_ws();
        if self.bump(',') {
            Some(())
        } else {
            None
        }
    }

    fn float_argument(&mut self) -> Option<f64> {
        self.skip_ws();
        match self.number()? {
            TscnValue::Int(value) => {
                // i64 -> f64 is lossless for the magnitudes a scene file carries; anything
                // larger would not have round-tripped anyway and falls back to raw.
                let narrowed = value as f64;
                if narrowed as i64 == value {
                    Some(narrowed)
                } else {
                    None
                }
            }
            TscnValue::Float(value) => Some(value),
            _ => None,
        }
    }

    fn array(&mut self) -> Option<TscnValue> {
        if !self.bump('[') {
            return None;
        }
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if self.bump(']') {
                return Some(TscnValue::Array(items));
            }
            if !items.is_empty() && !self.bump(',') {
                return None;
            }
            self.skip_ws();
            if self.bump(']') {
                return Some(TscnValue::Array(items));
            }
            items.push(self.value()?);
        }
    }

    fn dict(&mut self) -> Option<TscnValue> {
        if !self.bump('{') {
            return None;
        }
        let mut entries: Vec<(String, TscnValue)> = Vec::new();
        loop {
            self.skip_ws();
            if self.bump('}') {
                return Some(TscnValue::Dict(entries));
            }
            if !entries.is_empty() && !self.bump(',') {
                return None;
            }
            self.skip_ws();
            if self.bump('}') {
                return Some(TscnValue::Dict(entries));
            }
            let key = self.string()?;
            self.skip_ws();
            if !self.bump(':') {
                return None;
            }
            entries.push((key, self.value()?));
        }
    }
}

// ── documents ────────────────────────────────────────────────────────────────────────

/// Which text format the document is: a scene (`[gd_scene]`) or a resource (`[gd_resource]`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TscnKind {
    Scene,
    Resource,
}

/// The first line of the file.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TscnHeader {
    pub kind: TscnKind,
    /// `[gd_resource type="…"]` only.
    #[serde(default)]
    pub type_: Option<String>,
    /// Godot omits `load_steps` when it would be 1.
    #[serde(default)]
    pub load_steps: Option<u32>,
    pub format: u32,
    #[serde(default)]
    pub uid: Option<String>,
    #[serde(default)]
    pub script_class: Option<String>,
    /// The attribute order the file was parsed with. Empty means "write the canonical
    /// order", which is what a document Bhippi built from scratch gets.
    #[serde(default)]
    pub order: Vec<String>,
}

/// `[ext_resource type="Script" path="res://scripts/player.gd" id="1_a1b2c"]`
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ExtResource {
    pub type_: String,
    pub path: String,
    pub id: String,
    #[serde(default)]
    pub uid: Option<String>,
    #[serde(default)]
    pub order: Vec<String>,
}

/// `[sub_resource type="BoxShape3D" id="BoxShape3D_1"]` and its property lines.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SubResource {
    pub type_: String,
    pub id: String,
    #[serde(default)]
    pub properties: Vec<(String, TscnValue)>,
    #[serde(default)]
    pub order: Vec<String>,
}

/// One `[node …]` block.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TscnNode {
    pub name: String,
    /// `None` for the scene root, `Some(".")` for a direct child of the root, otherwise the
    /// parent's node path (`"Player"`, `"HUD/Control"`).
    #[serde(default)]
    pub parent: Option<String>,
    /// `None` when the node is an instance of another scene.
    #[serde(default)]
    pub type_: Option<String>,
    #[serde(default)]
    pub instance: Option<TscnValue>,
    #[serde(default)]
    pub instance_placeholder: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub index: Option<i64>,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub node_paths: Option<TscnValue>,
    #[serde(default)]
    pub properties: Vec<(String, TscnValue)>,
    #[serde(default)]
    pub order: Vec<String>,
}

impl TscnNode {
    /// A typed node with nothing but a name, a parent and a type.
    #[must_use]
    pub fn new(name: impl Into<String>, type_: impl Into<String>, parent: Option<&str>) -> Self {
        Self {
            name: name.into(),
            parent: parent.map(str::to_owned),
            type_: Some(type_.into()),
            instance: None,
            instance_placeholder: None,
            owner: None,
            index: None,
            groups: Vec::new(),
            node_paths: None,
            properties: Vec::new(),
            order: Vec::new(),
        }
    }

    /// Set (or replace) one property, keeping the existing position when it is already set.
    pub fn set(&mut self, key: &str, value: TscnValue) {
        if let Some(slot) = self
            .properties
            .iter_mut()
            .find(|(name, _)| name.as_str() == key)
        {
            slot.1 = value;
        } else {
            self.properties.push((key.to_owned(), value));
        }
    }

    /// Chainable [`TscnNode::set`], for building scenes in one expression.
    #[must_use]
    pub fn with(mut self, key: &str, value: TscnValue) -> Self {
        self.set(key, value);
        self
    }

    /// Chainable group assignment.
    #[must_use]
    pub fn in_groups(mut self, groups: &[&str]) -> Self {
        self.groups = groups.iter().map(|group| (*group).to_owned()).collect();
        self
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&TscnValue> {
        self.properties
            .iter()
            .find(|(name, _)| name.as_str() == key)
            .map(|(_, value)| value)
    }

    /// Remove one property; `true` when it was there.
    pub fn remove(&mut self, key: &str) -> bool {
        let before = self.properties.len();
        self.properties.retain(|(name, _)| name.as_str() != key);
        self.properties.len() != before
    }
}

/// `[connection signal="pressed" from="UI/Button" to="." method="_on_button_pressed"]`
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct Connection {
    pub signal: String,
    pub from: String,
    pub to: String,
    pub method: String,
    #[serde(default)]
    pub flags: Option<i64>,
    #[serde(default)]
    pub binds: Option<TscnValue>,
    #[serde(default)]
    pub unbinds: Option<i64>,
    #[serde(default)]
    pub order: Vec<String>,
}

/// A parsed `.tscn` / `.tres` document.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TscnDocument {
    pub header: TscnHeader,
    #[serde(default)]
    pub ext_resources: Vec<ExtResource>,
    #[serde(default)]
    pub sub_resources: Vec<SubResource>,
    /// The `[resource]` block of a `.tres`.
    #[serde(default)]
    pub resource: Option<Vec<(String, TscnValue)>>,
    #[serde(default)]
    pub nodes: Vec<TscnNode>,
    #[serde(default)]
    pub connections: Vec<Connection>,
    #[serde(default)]
    pub editables: Vec<String>,
}

impl TscnDocument {
    /// An empty scene with one typed root node.
    #[must_use]
    pub fn new_scene(root_name: &str, root_type: &str) -> Self {
        Self {
            header: TscnHeader {
                kind: TscnKind::Scene,
                type_: None,
                load_steps: None,
                format: TSCN_FORMAT,
                uid: None,
                script_class: None,
                order: Vec::new(),
            },
            ext_resources: Vec::new(),
            sub_resources: Vec::new(),
            resource: None,
            nodes: vec![TscnNode::new(root_name, root_type, None)],
            connections: Vec::new(),
            editables: Vec::new(),
        }
    }

    /// `load_steps` as Godot computes it: one per external resource, one per sub-resource,
    /// plus one for the scene itself. Godot omits the attribute entirely when the total is
    /// 1, which is what `None` means here.
    #[must_use]
    pub fn computed_load_steps(&self) -> Option<u32> {
        let steps = self.ext_resources.len() + self.sub_resources.len() + 1;
        let steps = u32::try_from(steps).unwrap_or(u32::MAX);
        if steps > 1 {
            Some(steps)
        } else {
            None
        }
    }

    /// Recompute `load_steps` after an edit. Every mutation path calls this, because a
    /// stale count makes Godot allocate the wrong number of load slots.
    pub fn refresh_load_steps(&mut self) {
        self.header.load_steps = self.computed_load_steps();
    }

    #[must_use]
    pub fn root(&self) -> Option<&TscnNode> {
        self.nodes.iter().find(|node| node.parent.is_none())
    }

    #[must_use]
    pub fn ext_resource(&self, id: &str) -> Option<&ExtResource> {
        self.ext_resources.iter().find(|resource| resource.id == id)
    }

    #[must_use]
    pub fn ext_resource_by_path(&self, path: &str) -> Option<&ExtResource> {
        let wanted = super::rel_to_res(path);
        self.ext_resources
            .iter()
            .find(|resource| resource.path == wanted)
    }

    /// Find (or add) an `ext_resource` for `path`, returning its id. Ids follow Godot's
    /// `<n>_<slug>` shape; the slug is derived from the file stem so a generated scene is
    /// still readable in a diff.
    pub fn ensure_ext_resource(&mut self, type_: &str, path: &str) -> String {
        let res_path = super::rel_to_res(path);
        if let Some(existing) = self
            .ext_resources
            .iter()
            .find(|resource| resource.path == res_path)
        {
            return existing.id.clone();
        }
        let stem = res_path
            .rsplit('/')
            .next()
            .unwrap_or("res")
            .split('.')
            .next()
            .unwrap_or("res");
        let slug: String = stem
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        let mut index = self.ext_resources.len() + 1;
        let mut id = format!("{index}_{slug}");
        while self.ext_resources.iter().any(|resource| resource.id == id) {
            index += 1;
            id = format!("{index}_{slug}");
        }
        self.ext_resources.push(ExtResource {
            type_: type_.to_owned(),
            path: res_path,
            id: id.clone(),
            uid: None,
            order: Vec::new(),
        });
        self.refresh_load_steps();
        id
    }

    /// Drop external resources nothing references any more (after removing a node, say).
    /// Returns the ids that were dropped.
    pub fn prune_ext_resources(&mut self) -> Vec<String> {
        let used = self.referenced_ext_resource_ids();
        let dropped: Vec<String> = self
            .ext_resources
            .iter()
            .filter(|resource| !used.contains(&resource.id))
            .map(|resource| resource.id.clone())
            .collect();
        if dropped.is_empty() {
            return dropped;
        }
        self.ext_resources
            .retain(|resource| used.contains(&resource.id));
        self.refresh_load_steps();
        dropped
    }

    fn referenced_ext_resource_ids(&self) -> std::collections::BTreeSet<String> {
        let mut used = std::collections::BTreeSet::new();
        let mut visit = |value: &TscnValue| collect_ext_ids(value, &mut used);
        for node in &self.nodes {
            if let Some(instance) = &node.instance {
                visit(instance);
            }
            for (_, value) in &node.properties {
                visit(value);
            }
        }
        for sub in &self.sub_resources {
            for (_, value) in &sub.properties {
                visit(value);
            }
        }
        if let Some(resource) = &self.resource {
            for (_, value) in resource {
                visit(value);
            }
        }
        used
    }

    /// Every node's stable path (`"."` for the root).
    #[must_use]
    pub fn node_paths(&self) -> Vec<String> {
        self.nodes.iter().map(node_path).collect()
    }

    #[must_use]
    pub fn node(&self, path: &str) -> Option<&TscnNode> {
        self.nodes.iter().find(|node| node_path(node) == path)
    }

    pub fn node_mut(&mut self, path: &str) -> Option<&mut TscnNode> {
        self.nodes.iter_mut().find(|node| node_path(node) == path)
    }

    /// Render the document exactly as Godot would lay it out.
    ///
    /// Layout: the header line, then a blank line before each group — the `ext_resource`
    /// lines as one block, then one block per `sub_resource`, per `node`, the `[resource]`
    /// block of a `.tres`, the connections and finally the `editable` lines. There is no
    /// trailing blank line.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.header_line());
        out.push('\n');

        if !self.ext_resources.is_empty() {
            out.push('\n');
            for resource in &self.ext_resources {
                out.push_str(&resource.header_line());
                out.push('\n');
            }
        }
        for sub in &self.sub_resources {
            out.push('\n');
            out.push_str(&sub.header_line());
            out.push('\n');
            write_properties(&mut out, &sub.properties);
        }
        if let Some(resource) = &self.resource {
            out.push_str("\n[resource]\n");
            write_properties(&mut out, resource);
        }
        for node in &self.nodes {
            out.push('\n');
            out.push_str(&node.header_line());
            out.push('\n');
            write_properties(&mut out, &node.properties);
        }
        if !self.connections.is_empty() {
            out.push('\n');
            for connection in &self.connections {
                out.push_str(&connection.header_line());
                out.push('\n');
            }
        }
        if !self.editables.is_empty() {
            out.push('\n');
            for path in &self.editables {
                let _ = writeln!(out, "[editable path={}]", quote(path));
            }
        }
        out
    }

    fn header_line(&self) -> String {
        let tag = match self.header.kind {
            TscnKind::Scene => "gd_scene",
            TscnKind::Resource => "gd_resource",
        };
        render_header(
            tag,
            &self.header.order,
            &["type", "script_class", "load_steps", "format", "uid"],
            |key| match key {
                "type" => self
                    .header
                    .type_
                    .as_ref()
                    .map(|value| format!("type={}", quote(value))),
                "script_class" => self
                    .header
                    .script_class
                    .as_ref()
                    .map(|value| format!("script_class={}", quote(value))),
                "load_steps" => self
                    .header
                    .load_steps
                    .map(|steps| format!("load_steps={steps}")),
                "format" => Some(format!("format={}", self.header.format)),
                "uid" => self
                    .header
                    .uid
                    .as_ref()
                    .map(|value| format!("uid={}", quote(value))),
                _ => None,
            },
        )
    }
}

fn collect_ext_ids(value: &TscnValue, into: &mut std::collections::BTreeSet<String>) {
    match value {
        TscnValue::ExtResource(id) => {
            into.insert(id.clone());
        }
        TscnValue::Array(items) => {
            for item in items {
                collect_ext_ids(item, into);
            }
        }
        TscnValue::Dict(entries) => {
            for (_, item) in entries {
                collect_ext_ids(item, into);
            }
        }
        TscnValue::Raw(text) => {
            // A raw value can still name a resource (`Array[Texture2D]([ExtResource("1_a")])`).
            // Missing one would prune a resource the scene is using, so the raw text is
            // scanned for the literal call.
            let mut rest = text.as_str();
            while let Some(start) = rest.find("ExtResource(\"") {
                let after = start + "ExtResource(\"".len();
                let Some(tail) = rest.get(after..) else { break };
                let Some(end) = tail.find('"') else { break };
                if let Some(id) = tail.get(..end) {
                    into.insert(id.to_owned());
                }
                rest = tail.get(end..).unwrap_or("");
            }
        }
        _ => {}
    }
}

/// The stable path of one node: `"."` for the root, `"Player/Mesh"` for a grandchild.
#[must_use]
pub fn node_path(node: &TscnNode) -> String {
    match node.parent.as_deref() {
        None => ".".to_owned(),
        Some(parent) => super::join_node_path(parent, &node.name),
    }
}

fn write_properties(out: &mut String, properties: &[(String, TscnValue)]) {
    for (key, value) in properties {
        let _ = writeln!(out, "{key} = {}", value.to_text());
    }
}

fn render_header(
    tag: &str,
    order: &[String],
    canonical: &[&str],
    attribute: impl Fn(&str) -> Option<String>,
) -> String {
    let mut out = format!("[{tag}");
    let mut written: Vec<&str> = Vec::new();
    for key in order {
        if let Some(fragment) = attribute(key) {
            out.push(' ');
            out.push_str(&fragment);
            written.push(key.as_str());
        }
    }
    for key in canonical {
        if written.contains(key) {
            continue;
        }
        if let Some(fragment) = attribute(key) {
            out.push(' ');
            out.push_str(&fragment);
        }
    }
    out.push(']');
    out
}

impl ExtResource {
    fn header_line(&self) -> String {
        render_header(
            "ext_resource",
            &self.order,
            &["type", "uid", "path", "id"],
            |key| match key {
                "type" => Some(format!("type={}", quote(&self.type_))),
                "uid" => self.uid.as_ref().map(|uid| format!("uid={}", quote(uid))),
                "path" => Some(format!("path={}", quote(&self.path))),
                "id" => Some(format!("id={}", quote(&self.id))),
                _ => None,
            },
        )
    }
}

impl SubResource {
    fn header_line(&self) -> String {
        render_header(
            "sub_resource",
            &self.order,
            &["type", "id"],
            |key| match key {
                "type" => Some(format!("type={}", quote(&self.type_))),
                "id" => Some(format!("id={}", quote(&self.id))),
                _ => None,
            },
        )
    }
}

impl TscnNode {
    /// The `[node …]` line. Attribute order follows Godot's own writer:
    /// `name`, `type`, `parent`, `instance` / `instance_placeholder`, `owner`, `index`,
    /// `groups`, `node_paths` — unless the node was parsed, in which case its own order is
    /// reproduced and any newly set attribute is appended.
    fn header_line(&self) -> String {
        render_header(
            "node",
            &self.order,
            &[
                "name",
                "type",
                "parent",
                "instance",
                "instance_placeholder",
                "owner",
                "index",
                "groups",
                "node_paths",
            ],
            |key| match key {
                "name" => Some(format!("name={}", quote(&self.name))),
                "type" => self
                    .type_
                    .as_ref()
                    .map(|value| format!("type={}", quote(value))),
                "parent" => self
                    .parent
                    .as_ref()
                    .map(|value| format!("parent={}", quote(value))),
                "instance" => self
                    .instance
                    .as_ref()
                    .map(|value| format!("instance={}", value.to_text())),
                "instance_placeholder" => self
                    .instance_placeholder
                    .as_ref()
                    .map(|value| format!("instance_placeholder={}", quote(value))),
                "owner" => self
                    .owner
                    .as_ref()
                    .map(|value| format!("owner={}", quote(value))),
                "index" => self.index.map(|index| format!("index=\"{index}\"")),
                "groups" => {
                    if self.groups.is_empty() {
                        None
                    } else {
                        let rendered: Vec<String> =
                            self.groups.iter().map(|group| quote(group)).collect();
                        Some(format!("groups=[{}]", rendered.join(", ")))
                    }
                }
                "node_paths" => self
                    .node_paths
                    .as_ref()
                    .map(|value| format!("node_paths={}", value.to_text())),
                _ => None,
            },
        )
    }
}

impl Connection {
    fn header_line(&self) -> String {
        render_header(
            "connection",
            &self.order,
            &[
                "signal", "from", "to", "method", "flags", "binds", "unbinds",
            ],
            |key| match key {
                "signal" => Some(format!("signal={}", quote(&self.signal))),
                "from" => Some(format!("from={}", quote(&self.from))),
                "to" => Some(format!("to={}", quote(&self.to))),
                "method" => Some(format!("method={}", quote(&self.method))),
                "flags" => self.flags.map(|flags| format!("flags={flags}")),
                "binds" => self
                    .binds
                    .as_ref()
                    .map(|value| format!("binds={}", value.to_text())),
                "unbinds" => self.unbinds.map(|value| format!("unbinds={value}")),
                _ => None,
            },
        )
    }
}

// ── parsing ──────────────────────────────────────────────────────────────────────────

/// One `[tag key=value …]` line, split but not yet interpreted.
pub(crate) struct SectionHeader {
    pub(crate) tag: String,
    pub(crate) attributes: Vec<(String, String)>,
}

impl SectionHeader {
    pub(crate) fn order(&self) -> Vec<String> {
        self.attributes.iter().map(|(key, _)| key.clone()).collect()
    }

    pub(crate) fn raw(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    pub(crate) fn text(&self, key: &str) -> Option<String> {
        match parse_value(self.raw(key)?) {
            TscnValue::Str(value) => Some(value),
            other => Some(other.to_text()),
        }
    }

    pub(crate) fn int(&self, key: &str) -> Option<i64> {
        match parse_value(self.raw(key)?) {
            TscnValue::Int(value) => Some(value),
            // Godot writes `index="3"` — the number is quoted.
            TscnValue::Str(value) => value.parse().ok(),
            _ => None,
        }
    }
}

/// Split `[tag a="1" b=Vector3(0, 0, 0)]` into its tag and raw attribute texts.
pub(crate) fn parse_section_header(line: &str) -> Option<SectionHeader> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let bytes = inner.as_bytes();
    let mut at = 0usize;
    while at < bytes.len() && !bytes[at].is_ascii_whitespace() {
        at += 1;
    }
    let tag = inner.get(..at)?.to_owned();
    if tag.is_empty() {
        return None;
    }
    let mut attributes = Vec::new();
    while at < bytes.len() {
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at >= bytes.len() {
            break;
        }
        let key_start = at;
        while at < bytes.len() && bytes[at] != b'=' && !bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        let key = inner.get(key_start..at)?.to_owned();
        if at >= bytes.len() || bytes[at] != b'=' {
            return None;
        }
        at += 1;
        let value_start = at;
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escaped = false;
        while at < bytes.len() {
            let byte = bytes[at];
            if in_string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    in_string = false;
                }
            } else {
                match byte {
                    b'"' => in_string = true,
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => depth -= 1,
                    b' ' | b'\t' if depth == 0 => break,
                    _ => {}
                }
            }
            at += 1;
        }
        attributes.push((key, inner.get(value_start..at)?.to_owned()));
    }
    Some(SectionHeader { tag, attributes })
}

/// True when `line` opens a new section (`[node …]`) at column 0.
pub(crate) fn is_section_line(line: &str) -> bool {
    line.starts_with('[') && line.ends_with(']')
}

/// True when `line` starts a `key = value` (or `key=value`) assignment at column 0. This is
/// the rule that decides where a multi-line value ends.
pub(crate) fn is_key_line(line: &str) -> bool {
    let mut chars = line.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    // A leading digit is legal: `project.godot` carries `3d/default_gravity=9.8`.
    if !(first.is_ascii_alphanumeric() || first == '_') {
        return false;
    }
    let mut seen_equals = false;
    for character in line.chars() {
        if character == '=' {
            seen_equals = true;
            break;
        }
        if !(character.is_ascii_alphanumeric()
            || matches!(character, '_' | '/' | '.' | ':' | '-' | ' '))
        {
            return false;
        }
    }
    seen_equals
}

/// Normalise line endings and reject files that are not text scenes at all.
fn prepare(text: &str, what: &str) -> Result<String> {
    if text.len() > MAX_TSCN_BYTES {
        return Err(EngineError::Scene(
            format!("{what} is {} bytes, past the parser cap", text.len()),
            Some(
                "Open the file in Godot; Bhippi only edits scenes it can hold in memory."
                    .to_owned(),
            ),
        ));
    }
    Ok(text.replace("\r\n", "\n"))
}

/// Parse a `.tscn` or `.tres` document.
pub fn parse(text: &str) -> Result<TscnDocument> {
    let text = prepare(text, "scene")?;
    let mut lines = text.lines();

    // The header is the first non-blank line and must be a section. `lines` is left sitting
    // on the line after it, which is where the section walk below picks up.
    let header_line = loop {
        match lines.next() {
            Some(line) if line.trim().is_empty() => continue,
            Some(line) => break line.trim_end(),
            None => {
                return Err(EngineError::Scene(
                    "the file is empty".to_owned(),
                    Some("A Godot scene starts with a [gd_scene …] line.".to_owned()),
                ))
            }
        }
    };
    let header_section = parse_section_header(header_line).ok_or_else(|| {
        EngineError::Scene(
            format!("expected a [gd_scene …] header, found `{header_line}`"),
            Some("Only Godot 4 text scenes (.tscn/.tres) can be opened here.".to_owned()),
        )
    })?;
    let kind = match header_section.tag.as_str() {
        "gd_scene" => TscnKind::Scene,
        "gd_resource" => TscnKind::Resource,
        other => {
            return Err(EngineError::Scene(
                format!("unknown text-format header `{other}`"),
                Some("Expected [gd_scene …] or [gd_resource …].".to_owned()),
            ))
        }
    };
    let format = header_section
        .int("format")
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(TSCN_FORMAT);
    if format != TSCN_FORMAT {
        return Err(EngineError::Scene(
            format!("scene format {format} is not Godot 4's format {TSCN_FORMAT}"),
            Some("Open the project once in Godot 4 to upgrade its scenes.".to_owned()),
        ));
    }
    let header = TscnHeader {
        kind,
        type_: header_section.text("type"),
        load_steps: header_section
            .int("load_steps")
            .and_then(|value| u32::try_from(value).ok()),
        format,
        uid: header_section.text("uid"),
        script_class: header_section.text("script_class"),
        order: header_section.order(),
    };

    let mut document = TscnDocument {
        header,
        ext_resources: Vec::new(),
        sub_resources: Vec::new(),
        resource: None,
        nodes: Vec::new(),
        connections: Vec::new(),
        editables: Vec::new(),
    };

    // Sections carry property lines; a value may span lines until the next `key =` or the
    // next `[section]`, both at column 0.
    let mut pending: Option<(SectionHeader, Vec<(String, TscnValue)>)> = None;
    let mut buffered: Option<(String, String)> = None;

    let flush_property = |buffered: &mut Option<(String, String)>,
                          into: &mut Vec<(String, TscnValue)>| {
        if let Some((key, raw)) = buffered.take() {
            into.push((key, parse_value(&raw)));
        }
    };

    for line in lines {
        let trimmed_end = line.trim_end();
        if is_section_line(trimmed_end) {
            if let Some((section, mut properties)) = pending.take() {
                flush_property(&mut buffered, &mut properties);
                finish_section(&mut document, section, properties)?;
            }
            buffered = None;
            let section = parse_section_header(trimmed_end).ok_or_else(|| {
                EngineError::Scene(
                    format!("malformed section line `{trimmed_end}`"),
                    Some("Sections look like [node name=\"X\" type=\"Y\"].".to_owned()),
                )
            })?;
            pending = Some((section, Vec::new()));
            continue;
        }
        let Some((_, properties)) = pending.as_mut() else {
            // Blank lines and stray text before the first section are ignored: Godot never
            // writes them, and refusing the file over one would be gratuitous.
            continue;
        };
        if is_key_line(trimmed_end) {
            flush_property(&mut buffered, properties);
            if let Some((key, value)) = trimmed_end.split_once('=') {
                buffered = Some((key.trim_end().to_owned(), value.to_owned()));
            }
            continue;
        }
        if let Some((_, raw)) = buffered.as_mut() {
            raw.push('\n');
            raw.push_str(trimmed_end);
        }
    }
    if let Some((section, mut properties)) = pending.take() {
        flush_property(&mut buffered, &mut properties);
        finish_section(&mut document, section, properties)?;
    }

    Ok(document)
}

fn finish_section(
    document: &mut TscnDocument,
    section: SectionHeader,
    properties: Vec<(String, TscnValue)>,
) -> Result<()> {
    match section.tag.as_str() {
        "ext_resource" => document.ext_resources.push(ExtResource {
            type_: section.text("type").unwrap_or_default(),
            path: section.text("path").unwrap_or_default(),
            id: section.text("id").unwrap_or_default(),
            uid: section.text("uid"),
            order: section.order(),
        }),
        "sub_resource" => document.sub_resources.push(SubResource {
            type_: section.text("type").unwrap_or_default(),
            id: section.text("id").unwrap_or_default(),
            properties,
            order: section.order(),
        }),
        "resource" => document.resource = Some(properties),
        "node" => {
            let name = section.text("name").ok_or_else(|| {
                EngineError::Scene(
                    "a [node] section has no name".to_owned(),
                    Some("Every node line carries name=\"…\".".to_owned()),
                )
            })?;
            document.nodes.push(TscnNode {
                name,
                parent: section.text("parent"),
                type_: section.text("type"),
                instance: section.raw("instance").map(parse_value),
                instance_placeholder: section.text("instance_placeholder"),
                owner: section.text("owner"),
                index: section.int("index"),
                groups: section
                    .raw("groups")
                    .map(|raw| parse_value(raw).as_string_list())
                    .unwrap_or_default(),
                node_paths: section.raw("node_paths").map(parse_value),
                properties,
                order: section.order(),
            });
        }
        "connection" => document.connections.push(Connection {
            signal: section.text("signal").unwrap_or_default(),
            from: section.text("from").unwrap_or_default(),
            to: section.text("to").unwrap_or_default(),
            method: section.text("method").unwrap_or_default(),
            flags: section.int("flags"),
            binds: section.raw("binds").map(parse_value),
            unbinds: section.int("unbinds"),
            order: section.order(),
        }),
        "editable" => {
            if let Some(path) = section.text("path") {
                document.editables.push(path);
            }
        }
        other => {
            return Err(EngineError::Scene(
                format!("unknown section `[{other}]` in a text scene"),
                Some("Bhippi understands gd_scene, ext_resource, sub_resource, resource, node, connection and editable.".to_owned()),
            ))
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse, parse_value, TscnDocument, TscnNode, TscnValue};

    const MAIN: &str = include_str!("../../../../tests/fixtures/godot/main.tscn");
    const HUD: &str = include_str!("../../../../tests/fixtures/godot/hud.tscn");

    fn lf(text: &str) -> String {
        text.replace("\r\n", "\n")
    }

    #[test]
    fn realistic_scenes_round_trip_byte_for_byte() {
        for (name, source) in [("main.tscn", MAIN), ("hud.tscn", HUD)] {
            let source = lf(source);
            let document = parse(&source).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(document.to_text(), source, "{name} must round-trip");
        }
    }

    #[test]
    fn every_value_kind_parses_and_prints_back_the_way_godot_wrote_it() {
        let cases = [
            ("1", TscnValue::Int(1)),
            ("-12", TscnValue::Int(-12)),
            ("70.0", TscnValue::Float(70.0)),
            ("0.5", TscnValue::Float(0.5)),
            ("true", TscnValue::Bool(true)),
            ("false", TscnValue::Bool(false)),
            ("null", TscnValue::Null),
            ("\"Score: 0\"", TscnValue::Str("Score: 0".to_owned())),
            ("Vector2(1, 0.5)", TscnValue::Vector2(1.0, 0.5)),
            ("Vector3(0, 1, 0)", TscnValue::Vector3(0.0, 1.0, 0.0)),
            ("Color(1, 1, 1, 1)", TscnValue::Color(1.0, 1.0, 1.0, 1.0)),
            ("NodePath(\"../X\")", TscnValue::NodePath("../X".to_owned())),
            ("&\"walk\"", TscnValue::StringName("walk".to_owned())),
            (
                "ExtResource(\"1_a1b2c\")",
                TscnValue::ExtResource("1_a1b2c".to_owned()),
            ),
            (
                "SubResource(\"BoxShape3D_1\")",
                TscnValue::SubResource("BoxShape3D_1".to_owned()),
            ),
            (
                "[1, 2]",
                TscnValue::Array(vec![TscnValue::Int(1), TscnValue::Int(2)]),
            ),
            ("[]", TscnValue::Array(Vec::new())),
            ("{}", TscnValue::Dict(Vec::new())),
            (
                "{\n\"k\": 1\n}",
                TscnValue::Dict(vec![("k".to_owned(), TscnValue::Int(1))]),
            ),
        ];
        for (text, expected) in cases {
            let parsed = parse_value(text);
            assert_eq!(parsed, expected, "parsing {text}");
            assert_eq!(parsed.to_text(), text, "printing {text}");
        }
    }

    #[test]
    fn forms_the_parser_does_not_model_survive_verbatim() {
        for text in [
            "Transform3D(1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 0)",
            "PackedStringArray(\"4.7\", \"Forward Plus\")",
            "PackedFloat32Array(0, 0.5, 1)",
            "Array[int]([1, 2])",
            "Rect2(0, 0, 10, 10)",
            "AABB(0, 0, 0, 1, 1, 1)",
            "Plane(0, 1, 0, 0)",
            "Quaternion(0, 0, 0, 1)",
            "Basis(1, 0, 0, 0, 1, 0, 0, 0, 1)",
            "Vector4(0, 0, 0, 1)",
            "1e-05",
            "inf",
            "Object(InputEventKey,\"keycode\":32,\"script\":null)\n",
        ] {
            let parsed = parse_value(text);
            assert!(
                matches!(parsed, TscnValue::Raw(_)),
                "{text} should stay raw, got {parsed:?}"
            );
            assert_eq!(parsed.to_text(), text.trim());
        }
    }

    #[test]
    fn load_steps_follow_the_resource_count_through_an_edit() {
        let mut document = parse(&lf(MAIN)).expect("fixture parses");
        let before = document.header.load_steps;
        assert_eq!(before, document.computed_load_steps());

        let id = document.ensure_ext_resource("Script", "res://scripts/extra.gd");
        assert_eq!(
            document.header.load_steps,
            before.map(|steps| steps + 1),
            "adding an ext_resource bumps load_steps"
        );

        // Nothing references the new script, so pruning takes it (and the count) back.
        let dropped = document.prune_ext_resources();
        assert_eq!(dropped, vec![id]);
        assert_eq!(document.header.load_steps, before);
    }

    #[test]
    fn a_scene_built_from_scratch_prints_the_layout_godot_writes() {
        let mut document = TscnDocument::new_scene("Main", "Node3D");
        document.nodes.push(
            TscnNode::new("Player", "CharacterBody3D", Some("."))
                .in_groups(&["bhippi_track"])
                .with("position", TscnValue::Vector3(0.0, 1.0, 0.0)),
        );
        document.refresh_load_steps();
        let text = document.to_text();
        assert_eq!(
            text,
            "[gd_scene format=3]\n\n[node name=\"Main\" type=\"Node3D\"]\n\n[node name=\"Player\" type=\"CharacterBody3D\" parent=\".\" groups=[\"bhippi_track\"]]\nposition = Vector3(0, 1, 0)\n"
        );
        assert_eq!(parse(&text).expect("re-parses").to_text(), text);
    }

    #[test]
    fn the_fixture_exposes_the_structures_the_editor_reads() {
        let document = parse(&lf(MAIN)).expect("fixture parses");
        assert_eq!(document.root().map(|node| node.name.as_str()), Some("Main"));
        assert!(document
            .ext_resources
            .iter()
            .any(|resource| resource.type_ == "Script"));
        assert!(!document.sub_resources.is_empty());
        assert_eq!(document.connections.len(), 1);
        assert_eq!(document.editables.len(), 1);
        let player = document.node("Player").expect("player node");
        assert!(player.groups.contains(&"bhippi_track".to_owned()));
        assert_eq!(
            player.get("script").and_then(TscnValue::as_resource_id),
            Some("1_a1b2c")
        );
    }
}
