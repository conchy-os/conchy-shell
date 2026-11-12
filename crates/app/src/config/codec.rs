//! KDL as the on-disk format of every file the shell reads or writes.
//!
//! The settings are a tree of tables, lists and scalars that serde already
//! knows how to fill. This module maps that tree to KDL and back, so the rest
//! of the shell keeps its serde structs and `conchy-config` keeps its KDL
//! files, comments and all.
//!
//! # Encoding
//!
//! ```kdl
//! bar {                          // a table: a node with a block
//!     position "top"             // a scalar: one argument
//!     size 32.0                  // a float keeps its dot, an integer does not
//!     start "LauncherBtn" "Clock" // a list of scalars: two or more arguments
//!     (list)end "Clock"          // a list of one (or none: `(list)end`)
//!     modules {
//!         clock format_horizontal="%H:%M"   // properties are a one-line table
//!     }
//! }
//! providers {                    // a list of tables, or of anything but plain
//!     - shebang="g" name="Google"  // scalars: one `-` node per item
//!     - shebang="w" name="Wiki"
//! }
//! ```
//!
//! A node with nothing on it is an empty table, which is what a section left
//! at its defaults is. `null` is the same as leaving the key out.
//!
//! Edits ([`set`], [`remove`]) touch the one node they name; every comment and
//! every line of the user's formatting around it survives.

use anyhow::{Result, anyhow, bail};
use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use toml::{Table, Value};

/// A list item. KDL's own convention for sequences.
const ITEM: &str = "-";
/// The type annotation that says a node is a list whatever its argument count.
const LIST: &str = "list";

// -- KDL to tree --------------------------------------------------------------

/// The settings tree a document describes.
pub fn decode(doc: &KdlDocument) -> Result<Table> {
    decode_block(doc, "")
}

fn decode_block(doc: &KdlDocument, at: &str) -> Result<Table> {
    let mut table = Table::new();
    for node in doc.nodes() {
        let name = node.name().value();
        let here = join(at, name);
        if table.contains_key(name) {
            bail!("`{here}` is set twice");
        }
        if let Some(value) = decode_node(node, &here)? {
            table.insert(name.to_string(), value);
        }
    }
    Ok(table)
}

fn decode_node(node: &KdlNode, at: &str) -> Result<Option<Value>> {
    let forced_list = match node.ty() {
        None => false,
        Some(ty) if ty.value() == LIST => true,
        Some(ty) => bail!("`{at}` has an unknown type annotation `({})`", ty.value()),
    };

    let mut args = Vec::new();
    let mut props = Table::new();
    for entry in node.entries() {
        match entry.name() {
            Some(key) => {
                if let Some(value) = scalar(entry.value()) {
                    props.insert(key.value().to_string(), value);
                }
            }
            None => args.push(entry.value()),
        }
    }

    if let Some(block) = node.children() {
        if !args.is_empty() {
            bail!("`{at}` has both a value and a block");
        }
        let nodes = block.nodes();
        let items = !nodes.is_empty() && nodes.iter().all(|n| n.name().value() == ITEM);
        if items || forced_list {
            if !props.is_empty() {
                bail!("`{at}` is a list, it takes no properties");
            }
            let mut list = Vec::with_capacity(nodes.len());
            for (index, item) in nodes.iter().enumerate() {
                if item.name().value() != ITEM {
                    bail!(
                        "`{at}` is a list, its items are `-` nodes, not `{}`",
                        item.name().value()
                    );
                }
                if let Some(value) = decode_node(item, &format!("{at}[{index}]"))? {
                    list.push(value);
                }
            }
            return Ok(Some(Value::Array(list)));
        }

        if nodes.iter().any(|n| n.name().value() == ITEM) {
            bail!("`{at}` mixes list items (`-`) with keys");
        }
        let mut table = decode_block(block, at)?;
        for (key, value) in props {
            if table.insert(key.clone(), value).is_some() {
                bail!("`{}` is set twice", join(at, &key));
            }
        }
        return Ok(Some(Value::Table(table)));
    }

    if !props.is_empty() {
        if !args.is_empty() || forced_list {
            bail!(
                "`{at}` mixes properties with {}",
                if forced_list { "a list" } else { "values" }
            );
        }
        return Ok(Some(Value::Table(props)));
    }

    Ok(match (args.len(), forced_list) {
        (0, true) => Some(Value::Array(Vec::new())),
        (0, false) => Some(Value::Table(Table::new())),
        (1, false) => scalar(args[0]),
        _ => Some(Value::Array(args.into_iter().filter_map(scalar).collect())),
    })
}

