//! A `mimeapps.list` file, edited in place.
//!
//! The file is kept as the lines it was read from. Only the entries that are
//! changed are written afresh, so comments, blank lines, groups this module
//! knows nothing about and the way each untouched line is spelled all survive
//! an edit. Dotfiles are often kept in git; a save that reshuffles the file
//! would bury the one change that was made.

/// The three groups the specification gives the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    /// `[Default Applications]`: the application each type opens with.
    Default,
    /// `[Added Associations]`: applications offered for a type, beyond the
    /// ones their own desktop entries list.
    Added,
    /// `[Removed Associations]`: applications that must not be offered.
    Removed,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Default, Group::Added, Group::Removed];

    pub fn header(self) -> &'static str {
        match self {
            Group::Default => "Default Applications",
            Group::Added => "Added Associations",
            Group::Removed => "Removed Associations",
        }
    }

    fn from_header(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|group| group.header() == name)
    }
}

/// A parsed `mimeapps.list`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MimeApps {
    /// The first section holds what comes before any `[group]` line, and may
    /// be empty.
    sections: Vec<Section>,
    final_newline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Section {
    /// The `[group]` line as it was written; `None` for the lead-in.
    header: Option<String>,
    group: Option<Group>,
    lines: Vec<Line>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    /// A comment, a blank line, or a line of a group this module ignores.
    Raw(String),
    Entry(Entry),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    mime: String,
    ids: Vec<String>,
    /// The line as it was read. Dropped once `ids` changes.
    raw: Option<String>,
}

impl Entry {
    fn render(&self) -> String {
        match &self.raw {
            Some(raw) => raw.clone(),
            None => format!("{}={};", self.mime, self.ids.join(";")),
        }
    }
}

impl MimeApps {
    pub fn parse(text: &str) -> Self {
        let mut sections = vec![Section {
            header: None,
            group: None,
            lines: Vec::new(),
        }];
        for line in text.lines() {
            if let Some(name) = header_name(line) {
                sections.push(Section {
                    header: Some(line.to_string()),
                    group: Group::from_header(name),
                    lines: Vec::new(),
                });
                continue;
            }
            let section = sections.last_mut().expect("the lead-in is always there");
            let entry = section.group.and_then(|_| parse_entry(line));
            section.lines.push(match entry {
                Some(entry) => Line::Entry(entry),
                None => Line::Raw(line.to_string()),
            });
        }
        Self {
            sections,
            final_newline: text.is_empty() || text.ends_with('\n'),
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for section in &self.sections {
            if let Some(header) = &section.header {
                out.push_str(header);
                out.push('\n');
            }
            for line in &section.lines {
                match line {
                    Line::Raw(raw) => out.push_str(raw),
                    Line::Entry(entry) => out.push_str(&entry.render()),
                }
                out.push('\n');
            }
        }
        if !self.final_newline {
            out.pop();
        }
        out
    }

    /// The applications listed for `mime` in `group`, in order. When the file
    /// repeats a key the last one counts, as it does for GLib.
    pub fn get(&self, group: Group, mime: &str) -> Option<&[String]> {
        self.entries_of(group)
            .filter(|entry| entry.mime == mime)
            .last()
            .map(|entry| entry.ids.as_slice())
    }

    /// Every entry of `group` in file order, one per type.
    pub fn entries(&self, group: Group) -> Vec<(&str, &[String])> {
        let mut out: Vec<(&str, &[String])> = Vec::new();
        for entry in self.entries_of(group) {
            match out.iter_mut().find(|(mime, _)| *mime == entry.mime) {
                Some(slot) => slot.1 = &entry.ids,
                None => out.push((&entry.mime, &entry.ids)),
            }
        }
        out
    }

    fn entries_of(&self, group: Group) -> impl Iterator<Item = &Entry> {
        self.sections
            .iter()
            .filter(move |section| section.group == Some(group))
            .flat_map(|section| &section.lines)
            .filter_map(|line| match line {
                Line::Entry(entry) => Some(entry),
                Line::Raw(_) => None,
            })
    }

    /// Replace the list of `mime` in `group` with `ids`; an empty list takes
    /// the entry out. An entry that already says this is left as written.
    pub fn set(&mut self, group: Group, mime: &str, ids: Vec<String>) {
        let mut seen = Vec::new();
        let ids: Vec<String> = ids
            .into_iter()
            .filter(|id| {
                !id.is_empty() && !seen.contains(id) && {
                    seen.push(id.clone());
                    true
                }
            })
            .collect();

        let mut at = self.positions(group, mime);
        let Some(keep) = at.pop() else {
            if !ids.is_empty() {
                self.insert(group, mime, ids);
            }
            return;
        };
        if ids.is_empty() {
            at.push(keep);
        } else if let Line::Entry(entry) = &mut self.sections[keep.0].lines[keep.1]
            && entry.ids != ids
        {
            entry.ids = ids;
            entry.raw = None;
        }
        // Repeats of the key: only the one kept may stay. Back to front, so the
        // indices still to be removed do not move.
        for (section, line) in at.into_iter().rev() {
            self.sections[section].lines.remove(line);
        }
    }

    fn positions(&self, group: Group, mime: &str) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (s, section) in self.sections.iter().enumerate() {
            if section.group != Some(group) {
                continue;
            }
            for (l, line) in section.lines.iter().enumerate() {
                if matches!(line, Line::Entry(entry) if entry.mime == mime) {
                    out.push((s, l));
                }
            }
        }
        out
    }

