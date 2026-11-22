//! Names a person can read for MIME types: `PNG image` for `image/png`.
//!
//! The words come from the shared-mime-info database, which has a file per
//! type (`<data dir>/mime/image/png.xml`) with the name in many languages. The
//! untagged `<comment>` is the English one; the shell's own text is English,
//! so that is the one used.

use std::fs;

use crate::applications::get_application_dirs;

const SCHEME_PREFIX: &str = "x-scheme-handler/";

/// Schemes that are written `name:` and not `name://`.
const OPAQUE_SCHEMES: &[&str] = &[
    "mailto", "tel", "sms", "xmpp", "magnet", "geo", "sip", "sips", "news", "urn", "data",
];

/// What `mime` is called, if anything is known about it.
pub fn describe(mime: &str) -> Option<String> {
    if let Some(scheme) = mime.strip_prefix(SCHEME_PREFIX) {
        return scheme_name(scheme);
    }
    // A type comes from a file the user can edit; keep it inside the database.
    if mime.is_empty() || mime.contains("..") || mime.starts_with('/') {
        return None;
    }
    get_application_dirs().into_iter().find_map(|apps| {
        let path = apps.parent()?.join("mime").join(format!("{mime}.xml"));
        english_comment(&fs::read_to_string(path).ok()?)
    })
}

fn scheme_name(scheme: &str) -> Option<String> {
    if scheme.is_empty() {
        return None;
    }
    let sep = if OPAQUE_SCHEMES.contains(&scheme) {
        ":"
    } else {
        "://"
    };
    Some(format!("{scheme}{sep} links"))
}

/// The text of the first `<comment>` that has no `xml:lang`.
fn english_comment(xml: &str) -> Option<String> {
    let start = xml.find("<comment>")? + "<comment>".len();
    let len = xml[start..].find("</comment>")?;
    let text = unescape(xml[start..start + len].trim());
    (!text.is_empty()).then_some(text)
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_untagged_comment_not_a_translation() {
        let xml = r#"<mime-type type="image/png">
  <comment>PNG image</comment>
  <comment xml:lang="de">PNG-Bild</comment>
</mime-type>"#;
        assert_eq!(english_comment(xml).as_deref(), Some("PNG image"));
        // Only translations: nothing to say.
        let only_de = r#"<comment xml:lang="de">PNG-Bild</comment>"#;
        assert_eq!(english_comment(only_de), None);
    }

    #[test]
    fn unescapes_entities() {
        let xml = "<comment>AT&amp;T &lt;file&gt;</comment>";
        assert_eq!(english_comment(xml).as_deref(), Some("AT&T <file>"));
    }

    #[test]
    fn url_schemes_are_named_without_a_database() {
        assert_eq!(
            describe("x-scheme-handler/https").as_deref(),
            Some("https:// links")
        );
        assert_eq!(
            describe("x-scheme-handler/mailto").as_deref(),
            Some("mailto: links")
        );
        assert_eq!(describe("x-scheme-handler/"), None);
    }

    #[test]
    fn a_type_cannot_escape_the_database() {
        assert_eq!(describe("../../etc/passwd"), None);
        assert_eq!(describe("/etc/passwd"), None);
        assert_eq!(describe(""), None);
    }
}