fn scalar(value: &KdlValue) -> Option<Value> {
    match value {
        KdlValue::String(s) | KdlValue::RawString(s) => Some(Value::String(s.clone())),
        KdlValue::Base2(i) | KdlValue::Base8(i) | KdlValue::Base10(i) | KdlValue::Base16(i) => {
            Some(Value::Integer(*i))
        }
        KdlValue::Base10Float(f) => Some(Value::Float(*f)),
        KdlValue::Bool(b) => Some(Value::Boolean(*b)),
        KdlValue::Null => None,
    }
}

fn join(at: &str, name: &str) -> String {
    if at.is_empty() {
        name.to_string()
    } else {
        format!("{at}.{name}")
    }
}

// -- tree to KDL --------------------------------------------------------------

/// A whole document for `table`.
pub fn encode(table: &Table) -> KdlDocument {
    block(table)
}

fn block(table: &Table) -> KdlDocument {
    let mut doc = KdlDocument::new();
    for (key, value) in table {
        doc.nodes_mut().push(encode_node(key, value));
    }
    doc
}

/// The node for one key: how `value` is spelled is up to its shape.
pub fn encode_node(name: &str, value: &Value) -> KdlNode {
    let mut node = KdlNode::new(name);
    match value {
        Value::Table(table) => {
            let mut children = block(table);
            if table.is_empty() {
                // `name {}` on one line, not an empty block over three.
                children.set_leading("");
                children.set_trailing("");
            }
            node.set_children(children);
        }
        Value::Array(items) => match items.iter().map(kdl_scalar).collect::<Option<Vec<_>>>() {
            Some(values) => {
                if values.len() < 2 {
                    node.set_ty(LIST);
                }
                for value in values {
                    node.push(KdlEntry::new(value));
                }
            }
            None => {
                let mut children = KdlDocument::new();
                for item in items {
                    children.nodes_mut().push(encode_node(ITEM, item));
                }
                node.set_children(children);
            }
        },
        other => {
            if let Some(value) = kdl_scalar(other) {
                node.push(KdlEntry::new(value));
            }
        }
    }
    node
}

fn kdl_scalar(value: &Value) -> Option<KdlValue> {
    Some(match value {
        Value::String(s) => KdlValue::String(s.clone()),
        Value::Integer(i) => KdlValue::Base10(*i),
        Value::Float(f) => KdlValue::Base10Float(tidy(*f)),
        Value::Boolean(b) => KdlValue::Bool(*b),
        Value::Datetime(d) => KdlValue::String(d.to_string()),
        Value::Array(_) | Value::Table(_) => return None,
    })
}

/// An `f32` field arrives as the `f64` nearest to it, `0.949999988079071` for
/// a `0.95`. If the number is exactly an `f32`, write the `f32` spelling.
fn tidy(value: f64) -> f64 {
    let narrow = value as f32;
    if f64::from(narrow) == value {
        narrow.to_string().parse().unwrap_or(value)
    } else {
        value
    }
}

// -- editing a document in place ---------------------------------------------

