//! Structural code indexer: tree-sitter based symbol extraction per language.
//!
//! Each public function takes raw file bytes and a `file_stem` (used for
//! qualified-name construction) and returns a list of [`SymbolEntry`]
//! values ready to be fed into [`crate::ProjectBrain::scan_file_with_symbols`].

use crate::SymbolEntry;
use bhippi_types::SymbolId;
use std::path::Path;

/// Detect the language from a file extension and extract symbols.
pub fn extract_symbols(rel_path: &str, content: &str) -> Option<Vec<SymbolEntry>> {
    let ext = Path::new(rel_path).extension()?.to_str()?;
    let prefix = rel_path
        .trim_end_matches(Path::new(rel_path).file_name()?.to_str()?)
        .trim_end_matches('/')
        .trim_end_matches('\\');
    match ext {
        "rs" => Some(extract_rust(rel_path, prefix, content)),
        "ts" | "tsx" => Some(extract_typescript(rel_path, prefix, content, ext == "tsx")),
        "js" | "jsx" => Some(extract_javascript(rel_path, prefix, content, ext == "jsx")),
        _ => None,
    }
}

fn qualify(prefix: &str, file_name: &str, scope: &str, name: &str) -> String {
    let base = if prefix.is_empty() {
        file_name.to_owned()
    } else {
        format!("{prefix}/{file_name}")
    };
    if scope.is_empty() {
        format!("{base}::{name}")
    } else {
        format!("{base}::{scope}::{name}")
    }
}

// ── Rust ────────────────────────────────────────────────────────────────

fn extract_rust(rel_path: &str, prefix: &str, content: &str) -> Vec<SymbolEntry> {
    let mut parser = tree_sitter::Parser::new();
    let _ = parser.set_language(&tree_sitter::Language::new(tree_sitter_rust::LANGUAGE));
    let Some(tree) = parser.parse(content.as_bytes(), None) else {
        return Vec::new();
    };
    let file_name = Path::new(rel_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut symbols = Vec::new();
    walk_rust(
        tree.root_node(),
        content,
        prefix,
        &file_name,
        "",
        false,
        &mut symbols,
    );
    symbols
}

fn walk_rust(
    node: tree_sitter::Node<'_>,
    source: &str,
    prefix: &str,
    file_name: &str,
    scope: &str,
    method_mode: bool,
    out: &mut Vec<SymbolEntry>,
) {
    let kind = node.kind();
    match kind {
        "function_item" | "function_signature_item" => {
            let name = node_name(node, source);
            let qn = qualify(prefix, file_name, scope, &name);
            let body = node_text(node, source).to_owned();
            let sig = extract_rust_fn_signature(node, source);
            let symbol_kind = if method_mode { "method" } else { "function" };
            out.push(SymbolEntry {
                id: SymbolId::new(),
                kind: symbol_kind.to_owned(),
                name,
                qualified_name: qn,
                signature: sig,
                body,
                start_line: Some(node.start_position().row as i64 + 1),
                end_line: Some(node.end_position().row as i64 + 1),
                parent_id: None,
            });
        }
        "impl_item" | "trait_item" => {
            let container_name = node_name(node, source);
            let container_kind = if kind == "impl_item" { "impl" } else { "trait" };
            let child_scope = if scope.is_empty() {
                container_name.clone()
            } else {
                format!("{scope}::{container_name}")
            };
            out.push(SymbolEntry {
                id: SymbolId::new(),
                kind: container_kind.to_owned(),
                name: container_name,
                qualified_name: qualify(prefix, file_name, scope, &child_scope),
                signature: None,
                body: node_text(node, source).to_owned(),
                start_line: Some(node.start_position().row as i64 + 1),
                end_line: Some(node.end_position().row as i64 + 1),
                parent_id: None,
            });
            // Descend; functions inside the impl/trait body become methods.
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                walk_rust(child, source, prefix, file_name, &child_scope, true, out);
            }
        }
        "struct_item" | "enum_item" | "type_item" => {
            let name = node_name(node, source);
            let qn = qualify(prefix, file_name, scope, &name);
            let kind_str = match kind {
                "struct_item" => "struct",
                "enum_item" => "enum",
                _ => "type",
            };
            out.push(SymbolEntry {
                id: SymbolId::new(),
                kind: kind_str.to_owned(),
                name,
                qualified_name: qn,
                signature: None,
                body: node_text(node, source).to_owned(),
                start_line: Some(node.start_position().row as i64 + 1),
                end_line: Some(node.end_position().row as i64 + 1),
                parent_id: None,
            });
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                walk_rust(child, source, prefix, file_name, scope, method_mode, out);
            }
        }
        "mod_item" => {
            let name = node_name(node, source);
            let child_scope = if scope.is_empty() {
                name.clone()
            } else {
                format!("{scope}::{name}")
            };
            out.push(SymbolEntry {
                id: SymbolId::new(),
                kind: "module".to_owned(),
                name,
                qualified_name: qualify(prefix, file_name, scope, &child_scope),
                signature: None,
                body: node_text(node, source).to_owned(),
                start_line: Some(node.start_position().row as i64 + 1),
                end_line: Some(node.end_position().row as i64 + 1),
                parent_id: None,
            });
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                walk_rust(
                    child,
                    source,
                    prefix,
                    file_name,
                    &child_scope,
                    method_mode,
                    out,
                );
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                walk_rust(child, source, prefix, file_name, scope, method_mode, out);
            }
        }
    }
}

