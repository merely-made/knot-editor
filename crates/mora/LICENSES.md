# Licenses in this repository

**This repository: MPL-2.0.** Every file Mark wrote carries Exhibit A and the
SPDX tag `MPL-2.0`, per the
[license posture brief](../mere/design_docs/2026-08-22_license_posture_brief.md)
of 2026-08-22 (mere `design_docs/2026-08-22_license_posture_brief.md`). The full
text is in [`LICENSE`](LICENSE).

This file is the provenance ledger. It is the authority for what the relicense
tool (mere `scripts/relicense_headers.py`) skips: the backtick-quoted paths in
the **Retained licenses** table are never touched. Provenance comes before
license: a file gets Exhibit A only if Mark wrote it.

## Retained licenses

**None.** Every tracked file in this repository is Mark's own work. The
discovery grep of 2026-09-03 — `Copyright` unqualified, `Licensed under`,
`Permission is hereby granted`, `Apache License`, and SPDX lines naming anything
but MPL-2.0, over all seven tracked sources — returned nothing outside the two
license texts themselves, whose notice read `Copyright (c) 2026 Mark AB
(markik)` and was therefore his.

`mora` has **zero dependencies** and ships **no pronunciation lexicon**. The one
place a third-party corpus could have entered is the English module, and it does
not: `src/english.rs` holds a 39-entry ARPAbet table (symbol, phonological
class, and a long/diphthong flag) and a line splitter for CMUdict-shaped input.
The ARPAbet symbol inventory is a phone-set notation, not a licensed corpus, and
the class and length annotations beside it are Mark's own encoding.
`english::cmudict_entry` parses the **format** CMUdict publishes; none of
CMUdict's data is embedded, and storage is left to the consumer, as the README's
"Pronunciations" section says. CMUdict itself is BSD-2-Clause, and a consumer
who bundles it takes on that notice in their own tree, not this one.

If a lexicon, word list, or phone table from elsewhere is ever vendored here, it
is retained third-party content and belongs in a table in this section with its
license, upstream, and notice file, so the tool skips it.

## Exceptions under the fork/vendor criterion

**None.** The brief's §4 test — a crate stays MIT OR Apache-2.0 only when a
third party would need to *modify or vendor* it rather than merely link it —
admits nothing here. `mora` is consumed by linking.

`mora` **0.1.0 is published under MIT OR Apache-2.0, and that version keeps that
grant permanently.** MPL-2.0 ships at the crate's next functional bump; this
sweep changes no version and publishes nothing, per the sweep plan's
invariant 8.

## How to add a file from elsewhere

1. Do not delete or rewrite the upstream copyright or license notice, ever.
2. Add its path to **Retained licenses** above with its license, upstream URL,
   and where its notice text lives. The tool then skips it automatically.
3. If it is a substantial derivative rather than a verbatim import, the brief's
   rule is MPL-2.0 on the derivative *with the upstream notice retained* —
   record it with that disposition so the distinction is not lost.
4. Never add `license-file` to an owned manifest; the field is for retained
   third-party crates only.
5. Re-run `python ../mere/scripts/relicense_headers.py --repo . --audit` and
   confirm the owned source count moved by exactly what you expected.
