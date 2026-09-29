// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Pure helpers for source-authored links between explicitly named local files.
//! These links are source text, not signed relation assertions.

use std::path::{Component, Path, PathBuf};

use knot_document::{DocumentFormat, KnotDocumentSession};
use knot_readings::{ReadingInput, ReadingLinkV1};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

// Encode a component, never the slash separating components. Keeping only URL
// unreserved punctuation prevents a filename from becoming Djot syntax.
const PATH_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Relationship {
    pub slug: &'static str,
    pub label: &'static str,
    pub iri: &'static str,
}

macro_rules! relationship {
    ($slug:literal, $label:literal) => {
        Relationship {
            slug: $slug,
            label: $label,
            iri: concat!("https://mere.computer/ns/rel#", $slug),
        }
    };
}

/// The existing Knot core vocabulary, also used by Mere's relation kernel.
pub(crate) const RELATIONSHIPS: &[Relationship] = &[
    relationship!("cites", "cites"),
    relationship!("quotes", "quotes"),
    relationship!("supports", "supports"),
    relationship!("contradicts", "contradicts"),
    relationship!("questions", "questions"),
    relationship!("elaborates", "elaborates"),
    relationship!("example-of", "example of"),
    relationship!("summarizes", "summarizes"),
];

pub(crate) const MAX_SOURCE_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_LINKS: usize = 4096;

pub(crate) fn supported(format: DocumentFormat) -> bool {
    matches!(format, DocumentFormat::Djot | DocumentFormat::Knot)
}

/// Read links through the same parsed preview used by readings and navigation.
pub(crate) fn parsed_links(session: &KnotDocumentSession) -> Result<Vec<ReadingLinkV1>, String> {
    if session.snapshot().text.len() > MAX_SOURCE_BYTES {
        return Err("Document links are unavailable for sources larger than 1 MiB".into());
    }
    let links = ReadingInput::from_session(session)?.links;
    if links.len() > MAX_LINKS {
        return Err("Document links are unavailable for sources with more than 4096 links".into());
    }
    Ok(links)
}

/// Serialize a relative URL from one saved file to another. This does not touch
/// the filesystem or resolve symlinks; callers supply the actual saved paths.
pub(crate) fn relative_url(source: &Path, target: &Path) -> Result<String, String> {
    let source = normalized_path(source)?;
    let target = normalized_path(target)?;
    let parent = source
        .parent()
        .ok_or_else(|| "The source must name a saved file".to_owned())?;
    if target.file_name().is_none() {
        return Err("The target must name a saved file".into());
    }
    let from: Vec<_> = parent.components().collect();
    let to: Vec<_> = target.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    if common == 0 || from[0] != to[0] {
        return Err("The documents must share a filesystem root".into());
    }
    let mut parts = vec!["..".to_owned(); from.len() - common];
    for component in &to[common..] {
        let Component::Normal(value) = component else {
            return Err("The documents must share a filesystem root".into());
        };
        let value = value
            .to_str()
            .ok_or_else(|| "The target path is not UTF-8".to_owned())?;
        parts.push(utf8_percent_encode(value, PATH_COMPONENT).to_string());
    }
    // An explicit relative prefix also makes names such as `https:notes.djot`
    // unambiguously local in URL consumers.
    let url = parts.join("/");
    Ok(if url.starts_with("../") {
        url
    } else {
        format!("./{url}")
    })
}