fn node_name(node: tree_sitter::Node<'_>, source: &str) -> String {
    // For `function_item` the name child is `identifier`.
    // For `impl_item` the name is the type child.
    // For `struct_item`, `enum_item`, `trait_item`, `mod_item` — identifier child.
    if let Some(name_child) = (0..node.child_count())
        .filter_map(|i| node.child(i))
        .find(|c| {
            matches!(
                c.kind(),
                "identifier" | "type_identifier" | "scoped_identifier"
            )
        })
    {
        node_text(name_child, source).to_owned()
    } else {
        "<anonymous>".to_owned()
    }
}

fn extract_rust_fn_signature(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    // Extract signature: `fn name(params) -> RetType`
    // Find the `fn` keyword, then read up to (but not including) the body block.
    let start = node.start_byte();
    let mut end = node.end_byte();
    // Walk children to find the block — signature ends before the block.
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "block" {
                end = child.start_byte();
                break;
            }
        }
    }
    let sig_bytes = &source.as_bytes()[start..end];
    let sig = String::from_utf8_lossy(sig_bytes).trim().to_owned();
    Some(sig)
}

fn node_text<'s>(node: tree_sitter::Node<'_>, source: &'s str) -> &'s str {
    &source[node.start_byte()..node.end_byte()]
}
// ── TypeScript / JavaScript ─────────────────────────────────────────────

fn extract_typescript(
    rel_path: &str,
    prefix: &str,
    content: &str,
    is_tsx: bool,
) -> Vec<SymbolEntry> {
    let mut parser = tree_sitter::Parser::new();
    let lang = if is_tsx {
        tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TSX)
    } else {
        tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT)
    };
    let _ = parser.set_language(&lang);
    let Some(tree) = parser.parse(content.as_bytes(), None) else {
        return Vec::new();
    };
    let file_name = Path::new(rel_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut symbols = Vec::new();
    walk_ts(
        tree.root_node(),
        content,
        prefix,
        &file_name,
        "",
        &mut symbols,
    );
    symbols
}

fn extract_javascript(
    rel_path: &str,
    prefix: &str,
    content: &str,
    is_jsx: bool,
) -> Vec<SymbolEntry> {
    let mut parser = tree_sitter::Parser::new();
    let lang = if is_jsx {
        tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TSX) // JSX needs TSX grammar
    } else {
        tree_sitter::Language::new(tree_sitter_javascript::LANGUAGE)
    };
    let _ = parser.set_language(&lang);
    let Some(tree) = parser.parse(content.as_bytes(), None) else {
        return Vec::new();
    };
    let file_name = Path::new(rel_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut symbols = Vec::new();
    walk_ts(
        tree.root_node(),
        content,
        prefix,
        &file_name,
        "",
        &mut symbols,
    );
    symbols
}

