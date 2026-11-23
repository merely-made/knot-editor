// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Sandbox receipts for the Lua reading lane, the counterpart of
//! `reading_sandbox.rs`. A reading gets the reading surface and nothing else:
//! no file, no fetch, no module, no `load`, no reach into the host's input, and
//! no write to a host value.
//!
//! Nothing here is checked by reading an error's wording alone. Where a name
//! must be absent the test reads the name itself (`io == nil`); where a call
//! must fail it asserts the failure is a runtime error and that a call to a
//! name that is present resolves (the control in the surface test).

use knot_document::KnotDocumentSession;
use knot_readings::backend::ReadingBackend;
use knot_readings::didge::DidgeReadingBackend;
use knot_readings::{
    DIDGE_READING_SURFACE, ReadingBudget, ReadingError, ReadingInput, ReadingResult, ReadingScript,
};

fn input() -> ReadingInput {
    ReadingInput::from_session(&KnotDocumentSession::read_only(
        "memory:sandbox",
        "# Sandbox\n\nOne [link](/target){rel=cites} here.\n",
    ))
    .unwrap()
}

fn evaluate_with(source: &str, budget: ReadingBudget) -> Result<ReadingResult, ReadingError> {
    DidgeReadingBackend.run(&ReadingScript::new("sandbox", source), &input(), budget)
}

fn evaluate(source: &str) -> Result<ReadingResult, ReadingError> {
    evaluate_with(source, ReadingBudget::default())
}

/// The one row a script returns, as its label.
fn label_of(source: &str) -> String {
    evaluate(source).unwrap().rows.remove(0).label
}

/// A call that must not resolve is a runtime error, never a result.
fn must_not_resolve(call: &str) {
    match evaluate(call) {
        Err(ReadingError::Runtime { message }) => assert!(
            message.contains("nil"),
            "{call} failed for the wrong reason: {message}"
        ),
        Err(ReadingError::Compile { message, .. }) => {
            assert!(!message.is_empty(), "{call} must not compile into anything")
        },
        other => panic!("{call} was not refused: {other:?}"),
    }
}

/// The globals every state has besides the reading surface: Lua's `base`
/// (without `load`, `dofile`, `loadfile`, `require` or `print`) and the
/// libraries the sandbox loads. A name the fork or the sandbox adds shows up
/// here as a failure, to be read and named, not inherited.
const ALLOWED_GLOBALS: &[&str] = &[
    "_VERSION",
    "assert",
    "collectgarbage",
    "coroutine",
    "error",
    "getmetatable",
    "ipairs",
    "math",
    "next",
    "pairs",
    "pcall",
    "rawget",
    "rawlen",
    "rawset",
    "select",
    "setmetatable",
    "string",
    "table",
    "tonumber",
    "tostring",
    "type",
    "utf8",
];