/// Set the node at `path` to `value`, creating the tables on the way.
pub fn set(doc: &mut KdlDocument, path: &[&str], value: &Value) -> Result<()> {
    let (last, parents) = path.split_last().ok_or_else(|| anyhow!("empty key path"))?;

    let mut block = doc;
    let mut depth = 0;
    for (index, key) in parents.iter().enumerate() {
        let at = path[..=index].join(".");
        let position = match find(block, key) {
            Some(position) => position,
            None => {
                let mut table = KdlNode::new(*key);
                table.ensure_children();
                append(block, depth, table);
                block.nodes().len() - 1
            }
        };
        let node = &mut block.nodes_mut()[position];
        if !node.entries().is_empty() || node.ty().is_some() {
            bail!("`{at}` is not a table");
        }
        let children = node.ensure_children();
        if children.nodes().iter().any(|n| n.name().value() == ITEM) {
            bail!("`{at}` is not a table");
        }
        block = children;
        depth += 1;
    }

    let fresh = encode_node(last, value);
    match find(block, last) {
        Some(position) => {
            // Keep what surrounds the node (its comments, its trailing
            // remark); replace what it says.
            let node = &mut block.nodes_mut()[position];
            *node.entries_mut() = fresh.entries().to_vec();
            *node.ty_mut() = fresh.ty().cloned();
            *node.children_mut() = fresh.children().cloned();
        }
        None => append(block, depth, fresh),
    }
    Ok(())
}

/// Drop the node at `path`, and any table it leaves empty. Whether anything
/// was there.
pub fn remove(doc: &mut KdlDocument, path: &[&str]) -> bool {
    let Some((last, parents)) = path.split_last() else {
        return false;
    };
    remove_in(doc, parents, last)
}

fn remove_in(block: &mut KdlDocument, parents: &[&str], last: &str) -> bool {
    let Some((first, rest)) = parents.split_first() else {
        return match find(block, last) {
            Some(position) => {
                block.nodes_mut().remove(position);
                true
            }
            None => false,
        };
    };
    let Some(position) = find(block, first) else {
        return false;
    };
    let node = &mut block.nodes_mut()[position];
    let removed = match node.children_mut() {
        Some(children) => remove_in(children, rest, last),
        None => false,
    };
    if removed && is_vacant(node) {
        block.nodes_mut().remove(position);
    }
    removed
}

/// A table with nothing in it and nothing said about it.
fn is_vacant(node: &KdlNode) -> bool {
    node.entries().is_empty()
        && node.ty().is_none()
        && node.children().is_some_and(|children| {
            children.nodes().is_empty()
                && !children
                    .leading()
                    .is_some_and(|s| s.contains("//") || s.contains("/*"))
                && !children
                    .trailing()
                    .is_some_and(|s| s.contains("//") || s.contains("/*"))
        })
}

fn find(block: &KdlDocument, name: &str) -> Option<usize> {
    block.nodes().iter().position(|n| n.name().value() == name)
}