/// Wrap a single line of literal Unicode text. An empty selection uses the
/// target's display label; every ASCII punctuation character is Djot-escaped.
pub(crate) fn markup(
    source: &Path,
    target: &Path,
    selected: &str,
    target_label: &str,
    relationship: Option<&str>,
) -> Result<String, String> {
    let label = if selected.is_empty() {
        target_label
    } else {
        selected
    };
    if label.is_empty() || label.chars().any(char::is_control) {
        return Err("A document link needs a nonempty single-line label".into());
    }
    let mut escaped = String::with_capacity(label.len());
    for character in label.chars() {
        if character.is_ascii_punctuation() {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    let url = relative_url(source, target)?;
    let mut result = format!("[{escaped}]({url})");
    if let Some(value) = relationship {
        let relation = RELATIONSHIPS
            .iter()
            .find(|relation| relation.slug == value || relation.iri == value)
            .ok_or_else(|| "Choose a supported document relationship".to_owned())?;
        result.push_str(&format!("{{rel=\"{}\"}}", relation.iri));
    }
    Ok(result)
}

/// Resolve only a local relative URL. Fragments and queries are not paths;
/// encoded punctuation is decoded after splitting them off. The returned path
/// is a candidate for matching against the host's explicitly admitted files.
/// It never opens a file, follows a network URL, or expands an authority.
pub(crate) fn resolve_local(source: &Path, target: &str) -> Option<PathBuf> {
    let source = normalized_path(source).ok()?;
    let raw = target.split(['#', '?']).next()?;
    if raw.is_empty()
        || raw.starts_with('/')
        || raw.contains('\\')
        || raw.chars().any(char::is_control)
        || raw.split('/').next()?.contains(':')
    {
        return None;
    }
    let mut result = source.parent()?.to_path_buf();
    for component in raw.split('/') {
        let bytes = component.as_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'%'
                && (index + 2 >= bytes.len()
                    || !bytes[index + 1].is_ascii_hexdigit()
                    || !bytes[index + 2].is_ascii_hexdigit())
            {
                return None;
            }
        }
        let decoded = percent_encoding::percent_decode_str(component)
            .decode_utf8()
            .ok()?;
        if decoded.contains(['/', '\\']) || decoded.chars().any(char::is_control) {
            return None;
        }
        match decoded.as_ref() {
            "" | "." => {},
            ".." => {
                if !result.pop() {
                    return None;
                }
            },
            value => result.push(value),
        }
    }
    normalized_path(&result).ok()
}

fn normalized_path(path: &Path) -> Result<PathBuf, String> {
    let text = path
        .to_str()
        .ok_or_else(|| "Document paths must be UTF-8".to_owned())?;
    if !path.is_absolute() || text.chars().any(char::is_control) {
        return Err(
            "Document links require saved absolute paths without control characters".into(),
        );
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {},
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err("A document path escapes its filesystem root".into());
                }
            },
            Component::Normal(value) => {
                // On Unix this can be a filename; on Windows it is a path
                // separator. Reject it to keep authored URLs portable.
                if value.to_str().is_none_or(|value| value.contains('\\')) {
                    return Err("A document path is not portable as a local link".into());
                }
                normalized.push(value);
            },
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> PathBuf {
        std::env::temp_dir().join("knot-link-tests").join(name)
    }

    #[test]
    fn relative_urls_encode_unicode_and_syntax_per_component() {
        let source = file("notes/a.djot");
        let target = file("sources/東京 space(#?%).djot");
        let url = relative_url(&source, &target).unwrap();
        assert_eq!(
            url,
            "../sources/%E6%9D%B1%E4%BA%AC%20space%28%23%3F%25%29.djot"
        );
        assert_eq!(resolve_local(&source, &url), Some(target));
        let target = file("notes/https:notes.djot");
        let url = relative_url(&source, &target).unwrap();
        assert_eq!(url, "./https%3Anotes.djot");
        assert_eq!(resolve_local(&source, &url), Some(target));
    }

    #[test]
    fn authored_labels_and_relationships_round_trip_through_both_parsers() {
        let labels = [
            "plain text",
            "Unicode α 東京",
            r"[brackets] \backslash",
            "*strong* `code` ![image](x) {attributes}",
            "a_b ~c~ $d$ <e> &f; ^g^ +h+ =i= \"j\" 'k'",
        ];
        for format in [DocumentFormat::Djot, DocumentFormat::Knot] {
            assert!(supported(format));
            for label in labels {
                for relationship in std::iter::once(None)
                    .chain(RELATIONSHIPS.iter().map(|relation| Some(relation.slug)))
                {
                    let source = markup(
                        &file("source.djot"),
                        &file("target (東京).djot"),
                        label,
                        "unused",
                        relationship,
                    )
                    .unwrap();
                    let session = KnotDocumentSession::read_only_with_format(
                        "memory:link",
                        source.clone(),
                        format,
                    );
                    let links = parsed_links(&session).unwrap();
                    assert_eq!(links.len(), 1, "{format:?}: {source}");
                    assert_eq!(links[0].text, label, "{format:?}: {source}");
                    assert_eq!(links[0].span, Some((0, source.len())), "{source}");
                    assert_eq!(links[0].target, "./target%20%28%E6%9D%B1%E4%BA%AC%29.djot");
                    assert_eq!(
                        links[0].rel.as_deref(),
                        relationship.map(|slug| RELATIONSHIPS
                            .iter()
                            .find(|relation| relation.slug == slug)
                            .unwrap()
                            .iri)
                    );
                }
            }
        }
    }

    #[test]
    fn empty_selection_uses_label_and_invalid_labels_or_relationships_refuse() {
        assert_eq!(
            markup(&file("a.djot"), &file("b.djot"), "", "B", None).unwrap(),
            "[B](./b.djot)"
        );
        for label in ["", "line\nbreak", "tab\t", "nul\0"] {
            assert!(markup(&file("a.djot"), &file("b.djot"), label, label, None).is_err());
        }
        assert!(
            markup(
                &file("a.djot"),
                &file("b.djot"),
                "label",
                "",
                Some("cites\"} evil")
            )
            .is_err()
        );
        assert!(!supported(DocumentFormat::Scroll));
        assert!(!supported(DocumentFormat::Markdown));
        assert!(!supported(DocumentFormat::Gemtext));
    }

    #[test]
    fn local_resolution_rejects_network_roots_controls_and_malformed_encoding() {
        let source = file("notes/a.djot");
        assert_eq!(
            resolve_local(&source, "../b%20c.djot#section?ignored"),
            Some(file("b c.djot"))
        );
        for target in [
            "https://example.test/a",
            "mailto:x@y",
            "file:///a",
            "mere://node/a",
            "//server/a",
            "/a",
            "C:/a",
            r"..\a",
            "#section",
            "a%ZZ",
            "a%",
            "%FF",
            "a%00",
            "a%0A",
            "a%2Fb",
            "a%5Cb",
        ] {
            assert_eq!(resolve_local(&source, target), None, "{target}");
        }
        assert!(relative_url(Path::new("unsaved.djot"), &file("a.djot")).is_err());
        assert!(relative_url(&file("a.djot"), &file("bad\nname.djot")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_paths_refuse() {
        use std::os::unix::ffi::OsStringExt;
        let bad = file("root").join(std::ffi::OsString::from_vec(vec![0xff]));
        assert!(relative_url(&file("a.djot"), &bad).is_err());
    }

    #[test]
    fn parsed_link_work_is_bounded() {
        let too_large =
            KnotDocumentSession::read_only("memory:large", "x".repeat(MAX_SOURCE_BYTES + 1));
        assert!(parsed_links(&too_large).unwrap_err().contains("1 MiB"));
        let too_many =
            KnotDocumentSession::read_only("memory:many", "[b](b.djot) ".repeat(MAX_LINKS + 1));
        assert!(parsed_links(&too_many).unwrap_err().contains("4096"));
    }

    #[cfg(windows)]
    #[test]
    fn different_windows_roots_refuse() {
        assert!(
            relative_url(Path::new(r"C:\notes\a.djot"), Path::new(r"D:\notes\b.djot")).is_err()
        );
    }
}
