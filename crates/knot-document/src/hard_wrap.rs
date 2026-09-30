// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Conservative detection of paragraph hard wrapping.

const TAB_STOP: usize = 8;
const MIN_LINE_COLUMN: usize = 32;
const MAX_SOURCE_BYTES: usize = 1024 * 1024;

/// Returns the inferred hard-wrap column of plain prose in `source`.
///
/// This is a read-only heuristic. It never changes text. It considers only
/// paragraphs with at least three physical lines and returns a column only
/// when more than half of all admitted plain-prose lines are long,
/// similarly-sized continuations. Single-line paragraphs and each paragraph's
/// final line count in that majority. The returned value is the longest
/// qualifying continuation column; an isolated long line does not set it.
///
/// Blank lines end paragraphs. Headings, lists, quotes, fenced or indented
/// code, tables, thematic breaks, and common block-level markup are excluded
/// from consideration. Pipe-prefixed lines are excluded conservatively as
/// table rows.
///
/// ASCII and Latin-1 characters each count as one monospace column (this
/// includes precomposed characters such as `é`). A tab advances to the next
/// multiple of eight columns. Since this dependency-free detector cannot
/// reliably measure combining sequences or wide Unicode glyphs, it returns
/// `None` if the source contains characters beyond Latin-1. Sources larger
/// than 1 MiB also return `None` to bound its work. The result is a text
/// column estimate, not a measurement of proportional font advances.
pub fn hard_wrap_column(source: &str) -> Option<usize> {
    if source.len() > MAX_SOURCE_BYTES
        || source.chars().any(|character| {
            character > '\u{00ff}'
                || (character.is_control() && !matches!(character, '\t' | '\n' | '\r'))
        })
    {
        return None;
    }

    let mut paragraphs = Vec::<Vec<&str>>::new();
    let mut current = Vec::new();
    let mut fence = None;

    for raw_line in source.lines() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if let Some(open_fence) = fence {
            if closes_fence(line, open_fence) {
                fence = None;
            }
            continue;
        }
        if let Some(open_fence) = opening_fence(line) {
            if !current.is_empty() {
                paragraphs.push(std::mem::take(&mut current));
            }
            fence = Some(open_fence);
            continue;
        }
        if line.trim().is_empty() {
            if !current.is_empty() {
                paragraphs.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        paragraphs.push(current);
    }
    if fence.is_some() {
        // An unclosed fence leaves the remainder structurally ambiguous.
        return None;
    }

    let mut continuations = Vec::new();
    let mut prose_line_count = 0;
    for paragraph in paragraphs {
        if paragraph.iter().any(|line| is_structural(line)) {
            continue;
        }

        prose_line_count += paragraph.len();
        if paragraph.len() < 3 {
            continue;
        }
        let widths = paragraph[..paragraph.len() - 1]
            .iter()
            .map(|line| column_width(line))
            .collect::<Vec<_>>();
        if widths.len() < 2 {
            continue;
        }
        let mut sorted = widths.clone();
        sorted.sort_unstable();
        let median = sorted[sorted.len() / 2];
        if median < MIN_LINE_COLUMN {
            continue;
        }

        // Keep lines near the paragraph's repeated measure. The upper bound
        // drops an isolated long outlier; the lower bound tolerates ordinary
        // variation from words that do not fit exactly at the margin.
        let near_measure = widths
            .into_iter()
            .filter(|width| {
                *width >= median.saturating_mul(3) / 4 && *width <= median.saturating_mul(5) / 4
            })
            .collect::<Vec<_>>();
        if near_measure.len() >= 2 && near_measure.len() * 2 > paragraph.len() {
            continuations.extend(near_measure);
        }
    }

    if continuations.is_empty() {
        return None;
    }

    let qualifying_count = continuations.len();
    // Across the document, enough paragraph lines must exhibit the repeated
    // measure to distinguish it from an occasional formatted passage.
    if qualifying_count * 2 <= prose_line_count {
        return None;
    }

    continuations.into_iter().max()
}

fn column_width(line: &str) -> usize {
    line.chars().fold(0, |column, character| {
        if character == '\t' {
            column + (TAB_STOP - column % TAB_STOP)
        } else {
            column + 1
        }
    })
}

#[derive(Clone, Copy)]
struct Fence {
    marker: char,
    length: usize,
}

fn opening_fence(line: &str) -> Option<Fence> {
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next()?;
    if !matches!(marker, '`' | '~' | ':') {
        return None;
    }
    let length = trimmed
        .chars()
        .take_while(|character| *character == marker)
        .count();
    if length < 3 {
        return None;
    }
    Some(Fence { marker, length })
}

fn closes_fence(line: &str, opening: Fence) -> bool {
    let trimmed = line.trim_start();
    let run_length = trimmed
        .chars()
        .take_while(|character| *character == opening.marker)
        .count();
    run_length >= opening.length && trimmed[run_length..].trim().is_empty()
}

fn is_structural(line: &str) -> bool {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();

    if indent >= 4
        || trimmed.starts_with('|')
        || trimmed.starts_with('>')
        || trimmed.starts_with('#')
        || trimmed.starts_with("{:")
        || is_footnote_definition(trimmed)
        || trimmed.starts_with("<")
        || trimmed.starts_with("---")
        || trimmed.starts_with("***")
        || trimmed.starts_with("___")
        || trimmed.starts_with("===")
        || trimmed.starts_with("+++")
        || is_list_marker(trimmed)
        || is_reference_definition(trimmed)
    {
        return true;
    }

    false
}

fn is_list_marker(line: &str) -> bool {
    let mut chars = line.chars();
    match chars.next() {
        Some('-' | '+' | '*') => chars.next().is_some_and(char::is_whitespace),
        Some(first) if first.is_ascii_digit() => {
            let marker_tail = line
                .char_indices()
                .find(|(_, character)| !character.is_ascii_digit())
                .map(|(index, _)| &line[index..]);
            marker_tail.is_some_and(|tail| tail.starts_with(". ") || tail.starts_with(") "))
        },
        _ => false,
    }
}

fn is_reference_definition(line: &str) -> bool {
    line.starts_with('[') && line.contains("]:")
}

fn is_footnote_definition(line: &str) -> bool {
    line.starts_with("[^") && line.contains("]:")
}

#[cfg(test)]
mod tests {
    use super::hard_wrap_column;

    fn repeated_line(text: &str) -> String {
        format!("{text: <48}").chars().take(48).collect()
    }

    #[test]
    fn ordinary_soft_wrapped_paragraphs_are_not_inferred() {
        let source = "A single physical line is a normal soft-wrapped paragraph in source.\n\nAnother paragraph is also stored on one line, regardless of how it appears in the editor.";
        assert_eq!(hard_wrap_column(source), None);
    }

    #[test]
    fn small_wrapped_passage_does_not_outvote_single_line_paragraphs() {
        let wrapped = format!(
            "{}\n{}\nshort ending",
            repeated_line("A prose continuation reaches the repeated margin"),
            repeated_line("and another continuation reaches the same margin")
        );
        let soft = (0..30)
            .map(|index| format!("A separate short source paragraph {index}."))
            .collect::<Vec<_>>()
            .join("\n\n");
        let source = format!("{wrapped}\n\n{soft}");
        assert_eq!(hard_wrap_column(&source), None);
    }

    #[test]
    fn exactly_half_of_prose_lines_is_not_a_majority() {
        let wrapped = format!(
            "{}\n{}\nshort ending",
            repeated_line("A prose continuation reaches the repeated margin"),
            repeated_line("and another continuation reaches the same margin")
        );
        let source = format!("{wrapped}\n\none additional line");
        assert_eq!(hard_wrap_column(&source), None);
    }

    #[test]
    fn repeated_long_prose_continuations_infer_the_longest_column() {
        let a = repeated_line("This paragraph has repeated prose lines that continue");
        let b = repeated_line("across the same written margin with enough plain text");
        let c = "The final line is short.";
        let source = format!("{a}\n{b}\n{c}");
        assert_eq!(hard_wrap_column(&source), Some(48));
    }

    #[test]
    fn blank_lines_separate_paragraphs_and_short_blocks_do_not_count() {
        let source =
            "one line\n\ntwo lines\ncontinued\n\nA separate long line is not enough by itself.";
        assert_eq!(hard_wrap_column(source), None);
    }

    #[test]
    fn block_structure_is_excluded() {
        let source = "# Heading that happens to be a very long line and should not count\n## Another heading line\n### Third heading line\n\n- A long list item line that resembles a prose wrap continuation\n- Another long list item line with similar width\n- A third long list item line with similar width\n\n```text\nA long code line that resembles prose and another continuation\nA long code line that resembles prose and another continuation\nA long code line that resembles prose and another continuation\n```\n\n| first long table cell with enough words | second cell |\n| -------------------------------------- | ---------- |\n| another long table row with many words | final cell  |\n\n> A quote line that is long enough to resemble a wrap continuation\n> A quote line that is long enough to resemble a wrap continuation\n> A quote line that is long enough to resemble a wrap continuation";
        assert_eq!(hard_wrap_column(source), None);
    }

    #[test]
    fn matching_fences_and_div_closers_exclude_their_contents() {
        let inside = format!(
            "{}\n{}\nshort ending",
            repeated_line("A code continuation that could resemble prose"),
            repeated_line("another code continuation of similar width")
        );
        let prose = format!(
            "{}\n{}\nshort ending",
            repeated_line("This valid prose continuation reaches margin"),
            repeated_line("and next continuation reaches this margin")
        );
        let source = format!(":::div\n{inside}\n:::\n\n{prose}");
        assert_eq!(hard_wrap_column(&source), Some(48));
    }

    #[test]
    fn short_or_mismatched_fence_closers_do_not_end_a_fence() {
        let source = format!(
            "````text\n{}\n{}\n```\n{}\n{}\nshort ending",
            repeated_line("A code continuation that could resemble prose"),
            repeated_line("another code continuation of similar width"),
            repeated_line("still inside the four tick fence"),
            repeated_line("still another code line in that fence")
        );
        assert_eq!(hard_wrap_column(&source), None);

        let mismatched = format!(
            "~~~text\n{}\n{}\n```\n{}\n{}\nshort ending",
            repeated_line("A code continuation that could resemble prose"),
            repeated_line("another code continuation of similar width"),
            repeated_line("still inside the tilde fence"),
            repeated_line("still another code line in that fence")
        );
        assert_eq!(hard_wrap_column(&mismatched), None);
    }

    #[test]
    fn latin1_columns_and_tabs_use_eight_column_stops() {
        let source = "éééééééééééééééééééééééééééééééé\tabcdefghijklmnop\néééééééééééééééééééééééééééééééé\tabcdefghijklmnop\nshort final line";
        assert_eq!(hard_wrap_column(source), Some(56));
    }

    #[test]
    fn wide_or_combining_unicode_returns_none_instead_of_guessing() {
        let source = "界界界界界界界界界界界界界界界界界界界界界界界界界界界界界界界界\n界界界界界界界界界界界界界界界界界界界界界界界界界界界界界界界界\nshort final line";
        assert_eq!(hard_wrap_column(source), None);

        let combining = "e\u{301}e\u{301}e\u{301}";
        assert_eq!(hard_wrap_column(combining), None);
    }

    #[test]
    fn non_whitespace_latin1_control_characters_return_none() {
        assert_eq!(hard_wrap_column("plain text\u{0085}with control"), None);
        assert_eq!(hard_wrap_column("plain\u{007f}text"), None);
    }

    #[test]
    fn oversized_source_returns_none_before_scanning() {
        let source = "x".repeat(1024 * 1024 + 1);
        assert_eq!(hard_wrap_column(&source), None);
    }

    #[test]
    fn one_oversized_outlier_does_not_expand_the_wrap_column() {
        let line1 = repeated_line("This paragraph has repeated prose lines that continue");
        let line2 = repeated_line("across the same written margin with enough plain text");
        let line3 = repeated_line("a third prose continuation continues the pattern");
        let outlier = format!("{}", "x".repeat(400));
        let source = format!("{line1}\n{line2}\n{line3}\n{outlier}\nshort final line");
        assert_eq!(hard_wrap_column(&source), Some(48));
    }
}