/// Add `node` as the last of `block`, which sits `depth` levels down.
fn append(block: &mut KdlDocument, depth: usize, mut node: KdlNode) {
    if depth == 0 {
        // A blank line between top-level sections.
        if !block.nodes().is_empty() {
            node.set_leading("\n");
        }
    } else {
        // `section {}` and `section { a 1; }` have no line to start a new
        // node on, and no line for the closing brace either.
        let indent = " ".repeat(4 * (depth - 1));
        if block.nodes().is_empty() {
            block.set_leading("\n");
            block.set_trailing(indent);
        } else {
            if let Some(last) = block.nodes_mut().last_mut() {
                if !last.trailing().is_some_and(|s| s.contains('\n')) {
                    last.set_trailing("\n");
                }
            }
            if !block
                .trailing()
                .is_some_and(|s| s.contains('\n') || s.is_empty())
            {
                block.set_trailing(indent);
            }
        }
    }
    block.nodes_mut().push(node);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> KdlDocument {
        text.parse().expect("test KDL parses")
    }

    fn tree(text: &str) -> Value {
        Value::Table(decode(&parse(text)).expect("decodes"))
    }

    fn toml_tree(text: &str) -> Value {
        Value::Table(toml::from_str::<Table>(text).expect("test TOML parses"))
    }

    fn round_trip(value: &Value) -> Value {
        let Value::Table(table) = value else {
            panic!("a document is a table")
        };
        let text = encode(table).to_string();
        let doc: KdlDocument = text.parse().unwrap_or_else(|err| panic!("{err}\n{text}"));
        Value::Table(decode(&doc).expect("decodes"))
    }

    #[test]
    fn scalars_keep_their_types() {
        let value = tree("size 32.0\ncount 3\nname \"x\"\non true\n");
        assert_eq!(
            value,
            toml_tree("size = 32.0\ncount = 3\nname = \"x\"\non = true\n")
        );
    }

    #[test]
    fn a_node_with_a_block_is_a_table_and_a_bare_node_is_an_empty_one() {
        let value = tree("bar {\n    position \"top\"\n    tray\n    sub {}\n}\n");
        assert_eq!(
            value,
            toml_tree("[bar]\nposition = \"top\"\n[bar.tray]\n[bar.sub]\n")
        );
    }

    #[test]
    fn lists_by_argument_count_annotation_or_dash_items() {
        let value = tree(concat!(
            "many \"a\" \"b\"\n",
            "(list)one \"a\"\n",
            "(list)none\n",
            "dashed {\n    - \"a\"\n    - \"b\"\n}\n",
        ));
        assert_eq!(
            value,
            toml_tree("many = [\"a\", \"b\"]\none = [\"a\"]\nnone = []\ndashed = [\"a\", \"b\"]\n")
        );
    }

    #[test]
    fn a_list_of_tables_and_a_nested_list() {
        let value = tree(concat!(
            "providers {\n",
            "    - shebang=\"g\" name=\"Google\"\n",
            "    - {\n        shebang \"w\"\n        name \"Wiki\"\n    }\n",
            "}\n",
            "start {\n    - \"Launcher\"\n    - \"Systray\" \"KeyboardLayout\"\n}\n",
        ));
        assert_eq!(
            value,
            toml_tree(concat!(
                "providers = [{ shebang = \"g\", name = \"Google\" }, { shebang = \"w\", name = \"Wiki\" }]\n",
                "start = [\"Launcher\", [\"Systray\", \"KeyboardLayout\"]]\n",
            ))
        );
    }

    #[test]
    fn properties_are_a_one_line_table_and_null_is_absence() {
        let value = tree("clock format=\"%H\" gone=null\nunset null\n");
        assert_eq!(value, toml_tree("[clock]\nformat = \"%H\"\n"));
    }

    #[test]
    fn mistakes_name_the_key() {
        for (text, needle) in [
            ("a 1\na 2\n", "`a` is set twice"),
            ("a 1 {\n b 2\n}\n", "`a` has both a value and a block"),
            ("(tuple)a 1\n", "`a` has an unknown type annotation"),
            ("a {\n    - 1\n    b 2\n}\n", "`a` mixes list items"),
            (
                "a {\n    b {\n        c 1\n        c 2\n    }\n}\n",
                "`a.b.c` is set twice",
            ),
        ] {
            let err = decode(&parse(text)).expect_err(text).to_string();
            assert!(err.contains(needle), "{text:?} gave {err:?}");
        }
    }

    #[test]
    fn every_shape_survives_a_round_trip() {
        let value = toml_tree(concat!(
            "watch = true\n",
            "ratio = 0.95\n",
            "width = 600.0\n",
            "count = 16\n",
            "text = \"two\\nlines\"\n",
            "empty = []\n",
            "single = [\"a\"]\n",
            "pair = [\"a\", \"b\"]\n",
            "groups = [\"a\", [\"b\", \"c\"], [\"d\"]]\n",
            "records = [{ id = 1, tags = [\"x\"] }, { id = 2 }]\n",
            "[section]\n",
            "[section.deep]\n",
            "key = \"v\"\n",
        ));
        assert_eq!(round_trip(&value), value);
    }

    #[test]
    fn an_f32_is_written_the_way_it_was_typed() {
        let wide = f64::from(0.95_f32);
        let value = toml_tree(&format!("opacity = {wide}\nexact = 0.1\n"));
        let Value::Table(table) = &value else {
            unreachable!()
        };
        let text = encode(table).to_string();

        assert!(text.contains("opacity 0.95\n"), "{text}");
        assert!(text.contains("exact 0.1\n"), "{text}");
    }

    const FIXTURE: &str = "\
// My shell config

bar {
    position \"top\" // keep me
    size 32.0
}

launcher {
    width 700.0
}
";

    #[test]
    fn setting_a_value_in_place_keeps_everything_around_it() {
        let mut doc = parse(FIXTURE);
        set(
            &mut doc,
            &["bar", "position"],
            &Value::String("bottom".into()),
        )
        .unwrap();

        let out = doc.to_string();
        assert!(out.contains("position \"bottom\" // keep me\n"), "{out}");
        assert!(out.starts_with("// My shell config\n"), "{out}");
        assert_eq!(out.lines().count(), FIXTURE.lines().count(), "{out}");
    }

    #[test]
    fn a_deep_new_key_creates_its_tables_and_reads_back() {
        let mut doc = parse(FIXTURE);
        set(
            &mut doc,
            &["bar", "modules", "clock", "format_horizontal"],
            &Value::String("%H:%M".into()),
        )
        .unwrap();

        let out = doc.to_string();
        assert!(
            out.contains(
                "    modules {\n        clock {\n            format_horizontal \"%H:%M\"\n"
            ),
            "{out}"
        );
        assert!(out.contains("// keep me"), "{out}");
        let reread = tree(&out);
        assert_eq!(
            reread["bar"]["modules"]["clock"]["format_horizontal"].as_str(),
            Some("%H:%M")
        );
        assert_eq!(reread["launcher"]["width"].as_float(), Some(700.0));
    }

    #[test]
    fn a_new_top_level_section_gets_a_blank_line_before_it() {
        let mut doc = parse(FIXTURE);
        set(&mut doc, &["osd", "enabled"], &Value::Boolean(true)).unwrap();

        let out = doc.to_string();
        assert!(out.ends_with("}\n\nosd {\n    enabled true\n}\n"), "{out}");
    }

    #[test]
    fn appending_to_a_one_line_block_starts_a_new_line() {
        let mut doc = parse("bar { size 32.0; }\n");
        set(&mut doc, &["bar", "position"], &Value::String("top".into())).unwrap();

        let out = doc.to_string();
        let reread = tree(&out);
        assert_eq!(reread["bar"]["size"].as_float(), Some(32.0), "{out}");
        assert_eq!(reread["bar"]["position"].as_str(), Some("top"), "{out}");
    }

    #[test]
    fn a_list_value_replaces_the_old_spelling() {
        let mut doc = parse("bar {\n    start {\n        - \"A\"\n        - \"B\"\n    }\n}\n");
        let list = Value::Array(vec![
            Value::String("A".into()),
            Value::String("B".into()),
            Value::String("C".into()),
        ]);
        set(&mut doc, &["bar", "start"], &list).unwrap();

        let out = doc.to_string();
        assert!(out.contains("start \"A\" \"B\" \"C\"\n"), "{out}");
        assert_eq!(tree(&out)["bar"]["start"], list);
    }

    #[test]
    fn a_scalar_on_the_path_is_refused() {
        let mut doc = parse(FIXTURE);
        let err = set(&mut doc, &["bar", "position", "x"], &Value::Integer(1)).expect_err("scalar");
        assert!(
            err.to_string().contains("`bar.position` is not a table"),
            "{err}"
        );
    }

    #[test]
    fn removing_drops_the_node_and_the_tables_it_empties() {
        let mut doc = parse(FIXTURE);
        set(
            &mut doc,
            &["bar", "modules", "clock", "format_horizontal"],
            &Value::String("x".into()),
        )
        .unwrap();

        assert!(remove(
            &mut doc,
            &["bar", "modules", "clock", "format_horizontal"]
        ));
        let out = doc.to_string();
        assert!(!out.contains("modules"), "{out}");
        assert!(out.contains("position \"top\" // keep me"), "{out}");

        assert!(!remove(&mut doc, &["bar", "nope"]));
        assert_eq!(doc.to_string(), out);
    }

    #[test]
    fn a_table_with_a_comment_in_it_is_not_pruned() {
        // A comment above a key goes with the key; one left at the end of the
        // block was about the block, and keeps it alive.
        let mut doc = parse("bar {\n    position \"top\"\n    // more to come\n}\n");
        assert!(remove(&mut doc, &["bar", "position"]));
        assert!(doc.to_string().contains("// more to come"), "{}", doc);
    }
}
