//! The capture plan schema page of the docs site, rendered from the same JSON
//! Schema value `schema_files_are_current` writes (`schemars::schema_for!`).
//! `SSG_UPDATE_SCHEMAS=1` rewrites the page; otherwise the committed page must
//! match.

use serde_json::{Map, Value};

use crate::capture::plan::CapturePlan;
use crate::test_support::repo_root;

const PAGE: &str = "docs_site/docs/plan-reference/capture-plan-schema.md";
const SCHEMA_URL: &str =
    "https://conjunction-crew.github.io/ssg-tether-capture/schemas/capture-plan.v1.schema.json";

/// Escapes text for a table cell or a list line of a Docusaurus (MDX) page:
/// one line, `|` escaped, and outside code spans the characters MDX would
/// parse (`{`, `}`, `<`, `>`) written as entities.
fn cell(text: &str) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::new();
    let mut in_code = false;
    for c in one_line.chars() {
        match c {
            '`' => {
                in_code = !in_code;
                out.push(c);
            }
            '|' => out.push_str("\\|"),
            '{' if !in_code => out.push_str("&#123;"),
            '}' if !in_code => out.push_str("&#125;"),
            '<' if !in_code => out.push_str("&lt;"),
            '>' if !in_code => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

fn description(node: &Value) -> String {
    node.get("description")
        .and_then(Value::as_str)
        .map(cell)
        .unwrap_or_default()
}

/// The name a `$ref` points at, linked to its section.
fn ref_link(reference: &str) -> String {
    let name = reference.rsplit('/').next().unwrap_or(reference);
    format!("[`{name}`](#{})", name.to_lowercase())
}

/// A short type text: `string`, `number or null`, `array of Phase`, …
fn type_text(node: &Value) -> String {
    if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
        return ref_link(reference);
    }
    if let Some(any_of) = node.get("anyOf").and_then(Value::as_array) {
        return any_of
            .iter()
            .map(type_text)
            .collect::<Vec<_>>()
            .join(" or ");
    }
    let base = match node.get("type") {
        Some(Value::String(t)) => vec![t.clone()],
        Some(Value::Array(ts)) => ts
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => vec!["any JSON value".to_string()],
    };
    let mut text = base
        .iter()
        .map(|t| match (t.as_str(), node.get("items")) {
            ("array", Some(items)) => format!("array of {}", type_text(items)),
            _ => t.clone(),
        })
        .collect::<Vec<_>>()
        .join(" or ");
    match (node.get("minimum"), node.get("maximum")) {
        (Some(min), Some(max)) if min == max => text.push_str(&format!(", exactly {min}")),
        (min, max) => {
            if let Some(min) = min {
                text.push_str(&format!(", at least {min}"));
            }
            if let Some(max) = max {
                text.push_str(&format!(", at most {max}"));
            }
        }
    }
    text
}

/// One section: the object's keys as a table, or an enum's allowed values.
fn section(out: &mut String, heading: &str, node: &Value) {
    out.push_str(&format!("## {heading}\n\n"));
    let lead = description(node);
    if !lead.is_empty() {
        out.push_str(&format!("{lead}\n\n"));
    }
    if let Some(properties) = node.get("properties").and_then(Value::as_object) {
        let required: Vec<&str> = node
            .get("required")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        out.push_str("| Key | Type | Required | Description |\n|---|---|---|---|\n");
        let mut keys: Vec<&String> = properties.keys().collect();
        keys.sort();
        for key in keys {
            let property = &properties[key];
            let required = if required.contains(&key.as_str()) {
                "yes"
            } else {
                "no"
            };
            out.push_str(&format!(
                "| `{key}` | {} | {required} | {} |\n",
                type_text(property),
                description(property)
            ));
        }
        if node.get("additionalProperties") == Some(&Value::Bool(false)) {
            out.push_str("\nNo other key is allowed.\n");
        }
        out.push('\n');
    }
    if let Some(one_of) = node.get("oneOf").and_then(Value::as_array) {
        out.push_str("One of these strings:\n\n");
        for value in one_of {
            let name = value
                .get("const")
                .and_then(Value::as_str)
                .unwrap_or_default();
            out.push_str(&format!("- `{name}`: {}\n", description(value)));
        }
        out.push('\n');
    }
    if let Some(values) = node.get("enum").and_then(Value::as_array) {
        let names: Vec<String> = values
            .iter()
            .filter_map(Value::as_str)
            .map(|v| format!("`{v}`"))
            .collect();
        out.push_str(&format!("One of these strings: {}.\n\n", names.join(", ")));
    }
}

/// The page, from the schema value.
fn render(schema: &Value) -> String {
    let mut out = String::new();
    out.push_str(
        "---\ntitle: Capture plan schema (v1)\nsidebar_position: 1\n---\n\n\
         {/* Generated from the JSON Schema by the ssg-sim test schema_docs_are_current. \
         Do not edit; regenerate with: SSG_UPDATE_SCHEMAS=1 cargo test -p ssg-sim schema_ */}\n\n\
         # Capture plan schema (v1)\n\n",
    );
    out.push_str(&format!(
        "A capture plan is a JSON file that describes the capture device, the capture sphere and \
         the phases of a capture. This page lists every key of a plan with `schema_version: 1`. \
         It is generated from the plan's JSON Schema, [`capture-plan.v1.schema.json`]({SCHEMA_URL}), \
         which editors can use to check a plan as it is written; both are produced from the \
         simulator's own types, so they match what the app and `ssg-sim` accept.\n\n\
         A key marked \"no\" under Required may be left out; its description gives the default.\n\n"
    ));
    section(&mut out, "Plan file (top level)", schema);
    let empty = Map::new();
    let defs = schema
        .get("$defs")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let mut names: Vec<&String> = defs.keys().collect();
    names.sort();
    for name in names {
        section(&mut out, name, &defs[name]);
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

#[test]
fn schema_docs_are_current() {
    let schema = serde_json::to_value(schemars::schema_for!(CapturePlan)).unwrap();
    let generated = render(&schema);
    let path = repo_root().join(PAGE);
    if std::env::var_os("SSG_UPDATE_SCHEMAS").is_some() {
        std::fs::write(&path, &generated).unwrap();
    }
    // A Windows checkout may turn the file's line feeds into CR LF.
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    if committed != generated {
        let diff: Vec<String> = committed
            .lines()
            .zip(generated.lines())
            .enumerate()
            .filter(|(_, (c, g))| c != g)
            .take(10)
            .map(|(n, (c, g))| format!("line {}:\n- {c}\n+ {g}", n + 1))
            .collect();
        panic!(
            "{PAGE} is out of date ({} committed lines, {} generated); regenerate with \
             `SSG_UPDATE_SCHEMAS=1 cargo test -p ssg-sim schema_`\n{}",
            committed.lines().count(),
            generated.lines().count(),
            diff.join("\n")
        );
    }
}

#[test]
fn schema_docs_escape_mdx_and_table_characters() {
    assert_eq!(
        cell("a {b} <c> | `x | {y}`\n  next"),
        "a &#123;b&#125; &lt;c&gt; \\| `x \\| {y}` next"
    );
}
