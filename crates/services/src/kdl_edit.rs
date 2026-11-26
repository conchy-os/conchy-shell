//! Edit one node of a KDL file in place, keeping every comment and every line
//! of the user's formatting around it.
//!
//! The compositor's `wm.kdl` is the user's own file, so nothing here
//! reformats it: a node is found, changed or added, and the rest of the text
//! is left as it was. The compositor's configs are KDL v1, which is what the
//! `kdl` crate in use reads and writes.

use anyhow::{Result, anyhow};
use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};

/// Parse `text`, an empty document when it is blank.
pub fn parse(text: &str) -> Result<KdlDocument> {
    if text.trim().is_empty() {
        return Ok(KdlDocument::new());
    }
    text.parse::<KdlDocument>()
        .map_err(|err| anyhow!("invalid KDL: {err}"))
}

fn find(block: &KdlDocument, name: &str) -> Option<usize> {
    block.nodes().iter().position(|n| n.name().value() == name)
}

/// The node at `path`, then its parents' blocks.
pub fn node<'a>(doc: &'a KdlDocument, path: &[&str]) -> Option<&'a KdlNode> {
    let (last, parents) = path.split_last()?;
    let mut block = doc;
    for key in parents {
        block = block.nodes().get(find(block, key)?)?.children()?;
    }
    block.nodes().get(find(block, last)?)
}

/// Whether a node (a flag like `tap`) is there.
pub fn has(doc: &KdlDocument, path: &[&str]) -> bool {
    node(doc, path).is_some()
}

/// The first argument of the node at `path`.
pub fn arg<'a>(doc: &'a KdlDocument, path: &[&str]) -> Option<&'a KdlValue> {
    node(doc, path)?
        .entries()
        .iter()
        .find(|e| e.name().is_none())
        .map(KdlEntry::value)
}

pub fn arg_str(doc: &KdlDocument, path: &[&str]) -> Option<String> {
    arg(doc, path)?.as_string().map(str::to_string)
}

pub fn arg_f64(doc: &KdlDocument, path: &[&str]) -> Option<f64> {
    let value = arg(doc, path)?;
    value.as_f64().or_else(|| value.as_i64().map(|i| i as f64))
}

pub fn arg_bool(doc: &KdlDocument, path: &[&str]) -> Option<bool> {
    arg(doc, path)?.as_bool()
}

/// Make the node at `path` carry exactly `args`, creating it and the blocks
/// on the way. What surrounds an existing node (comments, trailing remark)
/// stays.
pub fn set_args(doc: &mut KdlDocument, path: &[&str], args: Vec<KdlValue>) -> Result<()> {
    let block_depth = path.len().saturating_sub(1);
    let (last, parents) = path.split_last().ok_or_else(|| anyhow!("empty key path"))?;
    let block = ensure_block(doc, parents)?;
    match find(block, last) {
        Some(position) => {
            let node = &mut block.nodes_mut()[position];
            // Keep named entries (`position x=1 y=2`), replace the arguments.
            node.entries_mut().retain(|e| e.name().is_some());
            let mut entries: Vec<KdlEntry> = args.into_iter().map(KdlEntry::new).collect();
            entries.append(node.entries_mut());
            *node.entries_mut() = entries;
        }
        None => {
            let mut fresh = KdlNode::new(*last);
            for value in args {
                fresh.push(KdlEntry::new(value));
            }
            append(block, block_depth, fresh);
        }
    }
    Ok(())
}

/// Make the node at `path` carry exactly the named entries `props`
/// (`position x=1 y=2`), creating it and the blocks on the way. Its
/// arguments stay.
pub fn set_props(doc: &mut KdlDocument, path: &[&str], props: Vec<(&str, KdlValue)>) -> Result<()> {
    let block_depth = path.len().saturating_sub(1);
    let (last, parents) = path.split_last().ok_or_else(|| anyhow!("empty key path"))?;
    let block = ensure_block(doc, parents)?;
    let build = |node: &mut KdlNode| {
        node.entries_mut().retain(|e| e.name().is_none());
        for (name, value) in props {
            node.push(KdlEntry::new_prop(name, value));
        }
    };
    match find(block, last) {
        Some(position) => build(&mut block.nodes_mut()[position]),
        None => {
            let mut fresh = KdlNode::new(*last);
            build(&mut fresh);
            append(block, block_depth, fresh);
        }
    }
    Ok(())
}

