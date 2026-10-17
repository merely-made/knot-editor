// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Editable attributed Djot blocks over the document's one source buffer.
//! These are ordinary text edits: the existing undo and Save paths own them.
use crate::{DocumentFormat, KnotDocumentSession};
use nematic::knot::djot::divs::{Div, divs, wrap};
use std::{collections::BTreeMap, ops::Range};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotDivSnapshot {
    pub address: String,
    pub source_text: String,
    pub divs: Vec<Div>,
}

impl KnotDocumentSession {
    pub fn div_snapshot(&self) -> Result<KnotDivSnapshot, String> {
        let snapshot = self.snapshot();
        if !matches!(snapshot.format, DocumentFormat::Djot | DocumentFormat::Knot) {
            return Err("Attributed blocks require a Djot document".into());
        }
        Ok(KnotDivSnapshot {
            address: snapshot.source.address,
            divs: divs(&snapshot.text),
            source_text: snapshot.text,
        })
    }

    /// Apply declarations to authored source without copying in foreign content.
    /// The selected bytes stay intact; a closing fence may need a final newline.
    pub fn wrap_source_div(
        &mut self,
        snapshot: &KnotDivSnapshot,
        range: Range<usize>,
        attributes: BTreeMap<String, String>,
    ) -> Result<(), String> {
        validate_attributes(&attributes)?;
        let body = snapshot
            .source_text
            .get(range.clone())
            .ok_or("Invalid source range")?;
        let replacement = wrap(&attributes, body);
        self.replace_source_span(
            &snapshot.address,
            &snapshot.source_text,
            range.start,
            range.end,
            replacement,
        )
    }

    /// Edit one current block's declarations, retaining its authored body exactly.
    /// A forged block or stale snapshot cannot redirect this operation.
    pub fn set_div_attributes(
        &mut self,
        snapshot: &KnotDivSnapshot,
        div: &Div,
        attributes: BTreeMap<String, String>,
    ) -> Result<(), String> {
        fn contains(divs: &[Div], target: &Div) -> bool {
            divs.iter()
                .any(|div| div == target || contains(&div.children, target))
        }
        validate_attributes(&attributes)?;
        let actual = divs(&snapshot.source_text);
        if actual != snapshot.divs || !contains(&actual, div) {
            return Err("Block does not match this document source".into());
        }
        // Change the declaration/opening only. Rewrapping the whole source
        // would strip a quote/list parent's markers from the new fence lines.
        let before_body = snapshot.source_text[..div.body.start].trim_end_matches(['\n', '\r']);
        let opening = before_body
            .rsplit('\n')
            .next()
            .ok_or("Missing block opening")?;
        let colon = opening.find(':').ok_or("Missing block fence")?;
        let prefix = &opening[..colon];
        let fence: String = opening[colon..].chars().take_while(|c| *c == ':').collect();
        let replacement = if attributes.is_empty() {
            format!("{fence}\n")
        } else {
            let encoded = wrap(&attributes, "");
            let declaration = encoded.lines().next().ok_or("Missing block declaration")?;
            format!("{declaration}\n{prefix}{fence}\n")
        };
        self.replace_source_span(
            &snapshot.address,
            &snapshot.source_text,
            div.range.start,
            div.body.start,
            replacement,
        )
    }

    /// Insert independent authored content. Provenance may be carried as data in
    /// the supplied attributes; it is not a live source connection.
    pub fn insert_div(
        &mut self,
        snapshot: &KnotDivSnapshot,
        at: usize,
        attributes: BTreeMap<String, String>,
        body: &str,
    ) -> Result<(), String> {
        validate_attributes(&attributes)?;
        self.replace_source_span(
            &snapshot.address,
            &snapshot.source_text,
            at,
            at,
            wrap(&attributes, body),
        )
    }
}

