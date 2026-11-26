//! Line-based editing of INI-like files (`.desktop` entries, GTK's
//! `settings.ini`), so that a change touches one line and everything else in
//! the file stays as its owner wrote it.

/// `text` with `key` in `group` set to `value`, or the key removed for
/// `None`. The key is added at the end of the group, or the group at the end
/// of the file, when it is not there.
pub fn set_key(text: &str, group: &str, key: &str, value: Option<&str>) -> String {
    let header = format!("[{group}]");
    let mut out: Vec<String> = Vec::new();
    let mut in_group = false;
    let mut group_seen = false;
    let mut done = false;
    // Where in `out` the group's last key line is, to insert after it.
    let mut insert_at: Option<usize> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_group = trimmed == header;
            group_seen |= in_group;
            if in_group {
                insert_at = Some(out.len() + 1);
            }
        } else if in_group
            && !trimmed.starts_with('#')
            && !trimmed.starts_with(';')
            && let Some((k, _)) = trimmed.split_once('=')
        {
            if k.trim() == key {
                if let (Some(v), false) = (value, done) {
                    out.push(format!("{key}={v}"));
                    done = true;
                }
                // A second copy of the key, or a removal: the line is dropped.
                insert_at = Some(out.len());
                continue;
            }
            out.push(line.to_string());
            insert_at = Some(out.len());
            continue;
        }
        out.push(line.to_string());
    }

    if let (false, Some(v)) = (done, value) {
        if group_seen {
            out.insert(insert_at.unwrap_or(out.len()), format!("{key}={v}"));
        } else {
            if out.last().is_some_and(|l| !l.trim().is_empty()) {
                out.push(String::new());
            }
            out.push(header);
            out.push(format!("{key}={v}"));
        }
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

/// The value of `key` in `group`.
pub fn get_key(text: &str, group: &str, key: &str) -> Option<String> {
    let header = format!("[{group}]");
    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_group = line == header;
        } else if in_group
            && !line.starts_with('#')
            && let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            return Some(v.trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const GTK: &str =
        "[Settings]\ngtk-theme-name=Foo\ngtk-font-name=Bar 10\n# note\ngtk-cursor-theme-size=24\n";

    #[test]
    fn a_value_is_replaced_in_place() {
        let out = set_key(GTK, "Settings", "gtk-font-name", Some("Baz 12"));
        assert!(out.contains("gtk-font-name=Baz 12"));
        assert!(!out.contains("Bar 10"));
        assert!(out.contains("# note"));
        assert_eq!(out.matches("gtk-font-name").count(), 1);
    }

    #[test]
    fn a_new_key_goes_to_the_end_of_its_group() {
        let text = "[Settings]\na=1\n\n[Other]\nb=2\n";
        let out = set_key(text, "Settings", "c", Some("3"));
        assert!(out.find("c=3").unwrap() < out.find("[Other]").unwrap());
        assert!(out.contains("b=2"));
    }

    #[test]
    fn a_missing_group_is_appended() {
        let out = set_key("[A]\nx=1\n", "Settings", "k", Some("v"));
        assert!(out.ends_with("[Settings]\nk=v\n"));
        let empty = set_key("", "Settings", "k", Some("v"));
        assert_eq!(empty, "[Settings]\nk=v\n");
    }

    #[test]
    fn removal_leaves_the_rest() {
        let out = set_key(GTK, "Settings", "gtk-theme-name", None);
        assert!(!out.contains("gtk-theme-name"));
        assert!(out.contains("gtk-font-name=Bar 10"));
        // Removing what is not there changes nothing.
        assert_eq!(set_key(&out, "Settings", "nope", None).trim(), out.trim());
    }

    #[test]
    fn values_are_read_from_their_group_only() {
        let text = "[A]\nk=1\n[B]\nk=2\n";
        assert_eq!(get_key(text, "B", "k").as_deref(), Some("2"));
        assert_eq!(get_key(text, "C", "k"), None);
    }
}