fn walk_ts(
    node: tree_sitter::Node<'_>,
    source: &str,
    prefix: &str,
    file_name: &str,
    scope: &str,
    out: &mut Vec<SymbolEntry>,
) {
    let kind = node.kind();
    match kind {
        "function_declaration" | "generator_function_declaration" => {
            let name = node_name_ts(node, source);
            let qn = qualify(prefix, file_name, scope, &name);
            let body = node_text(node, source).to_owned();
            out.push(SymbolEntry {
                id: SymbolId::new(),
                kind: "function".to_owned(),
                name,
                qualified_name: qn,
                signature: None,
                body: body.to_owned(),
                start_line: Some(node.start_position().row as i64 + 1),
                end_line: Some(node.end_position().row as i64 + 1),
                parent_id: None,
            });
        }
        "class_declaration" | "abstract_class_declaration" => {
            let name = node_name_ts(node, source);
            let child_scope = if scope.is_empty() {
                name.clone()
            } else {
                format!("{scope}::{name}")
            };
            out.push(SymbolEntry {
                id: SymbolId::new(),
                kind: "class".to_owned(),
                name,
                qualified_name: qualify(prefix, file_name, scope, &child_scope),
                signature: None,
                body: node_text(node, source).to_owned(),
                start_line: Some(node.start_position().row as i64 + 1),
                end_line: Some(node.end_position().row as i64 + 1),
                parent_id: None,
            });
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                walk_ts(child, source, prefix, file_name, &child_scope, out);
            }
        }
        "method_definition" | "abstract_method_definition" => {
            let name = node_name_ts(node, source);
            let qn = qualify(prefix, file_name, scope, &name);
            out.push(SymbolEntry {
                id: SymbolId::new(),
                kind: "method".to_owned(),
                name,
                qualified_name: qn,
                signature: None,
                body: node_text(node, source).to_owned(),
                start_line: Some(node.start_position().row as i64 + 1),
                end_line: Some(node.end_position().row as i64 + 1),
                parent_id: None,
            });
        }
        "interface_declaration" => {
            let name = node_name_ts(node, source);
            let child_scope = if scope.is_empty() {
                name.clone()
            } else {
                format!("{scope}::{name}")
            };
            out.push(SymbolEntry {
                id: SymbolId::new(),
                kind: "interface".to_owned(),
                name,
                qualified_name: qualify(prefix, file_name, scope, &child_scope),
                signature: None,
                body: node_text(node, source).to_owned(),
                start_line: Some(node.start_position().row as i64 + 1),
                end_line: Some(node.end_position().row as i64 + 1),
                parent_id: None,
            });
        }
        "type_alias_declaration" | "enum_declaration" => {
            let name = node_name_ts(node, source);
            let kind_str = if kind == "enum_declaration" {
                "enum"
            } else {
                "type"
            };
            out.push(SymbolEntry {
                id: SymbolId::new(),
                kind: kind_str.to_owned(),
                name: name.clone(),
                qualified_name: qualify(prefix, file_name, scope, &name),
                signature: None,
                body: node_text(node, source).to_owned(),
                start_line: Some(node.start_position().row as i64 + 1),
                end_line: Some(node.end_position().row as i64 + 1),
                parent_id: None,
            });
        }
        "lexical_declaration" | "variable_declaration" => {
            // `const fn = () => { ... }` or `const x = function() { ... }`
            extract_ts_arrow_or_fn_expr(node, source, prefix, file_name, scope, out);
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                walk_ts(child, source, prefix, file_name, scope, out);
            }
        }
    }
}

