//! The keyboard layouts, variants and options the system knows, from the XKB
//! rules list, and how a chosen set turns into the strings compositors take.

use std::fs;

const RULES: &[&str] = &[
    "/usr/share/X11/xkb/rules/evdev.lst",
    "/usr/share/X11/xkb/rules/base.lst",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XkbLayout {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XkbVariant {
    pub code: String,
    pub layout: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XkbOption {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XkbCatalog {
    pub layouts: Vec<XkbLayout>,
    pub variants: Vec<XkbVariant>,
    pub options: Vec<XkbOption>,
}

impl XkbCatalog {
    /// The system's list; empty when the rules file cannot be read.
    pub fn load() -> Self {
        RULES
            .iter()
            .find_map(|path| fs::read_to_string(path).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        #[derive(PartialEq)]
        enum Section {
            Other,
            Layout,
            Variant,
            Option,
        }
        let mut catalog = Self::default();
        let mut section = Section::Other;
        for line in text.lines() {
            if let Some(title) = line.strip_prefix('!') {
                section = match title.trim() {
                    "layout" => Section::Layout,
                    "variant" => Section::Variant,
                    "option" => Section::Option,
                    _ => Section::Other,
                };
                continue;
            }
            let line = line.trim();
            if line.is_empty() || section == Section::Other {
                continue;
            }
            let Some((code, rest)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            let rest = rest.trim();
            match section {
                Section::Layout => catalog.layouts.push(XkbLayout {
                    code: code.to_string(),
                    name: rest.to_string(),
                }),
                Section::Variant => {
                    // `intl  us: English (US, intl., with dead keys)`
                    if let Some((layout, name)) = rest.split_once(':') {
                        catalog.variants.push(XkbVariant {
                            code: code.to_string(),
                            layout: layout.trim().to_string(),
                            name: name.trim().to_string(),
                        });
                    }
                }
                Section::Option => catalog.options.push(XkbOption {
                    code: code.to_string(),
                    name: rest.to_string(),
                }),
                Section::Other => {}
            }
        }
        catalog
    }

    pub fn layout_name(&self, code: &str) -> Option<&str> {
        self.layouts
            .iter()
            .find(|l| l.code == code)
            .map(|l| l.name.as_str())
    }

    pub fn variants_of<'a>(&'a self, layout: &'a str) -> impl Iterator<Item = &'a XkbVariant> {
        self.variants.iter().filter(move |v| v.layout == layout)
    }

    /// The `grp:` options: the keys that switch between layouts.
    pub fn switch_options(&self) -> Vec<&XkbOption> {
        self.options
            .iter()
            .filter(|o| o.code.starts_with("grp:"))
            .collect()
    }
}

/// One layout of the user's set, with its variant (`""` for the default).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LayoutEntry {
    pub layout: String,
    pub variant: String,
}

impl LayoutEntry {
    pub fn new(layout: impl Into<String>, variant: impl Into<String>) -> Self {
        Self {
            layout: layout.into(),
            variant: variant.into(),
        }
    }
}

/// The layouts and options a compositor is told, as the two or three
/// comma-separated strings XKB takes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct XkbSet {
    pub layouts: Vec<LayoutEntry>,
    pub options: Vec<String>,
}

impl XkbSet {
    /// From the compositor's own strings: `layout "us,ru"`, `variant ",phonetic"`,
    /// `options "grp:alt_shift_toggle,caps:escape"`.
    pub fn from_strings(layout: &str, variant: &str, options: &str) -> Self {
        let variants: Vec<&str> = variant.split(',').collect();
        let layouts = layout
            .split(',')
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .enumerate()
            .map(|(index, layout)| {
                LayoutEntry::new(layout, variants.get(index).copied().unwrap_or("").trim())
            })
            .collect();
        let options = options
            .split(',')
            .map(str::trim)
            .filter(|o| !o.is_empty())
            .map(str::to_string)
            .collect();
        Self { layouts, options }
    }

    pub fn layout_string(&self) -> String {
        self.layouts
            .iter()
            .map(|l| l.layout.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Empty when no layout has a variant, else one slot per layout.
    pub fn variant_string(&self) -> String {
        if self.layouts.iter().all(|l| l.variant.is_empty()) {
            return String::new();
        }
        self.layouts
            .iter()
            .map(|l| l.variant.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn options_string(&self) -> String {
        self.options.join(",")
    }

    /// The option that switches layouts, if one is set.
    pub fn switch_option(&self) -> Option<&str> {
        self.options
            .iter()
            .find(|o| o.starts_with("grp:"))
            .map(String::as_str)
    }

    /// Replace the layout-switching option; `None` removes it.
    pub fn set_switch_option(&mut self, option: Option<&str>) {
        self.options.retain(|o| !o.starts_with("grp:"));
        if let Some(option) = option {
            self.options.insert(0, option.to_string());
        }
    }

    /// The same layout may not be listed twice with the same variant.
    pub fn contains(&self, layout: &str, variant: &str) -> bool {
        self.layouts
            .iter()
            .any(|l| l.layout == layout && l.variant == variant)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LST: &str = "! model\n  pc104  Generic 104-key PC\n\n! layout\n  us              English (US)\n  ru              Russian\n\n! variant\n  intl            us: English (US, intl., with dead keys)\n  phonetic        ru: Russian (phonetic)\n\n! option\n  grp                  Switching to another layout\n  grp:alt_shift_toggle Alt+Shift\n  caps:escape          Make Caps Lock an additional Esc\n";

    #[test]
    fn the_rules_list_is_read() {
        let c = XkbCatalog::parse(LST);
        assert_eq!(c.layouts.len(), 2);
        assert_eq!(c.layout_name("ru"), Some("Russian"));
        assert_eq!(c.variants_of("ru").next().unwrap().code, "phonetic");
        assert_eq!(c.switch_options().len(), 1);
        assert_eq!(c.options.len(), 3);
    }

    #[test]
    fn strings_round_trip() {
        let set = XkbSet::from_strings("us, ru", ",phonetic", "grp:win_space_toggle");
        assert_eq!(
            set.layouts,
            vec![
                LayoutEntry::new("us", ""),
                LayoutEntry::new("ru", "phonetic")
            ]
        );
        assert_eq!(set.layout_string(), "us,ru");
        assert_eq!(set.variant_string(), ",phonetic");
        assert_eq!(set.switch_option(), Some("grp:win_space_toggle"));
    }

    #[test]
    fn no_variants_means_an_empty_string() {
        let set = XkbSet::from_strings("us,ru", "", "");
        assert_eq!(set.variant_string(), "");
        assert_eq!(set.options_string(), "");
    }

    #[test]
    fn the_switch_option_is_replaced_not_added() {
        let mut set = XkbSet::from_strings("us,ru", "", "caps:escape,grp:alt_shift_toggle");
        set.set_switch_option(Some("grp:win_space_toggle"));
        assert_eq!(set.options, vec!["grp:win_space_toggle", "caps:escape"]);
        set.set_switch_option(None);
        assert_eq!(set.options, vec!["caps:escape"]);
    }
}