/// Make the flag node at `path` present or absent.
pub fn set_flag(doc: &mut KdlDocument, path: &[&str], on: bool) -> Result<()> {
    if on {
        if !has(doc, path) {
            set_args(doc, path, Vec::new())?;
        }
        Ok(())
    } else {
        remove(doc, path);
        Ok(())
    }
}

/// Drop the node at `path`; whether anything was there. Blocks left empty
/// stay (they may hold comments the user wrote).
pub fn remove(doc: &mut KdlDocument, path: &[&str]) -> bool {
    let Some((last, parents)) = path.split_last() else {
        return false;
    };
    let mut block = doc;
    for key in parents {
        let Some(position) = find(block, key) else {
            return false;
        };
        let Some(children) = block.nodes_mut()[position].children_mut().as_mut() else {
            return false;
        };
        block = children;
    }
    match find(block, last) {
        Some(position) => {
            block.nodes_mut().remove(position);
            true
        }
        None => false,
    }
}

/// The nodes named `name` in the top-level block whose first argument is
/// `first_arg` (`output "eDP-1"`); a mutable reference to the found one, or
/// a new one appended.
pub fn keyed_node<'a>(doc: &'a mut KdlDocument, name: &str, first_arg: &str) -> &'a mut KdlNode {
    let position = doc.nodes().iter().position(|n| {
        n.name().value() == name
            && n.entries()
                .iter()
                .find(|e| e.name().is_none())
                .and_then(|e| e.value().as_string())
                == Some(first_arg)
    });
    let position = match position {
        Some(position) => position,
        None => {
            let mut fresh = KdlNode::new(name);
            fresh.push(KdlEntry::new(first_arg));
            fresh.ensure_children();
            append(doc, 0, fresh);
            doc.nodes().len() - 1
        }
    };
    &mut doc.nodes_mut()[position]
}

/// Remove the keyed node, if present.
pub fn remove_keyed(doc: &mut KdlDocument, name: &str, first_arg: &str) -> bool {
    let position = doc.nodes().iter().position(|n| {
        n.name().value() == name
            && n.entries()
                .iter()
                .find(|e| e.name().is_none())
                .and_then(|e| e.value().as_string())
                == Some(first_arg)
    });
    match position {
        Some(position) => {
            doc.nodes_mut().remove(position);
            true
        }
        None => false,
    }
}

/// Like [`set_args`] / [`set_flag`] / [`remove`], inside a keyed node's block.
pub fn block_of(node: &mut KdlNode) -> &mut KdlDocument {
    node.ensure_children()
}

fn ensure_block<'a>(doc: &'a mut KdlDocument, parents: &[&str]) -> Result<&'a mut KdlDocument> {
    let mut block = doc;
    for (depth, key) in parents.iter().enumerate() {
        let position = match find(block, key) {
            Some(position) => position,
            None => {
                let mut table = KdlNode::new(*key);
                table.ensure_children();
                append(block, depth, table);
                block.nodes().len() - 1
            }
        };
        block = block.nodes_mut()[position].ensure_children();
    }
    Ok(block)
}

/// Add `node` as the last of `block`, which sits `depth` levels down.
pub fn append(block: &mut KdlDocument, depth: usize, mut node: KdlNode) {
    if depth == 0 {
        // A blank line between top-level sections.
        if !block.nodes().is_empty() {
            node.set_leading("\n");
        }
        if block.nodes().last().is_some_and(|last| {
            !last
                .trailing()
                .is_some_and(|trailing| trailing.contains('\n'))
        }) && let Some(last) = block.nodes_mut().last_mut()
        {
            last.set_trailing("\n");
        }
    } else {
        // `section {}` and `section { a 1; }` have no line to start a new
        // node on, and no line for the closing brace either.
        let indent = " ".repeat(4 * (depth - 1));
        node.set_leading(" ".repeat(4 * depth));
        if block.nodes().is_empty() {
            block.set_leading("\n");
            block.set_trailing(indent);
        } else {
            if let Some(last) = block.nodes_mut().last_mut()
                && !last.trailing().is_some_and(|s| s.contains('\n'))
            {
                last.set_trailing("\n");
            }
            if !block
                .trailing()
                .is_some_and(|s| s.contains('\n') || s.is_empty())
            {
                block.set_trailing(indent);
            }
        }
    }
    node.set_trailing("\n");
    block.nodes_mut().push(node);
}