    fn insert(&mut self, group: Group, mime: &str, ids: Vec<String>) {
        let section = match self.sections.iter().rposition(|s| s.group == Some(group)) {
            Some(ix) => ix,
            None => {
                // A new group goes at the end, after a blank line.
                if let Some(last) = self.sections.last_mut()
                    && last.lines.last().is_some_and(
                        |line| !matches!(line, Line::Raw(raw) if raw.trim().is_empty()),
                    )
                {
                    last.lines.push(Line::Raw(String::new()));
                }
                self.sections.push(Section {
                    header: Some(format!("[{}]", group.header())),
                    group: Some(group),
                    lines: Vec::new(),
                });
                self.final_newline = true;
                self.sections.len() - 1
            }
        };
        let lines = &mut self.sections[section].lines;
        // After the last entry, not after the blank line that closes the group.
        let at = lines
            .iter()
            .rposition(|line| matches!(line, Line::Entry(_)))
            .map_or(0, |ix| ix + 1);
        lines.insert(
            at,
            Line::Entry(Entry {
                mime: mime.to_string(),
                ids,
                raw: None,
            }),
        );
    }

    /// What `xdg-mime default` does: `id` opens `mime`, and is not hidden from
    /// it. `listed` says whether the application's own desktop entry names the
    /// type; when it does not, the app is also added to the type so that it
    /// shows up wherever the type's applications are listed.
    pub fn set_default(&mut self, mime: &str, id: &str, listed: bool) {
        self.set(Group::Default, mime, vec![id.to_string()]);
        self.drop_app(Group::Removed, mime, id);
        if !listed {
            self.add_app(Group::Added, mime, id);
        }
    }

    fn add_app(&mut self, group: Group, mime: &str, id: &str) {
        let mut ids = self.get(group, mime).map(<[_]>::to_vec).unwrap_or_default();
        if !ids.iter().any(|have| have == id) {
            ids.push(id.to_string());
            self.set(group, mime, ids);
        }
    }

    fn drop_app(&mut self, group: Group, mime: &str, id: &str) {
        if let Some(ids) = self.get(group, mime)
            && ids.iter().any(|have| have == id)
        {
            let rest = ids.iter().filter(|have| *have != id).cloned().collect();
            self.set(group, mime, rest);
        }
    }

    /// Take applications `installed` says are gone out of the default and added
    /// lists. The removed list is left alone: a line there keeps working for an
    /// application that comes back. Returns how many were dropped.
    pub fn prune_missing(&mut self, installed: impl Fn(&str) -> bool) -> usize {
        let mut dropped = 0;
        for group in [Group::Default, Group::Added] {
            let stale: Vec<(String, Vec<String>)> = self
                .entries(group)
                .into_iter()
                .filter(|(_, ids)| ids.iter().any(|id| !installed(id)))
                .map(|(mime, ids)| (mime.to_string(), ids.to_vec()))
                .collect();
            for (mime, ids) in stale {
                let kept: Vec<String> = ids.iter().filter(|id| installed(id)).cloned().collect();
                dropped += ids.len() - kept.len();
                self.set(group, &mime, kept);
            }
        }
        dropped
    }
}

/// `Default Applications` for `[Default Applications]`.
fn header_name(line: &str) -> Option<&str> {
    line.trim().strip_prefix('[')?.strip_suffix(']')
}