fn validate_attributes(attributes: &BTreeMap<String, String>) -> Result<(), String> {
    if attributes.iter().any(|(key, value)| {
        key.is_empty()
            || !key
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_:-".contains(&c))
            || value.contains(['\n', '\r', '\0'])
    }) {
        return Err("Block attributes need valid names and single-line values".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KnotDocumentIntentV1 as Intent, KnotDocumentWritePostureV1};
    use cambium::TextCommand;
    #[test]
    fn editing_declarations_keeps_quote_and_list_parent_markers() {
        for source in [
            "> {#verse}\n> ::: metre\n> quiet light\n> :::\n",
            "- item\n\n  {#verse}\n  ::: metre\n  quiet light\n  :::\n",
        ] {
            let mut session = KnotDocumentSession::scratch("memory:nested.djot", source);
            let before = session.div_snapshot().unwrap();
            let div = &before.divs[0];
            let original_body = before.source_text[div.body.clone()].to_string();
            let mut attrs = div.attributes.clone();
            attrs.insert("form".into(), "manual".into());
            session
                .set_div_attributes(&before, div, attrs.clone())
                .unwrap();
            let after = session.div_snapshot().unwrap();
            assert_eq!(after.divs.len(), 1, "{}", after.source_text);
            assert_eq!(after.divs[0].attributes, attrs);
            assert_eq!(
                &after.source_text[after.divs[0].body.clone()],
                original_body
            );
            assert!(after.source_text.contains(if source.starts_with('>') {
                "> :::\n"
            } else {
                "  :::\n"
            }));
            session.apply(Intent::Edit(TextCommand::Undo)).unwrap();
            assert_eq!(session.snapshot().text, source);
        }
    }
    #[test]
    fn both_consumers_apply_and_insert_in_one_source_history_and_normal_save() {
        let dir = tempfile::tempdir().unwrap();
        for (domain, body, rules) in [
            ("metre", "α unknownword\nthe light\n", "u /"),
            ("progression", "I IV V I\n", "4/4"),
        ] {
            let path = dir.path().join(format!("{domain}.djot"));
            let original = format!("# Before\n\n{body}\nAfter\n");
            std::fs::write(&path, &original).unwrap();
            let mut session = KnotDocumentSession::open(&path).unwrap();
            let before = session.div_snapshot().unwrap();
            let start = original.find(body).unwrap();
            let attrs = BTreeMap::from([
                ("class".into(), domain.into()),
                ("rules".into(), rules.into()),
            ]);
            session
                .wrap_source_div(&before, start..start + body.len(), attrs.clone())
                .unwrap();
            let applied = session.div_snapshot().unwrap();
            assert_eq!(&applied.source_text[applied.divs[0].body.clone()], body);
            session.apply(Intent::Edit(TextCommand::Undo)).unwrap();
            assert_eq!(session.snapshot().text, original);
            session.apply(Intent::Edit(TextCommand::Redo)).unwrap();
            assert_eq!(session.snapshot().text, applied.source_text);
            session
                .insert_div(
                    &applied,
                    applied.source_text.len(),
                    attrs,
                    "independent copy\n",
                )
                .unwrap();
            let expected = session.div_snapshot().unwrap();
            session.apply(Intent::Save).unwrap();
            drop(session);
            let reopened = KnotDocumentSession::open(&path).unwrap();
            assert_eq!(reopened.div_snapshot().unwrap(), expected);
            assert!(!reopened.snapshot().dirty);
        }
    }
    #[test]
    fn declarations_keep_nested_content_and_refuse_stale_forged_read_only_edits() {
        let source = "{#outer}\n:::: metre\n::: stanza\nhello\n:::\n::::\n";
        let mut session = KnotDocumentSession::scratch("memory:test", source);
        let snapshot = session.div_snapshot().unwrap();
        let div = &snapshot.divs[0];
        let mut attrs = div.attributes.clone();
        attrs.insert("form".into(), "local:custom".into());
        session
            .set_div_attributes(&snapshot, div, attrs.clone())
            .unwrap();
        let next = session.div_snapshot().unwrap();
        assert_eq!(
            &next.source_text[next.divs[0].body.clone()],
            &source[div.body.clone()]
        );
        let before = session.snapshot();
        assert!(
            session
                .set_div_attributes(&snapshot, div, attrs.clone())
                .is_err()
        );
        assert_eq!(session.snapshot(), before);
        let mut forged = next.divs[0].clone();
        forged.range.end -= 1;
        assert!(
            session
                .set_div_attributes(&next, &forged, attrs.clone())
                .is_err()
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("read.djot");
        std::fs::write(&path, source).unwrap();
        let mut read = KnotDocumentSession::open_read_only(path).unwrap();
        let snapshot = read.div_snapshot().unwrap();
        assert_eq!(
            read.snapshot().write_posture,
            KnotDocumentWritePostureV1::ReadOnly
        );
        assert!(
            read.set_div_attributes(&snapshot, &snapshot.divs[0], attrs)
                .is_err()
        );
        assert_eq!(read.snapshot().text, source);
    }
}