pub fn string(text: &str) -> KdlValue {
    KdlValue::String(text.to_string())
}

pub fn float(value: f64) -> KdlValue {
    KdlValue::Base10Float(value)
}

pub fn int(value: i64) -> KdlValue {
    KdlValue::Base10(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "// my config\ninput {\n    // typing\n    keyboard {\n        repeat-delay 600\n    }\n}\n\noutput \"eDP-1\" {\n    scale 1.0\n}\n";

    #[test]
    fn changing_a_value_keeps_the_comments() {
        let mut doc = parse(SAMPLE).unwrap();
        set_args(
            &mut doc,
            &["input", "keyboard", "repeat-delay"],
            vec![int(300)],
        )
        .unwrap();
        let out = doc.to_string();
        assert!(out.contains("// my config"));
        assert!(out.contains("// typing"));
        assert!(out.contains("repeat-delay 300"));
        assert!(!out.contains("600"));
    }

    #[test]
    fn a_new_node_lands_inside_its_block_and_parses_back() {
        let mut doc = parse(SAMPLE).unwrap();
        set_args(
            &mut doc,
            &["input", "keyboard", "repeat-rate"],
            vec![int(40)],
        )
        .unwrap();
        set_flag(&mut doc, &["input", "touchpad", "tap"], true).unwrap();
        let reparsed = parse(&doc.to_string()).unwrap();
        assert_eq!(
            arg_f64(&reparsed, &["input", "keyboard", "repeat-rate"]),
            Some(40.0)
        );
        assert!(has(&reparsed, &["input", "touchpad", "tap"]));
        assert!(!has(&reparsed, &["input", "mouse", "tap"]));
    }

    #[test]
    fn a_flag_goes_away_when_turned_off() {
        let mut doc =
            parse("input {\n    touchpad {\n        tap\n        dwt\n    }\n}\n").unwrap();
        set_flag(&mut doc, &["input", "touchpad", "tap"], false).unwrap();
        let out = doc.to_string();
        assert!(!out.contains("tap"));
        assert!(out.contains("dwt"));
    }

    #[test]
    fn an_empty_file_gets_a_whole_path() {
        let mut doc = parse("").unwrap();
        set_args(
            &mut doc,
            &["input", "mouse", "accel-speed"],
            vec![float(0.25)],
        )
        .unwrap();
        let reparsed = parse(&doc.to_string()).unwrap();
        assert_eq!(
            arg_f64(&reparsed, &["input", "mouse", "accel-speed"]),
            Some(0.25)
        );
    }

    #[test]
    fn a_keyed_node_is_found_or_created() {
        let mut doc = parse(SAMPLE).unwrap();
        {
            let out = keyed_node(&mut doc, "output", "eDP-1");
            set_args(block_of(out), &["scale"], vec![float(2.0)]).unwrap();
        }
        {
            let out = keyed_node(&mut doc, "output", "HDMI-A-1");
            set_args(block_of(out), &["scale"], vec![float(1.5)]).unwrap();
        }
        let reparsed = parse(&doc.to_string()).unwrap();
        assert_eq!(
            doc.nodes()
                .iter()
                .filter(|n| n.name().value() == "output")
                .count(),
            2
        );
        assert!(doc.to_string().contains("scale 2.0") || doc.to_string().contains("scale 2"));
        assert_eq!(reparsed.nodes().len(), 3);
    }

    #[test]
    fn named_entries_are_set_and_replaced() {
        let mut doc = parse("output \"A\" {\n    scale 1.0\n}\n").unwrap();
        {
            let out = keyed_node(&mut doc, "output", "A");
            set_props(
                block_of(out),
                &["position"],
                vec![("x", int(10)), ("y", int(20))],
            )
            .unwrap();
            set_props(
                block_of(out),
                &["position"],
                vec![("x", int(30)), ("y", int(40))],
            )
            .unwrap();
        }
        let text = doc.to_string();
        assert_eq!(text.matches("position").count(), 1, "{text}");
        assert!(text.contains("x=30") && text.contains("y=40"), "{text}");
        assert!(parse(&text).is_ok());
    }

    #[test]
    fn named_entries_survive_an_argument_change() {
        let mut doc = parse("pos 1 x=5\n").unwrap();
        set_args(&mut doc, &["pos"], vec![int(2)]).unwrap();
        let out = doc.to_string();
        assert!(out.contains("pos 2"), "{out}");
        assert!(out.contains("x=5"), "{out}");
    }
}