fn parse_entry(line: &str) -> Option<Entry> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (mime, value) = trimmed.split_once('=')?;
    let mime = mime.trim();
    if mime.is_empty() {
        return None;
    }
    Some(Entry {
        mime: mime.to_string(),
        ids: value
            .split(';')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect(),
        raw: Some(line.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of a real file: both spellings of the list, a comment, a group
    /// this module does not know, no newline at the end.
    const SAMPLE: &str = "\
# my defaults
[Default Applications]
image/png=viewer.desktop
text/html=firefox.desktop;

[Added Associations]
text/plain=zed.desktop;kate.desktop;

[X-Custom]
keep=me

[Removed Associations]
image/png=chrome.desktop;";

    #[test]
    fn untouched_file_renders_byte_for_byte() {
        for text in [
            SAMPLE,
            "",
            "\n",
            "[Default Applications]\n",
            "a=b\n[x]\n\n\n",
        ] {
            assert_eq!(MimeApps::parse(text).render(), text, "{text:?}");
        }
    }

    #[test]
    fn reads_each_group() {
        let doc = MimeApps::parse(SAMPLE);
        assert_eq!(
            doc.get(Group::Default, "image/png").unwrap(),
            ["viewer.desktop"]
        );
        assert_eq!(
            doc.get(Group::Added, "text/plain").unwrap(),
            ["zed.desktop", "kate.desktop"]
        );
        assert_eq!(
            doc.get(Group::Removed, "image/png").unwrap(),
            ["chrome.desktop"]
        );
        assert!(doc.get(Group::Default, "text/plain").is_none());
        // `keep=me` is in a group that is not ours.
        assert!(
            doc.entries(Group::Default)
                .iter()
                .all(|(m, _)| *m != "keep")
        );
    }

    #[test]
    fn editing_one_entry_changes_only_its_line() {
        let mut doc = MimeApps::parse(SAMPLE);
        doc.set(Group::Default, "image/png", vec!["gimp.desktop".into()]);
        assert_eq!(
            doc.render(),
            SAMPLE.replace("image/png=viewer.desktop", "image/png=gimp.desktop;")
        );
    }

    #[test]
    fn setting_what_is_already_there_keeps_the_original_spelling() {
        let mut doc = MimeApps::parse(SAMPLE);
        doc.set(Group::Default, "image/png", vec!["viewer.desktop".into()]);
        assert_eq!(doc.render(), SAMPLE);
    }

    #[test]
    fn new_entry_goes_after_the_last_entry_not_after_the_blank_line() {
        let mut doc = MimeApps::parse(SAMPLE);
        doc.set(Group::Default, "audio/flac", vec!["player.desktop".into()]);
        assert!(
            doc.render()
                .contains("text/html=firefox.desktop;\naudio/flac=player.desktop;\n\n[Added")
        );
    }

    #[test]
    fn missing_group_is_appended_after_a_blank_line() {
        let mut doc = MimeApps::parse("[Default Applications]\na/b=x.desktop");
        doc.set(Group::Removed, "a/b", vec!["y.desktop".into()]);
        assert_eq!(
            doc.render(),
            "[Default Applications]\na/b=x.desktop\n\n[Removed Associations]\na/b=y.desktop;\n"
        );
        let mut empty = MimeApps::default();
        empty.set(Group::Default, "a/b", vec!["x.desktop".into()]);
        assert_eq!(empty.render(), "[Default Applications]\na/b=x.desktop;\n");
    }

    #[test]
    fn empty_list_removes_the_entry() {
        let mut doc = MimeApps::parse(SAMPLE);
        doc.set(Group::Default, "image/png", vec![]);
        assert_eq!(
            doc.render(),
            SAMPLE.replace("image/png=viewer.desktop\n", "")
        );
    }

    #[test]
    fn repeated_keys_collapse_to_one() {
        let mut doc = MimeApps::parse("[Default Applications]\na/b=one.desktop\na/b=two.desktop\n");
        assert_eq!(doc.get(Group::Default, "a/b").unwrap(), ["two.desktop"]);
        doc.set(Group::Default, "a/b", vec!["three.desktop".into()]);
        assert_eq!(doc.render(), "[Default Applications]\na/b=three.desktop;\n");
    }

    #[test]
    fn set_default_unhides_and_adds_unlisted_apps() {
        let mut doc = MimeApps::parse(SAMPLE);
        doc.set_default("image/png", "chrome.desktop", false);
        assert_eq!(
            doc.get(Group::Default, "image/png").unwrap(),
            ["chrome.desktop"]
        );
        assert!(doc.get(Group::Removed, "image/png").is_none());
        assert_eq!(
            doc.get(Group::Added, "image/png").unwrap(),
            ["chrome.desktop"]
        );

        // An app that lists the type itself needs no added association.
        doc.set_default("image/jpeg", "viewer.desktop", true);
        assert!(doc.get(Group::Added, "image/jpeg").is_none());
    }

    #[test]
    fn prune_drops_missing_apps_but_not_removed_associations() {
        let mut doc = MimeApps::parse(SAMPLE);
        let dropped = doc.prune_missing(|id| id != "viewer.desktop" && id != "kate.desktop");
        assert_eq!(dropped, 2);
        assert!(doc.get(Group::Default, "image/png").is_none());
        assert_eq!(
            doc.get(Group::Added, "text/plain").unwrap(),
            ["zed.desktop"]
        );
        assert_eq!(
            doc.get(Group::Removed, "image/png").unwrap(),
            ["chrome.desktop"]
        );
    }

    #[test]
    fn entries_lists_one_row_per_type_in_file_order() {
        let doc = MimeApps::parse(SAMPLE);
        let names: Vec<_> = doc
            .entries(Group::Default)
            .iter()
            .map(|(m, _)| *m)
            .collect();
        assert_eq!(names, ["image/png", "text/html"]);
    }
}