/// (11) The allowlist receipt: every name on the reading surface resolves, and
/// the globals are the surface plus Lua's named libraries and nothing else.
#[test]
fn the_reading_surface_is_the_whole_surface() {
    let calls = [
        ("document", r#"return row(document().address)"#),
        ("outline", r#"return row("" .. #outline())"#),
        ("sections", r#"return row("" .. #sections())"#),
        ("folds", r#"return row("" .. #folds())"#),
        ("links", r#"return row("" .. #links())"#),
        ("relations", r#"return row("" .. #relations())"#),
        ("span", r#"return row("s", span(0, 1))"#),
        ("row", r#"return row("r")"#),
        ("note", r#"return note("n", "djot")"#),
        ("text_in", r#"return row(document():text_in(span(0, 1)))"#),
    ];
    assert_eq!(calls.len(), DIDGE_READING_SURFACE.len());
    for (name, call) in calls {
        assert!(
            DIDGE_READING_SURFACE.contains(&name),
            "{name} is callable but not named on the surface"
        );
        assert!(
            evaluate(call).is_ok(),
            "{name} is on the surface but did not resolve"
        );
    }
    for name in DIDGE_READING_SURFACE {
        assert!(
            calls.iter().any(|(called, _)| *called == name),
            "{name} is named on the surface but unexercised"
        );
    }
    let globals = label_of(
        r#"local names = {}
           for name in pairs(_ENV) do names[#names + 1] = name end
           table.sort(names)
           return row(table.concat(names, ","))"#,
    );
    for name in globals.split(',') {
        // `text_in` is a method of a document, not a global.
        let named = DIDGE_READING_SURFACE.contains(&name) && name != "text_in";
        assert!(
            named || ALLOWED_GLOBALS.contains(&name),
            "{name} is a global the sandbox did not name: {globals}"
        );
    }
    for name in ALLOWED_GLOBALS {
        assert!(globals.split(',').any(|got| got == *name), "{name} is gone");
    }
}

/// (12) Nothing file-shaped exists to call.
#[test]
fn no_file_binding_exists() {
    for call in [
        r#"open_file("/etc/passwd")"#,
        r#"File("/etc/passwd")"#,
        r#"read_file("/etc/passwd")"#,
        r#"write_file("out.txt", "x")"#,
        r#"remove_file("out.txt")"#,
        r#"io.open("/etc/passwd")"#,
        r#"io.write("x")"#,
        r#"io.lines("/etc/passwd")"#,
        r#"os.remove("out.txt")"#,
        r#"os.execute("true")"#,
        r#"dofile("/etc/passwd")"#,
        r#"loadfile("/etc/passwd")"#,
    ] {
        must_not_resolve(call);
    }
    assert_eq!(
        label_of(
            r#"return row(tostring(io) .. tostring(os) .. tostring(package) .. tostring(debug))"#
        ),
        "nilnilnilnil"
    );
}

/// (13) Nothing can bring in code that really is on disk. The plan named a
/// sibling script in the cwd; an absolute path is the same receipt without
/// changing a cwd the other tests share. The control is the same script with
/// the reach removed, which runs.
#[test]
fn no_module_or_file_can_be_loaded_from_disk() {
    let directory = tempfile::tempdir().unwrap();
    let sibling = directory.path().join("sibling.lua");
    std::fs::write(&sibling, "return row('borrowed')").unwrap();
    assert!(sibling.exists(), "the sibling script really is on disk");
    let stem = directory.path().join("sibling");
    let stem = stem.to_string_lossy().replace('\\', "/");
    let path = sibling.to_string_lossy().replace('\\', "/");
    for call in [
        format!(r#"return require("{stem}")"#),
        r#"return require("sibling")"#.to_owned(),
        format!(r#"return dofile("{path}")"#),
        format!(r#"return loadfile("{path}")()"#),
        format!(r#"return package.loadlib("{path}", "x")"#),
    ] {
        let outcome = evaluate(&call);
        assert!(
            matches!(outcome, Err(ReadingError::Runtime { .. })),
            "{call} was not refused: {outcome:?}"
        );
    }
    assert_eq!(label_of("return row('not borrowed')"), "not borrowed");
}

/// (13b) `load` is not there, so a script cannot assemble its own program, and
/// `string.dump`, `setfenv`-shaped tricks and the environment are not a way in
/// either.
#[test]
fn load_and_friends_are_absent() {
    let absent = label_of(
        r#"return row(tostring(load) .. "|" .. tostring(loadstring) .. "|" .. tostring(string.dump)
                      .. "|" .. tostring(setfenv) .. "|" .. tostring(getfenv)
                      .. "|" .. tostring(_G))"#,
    );
    assert_eq!(absent, "nil|nil|nil|nil|nil|nil");
    for source in [
        r#"return load("return row('smuggled')")()"#,
        r#"return loadstring("return row('smuggled')")()"#,
        r#"return (function() return string.dump(function() end) end)()"#,
    ] {
        let outcome = evaluate(source);
        assert!(
            matches!(outcome, Err(ReadingError::Runtime { .. })),
            "{source} was not refused: {outcome:?}"
        );
    }
}

/// (14) Nothing network-shaped exists to call.
#[test]
fn no_fetch_binding_exists() {
    for call in [
        r#"http_get("https://example.test/")"#,
        r#"fetch("https://example.test/")"#,
        r#"get("https://example.test/")"#,
        r#"request("https://example.test/")"#,
        r#"socket.tcp()"#,
        r#"http.get("https://example.test/")"#,
    ] {
        must_not_resolve(call);
    }
}

/// (15) There is no `print` and no `debug`: the Rhai lane silences them, and
/// here they do not exist, so there is nothing to silence. (Stdout is not
/// observable from in here; what is asserted is the absence and that a reading
/// without them is untouched.)
#[test]
fn print_and_debug_do_not_exist() {
    assert_eq!(
        label_of(r#"return row(tostring(print) .. "|" .. tostring(debug))"#),
        "nil|nil"
    );
    must_not_resolve(r#"print("noise")"#);
    must_not_resolve(r#"debug.traceback("noise")"#);
    let result = evaluate(r#"return row("only this")"#).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].label, "only this");
    assert!(result.notes.is_empty());
}

/// (16) A script gets copies. Nothing it does reaches the host's input, and a
/// host value is a read-only userdata: every write to it fails, however it is
/// attempted, and a write to a table the script owns works (the control).
#[test]
fn a_script_cannot_mutate_the_host_input() {
    let before = input();
    let mutated = evaluate(
        r#"local t = document().text
           t = t .. " appended"
           return row(#document().text .. "|" .. #t)"#,
    )
    .unwrap();
    let parts = mutated.rows[0].label.clone();
    let (seen, local) = parts.split_once('|').unwrap();
    assert_ne!(seen, local, "the script edited only its own copy");
    assert_eq!(
        seen.parse::<usize>().unwrap(),
        before.text.len(),
        "the document the script re-read is the host's, unchanged"
    );
    for write in [
        r#"local h = outline(); h[1].label = "rewritten"; return row(h[1].label)"#,
        r#"local d = document(); d.text = "rewritten"; return row(d.text)"#,
        r#"local l = links(); l[1].target = "elsewhere"; return row(l[1].target)"#,
        r#"local s = span(0, 1); s.start = 5; return row("x", s)"#,
        r#"local s = sections()[1]; s.body = nil; return row("x")"#,
        r#"local r = row("x"); r.label = "y"; return r"#,
        r#"local d = document(); d.new_field = 1; return row("x")"#,
        r#"local h = outline(); rawset(h[1], "label", "rewritten"); return row("x")"#,
        r#"local h = outline(); setmetatable(h[1], {}); return row("x")"#,
        r#"local h = outline(); getmetatable(h[1]).__newindex = nil; return row("x")"#,
        r#"local h = outline(); h[1]["label"] = nil; return row("x")"#,
    ] {
        let outcome = evaluate(write);
        assert!(
            matches!(outcome, Err(ReadingError::Runtime { .. })),
            "a reader value accepted a write: {write}: {outcome:?}"
        );
    }
    // Control: the same shapes on tables the script owns are fine, and the
    // host values read back what the host put there.
    assert_eq!(
        label_of(
            r#"local own = {label = "a"}; own.label = "b"; local h = outline()
               local copy = {label = h[1].label}; copy.label = "changed"
               return row(own.label .. "|" .. h[1].label .. "|" .. copy.label)"#
        ),
        "b|Sandbox|changed"
    );
    // A list a reader returns is the script's own: it may reorder or empty it.
    assert_eq!(
        label_of(r#"local h = outline(); h[1] = nil; return row(tostring(h[1]) .. #outline())"#),
        "nil1"
    );
    assert_eq!(input(), before);
}

/// Each call builds its values afresh, so a script never holds a value that
/// another call, or the host, also holds.
#[test]
fn readers_hand_back_fresh_values() {
    assert_eq!(
        label_of(r#"return row(tostring(outline()[1] == outline()[1]))"#),
        "false"
    );
}