fn extract_ts_arrow_or_fn_expr(
    node: tree_sitter::Node<'_>,
    source: &str,
    prefix: &str,
    file_name: &str,
    scope: &str,
    out: &mut Vec<SymbolEntry>,
) {
    // For `const name = () => {}` or `const name = function() {}`
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            let name_node = child.child_by_field_name("name");
            let value_node = child.child_by_field_name("value");
            if let (Some(name_n), Some(val_n)) = (name_node, value_node) {
                let is_fn = matches!(
                    val_n.kind(),
                    "arrow_function" | "function" | "function_expression"
                );
                if is_fn {
                    let name = node_text(name_n, source);
                    let qn = qualify(prefix, file_name, scope, name);
                    out.push(SymbolEntry {
                        id: SymbolId::new(),
                        kind: "function".to_owned(),
                        name: name.to_owned(),
                        qualified_name: qn,
                        signature: None,
                        body: node_text(node, source).to_owned(),
                        start_line: Some(node.start_position().row as i64 + 1),
                        end_line: Some(node.end_position().row as i64 + 1),
                        parent_id: None,
                    });
                }
            }
        }
    }
}

fn node_name_ts(node: tree_sitter::Node<'_>, source: &str) -> String {
    if let Some(name_child) = (0..node.child_count())
        .filter_map(|i| node.child(i))
        .find(|c| {
            matches!(
                c.kind(),
                "identifier" | "type_identifier" | "nested_identifier" | "property_identifier"
            )
        })
    {
        node_text(name_child, source).to_owned()
    } else {
        "<anonymous>".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_rust_functions() {
        let code = r#"
pub fn hello() -> &'static str {
    "world"
}

fn private_helper(x: i32) -> i32 {
    x + 1
}
"#;
        let symbols = extract_symbols("src/lib.rs", code).unwrap();
        assert!(
            symbols.len() >= 2,
            "should find at least hello + private_helper: {:?}",
            symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"hello"));
        assert!(names.contains(&"private_helper"));
        for s in &symbols {
            assert_eq!(s.kind, "function");
            assert!(s.qualified_name.contains("lib.rs::"));
        }
    }

    #[test]
    fn extract_rust_struct_and_impl() {
        let code = r#"
struct Foo {
    x: i32,
}

impl Foo {
    fn new(x: i32) -> Self {
        Self { x }
    }

    fn get(&self) -> i32 {
        self.x
    }
}
"#;
        let symbols = extract_symbols("src/foo.rs", code).unwrap();
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Foo"), "should find struct Foo: {names:?}");
        assert!(names.contains(&"new"), "should find method new: {names:?}");
        assert!(names.contains(&"get"), "should find method get: {names:?}");
        let methods: Vec<&str> = symbols
            .iter()
            .filter(|s| s.kind == "method")
            .map(|s| s.name.as_str())
            .collect();
        assert!(methods.contains(&"new"));
        assert!(methods.contains(&"get"));
    }

    #[test]
    fn extract_rust_trait() {
        let code = r#"
trait Drawable {
    fn draw(&self);
    fn bounds(&self) -> (i32, i32);
}
"#;
        let symbols = extract_symbols("src/draw.rs", code).unwrap();
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Drawable"));
        assert!(names.contains(&"draw"));
        assert!(names.contains(&"bounds"));
    }

    #[test]
    fn extract_typescript_functions() {
        let code = r#"
export function greet(name: string): string {
    return `Hello ${name}`;
}

const add = (a: number, b: number) => a + b;
"#;
        let symbols = extract_symbols("src/utils.ts", code).unwrap();
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"greet"), "should find greet: {names:?}");
        assert!(names.contains(&"add"), "should find add: {names:?}");
    }

    #[test]
    fn extract_typescript_class() {
        let code = r#"
class Animal {
    name: string;

    constructor(name: string) {
        this.name = name;
    }

    speak() {
        return `${this.name} makes a noise`;
    }
}
"#;
        let symbols = extract_symbols("src/animal.ts", code).unwrap();
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Animal"), "should find class: {names:?}");
        assert!(names.contains(&"speak"), "should find method: {names:?}");
    }

    #[test]
    fn unknown_extension_returns_none() {
        assert!(extract_symbols("src/style.css", "body {}").is_none());
        assert!(extract_symbols("README.md", "# Hello").is_none());
    }
}
