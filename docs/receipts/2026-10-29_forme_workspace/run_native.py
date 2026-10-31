#!/usr/bin/env python3
"""Run the bounded Forme native gate with isolated files and real process exits."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def slot(profile):
    value = json.loads((profile / "Knot/preferences.json").read_text())["workspace_forme"]
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    record = json.loads(bytes(value["record"]))
    fractions = [child["fraction"] for child in record["geometry"]["Split"]["children"]]
    assert all(abs(a - b) < 0.0001 for a, b in zip(fractions, [0.45, 0.55]))
    assert len(fractions) == 2
    return hashlib.sha256(encoded).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[3]
    binary = args.binary.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    fixture = output / "field_notes.djot"
    shutil.copyfile(repo / "scenarios/fixtures/field_notes.djot", fixture)
    profile = output / "profile"
    (profile / "Knot").mkdir(parents=True)
    (profile / "Knot/preferences.json").write_text(json.dumps({
        "version": 1,
        "appearance": {"font-size": 12, "measure": "narrow", "follow-hard-wrap": False},
    }, indent=2) + "\n")
    source_paths = ["Cargo.toml", "Cargo.lock", "apps/desktop/Cargo.toml"]
    source_paths += [str(p.relative_to(repo)) for p in sorted((repo / "apps/desktop/src").glob("*.rs"))]
    source_paths += ["apps/desktop/tests/workspace_forme.rs", "scenarios/workspace_forme.scn",
                     "scenarios/workspace_forme_reopen.scn"]
    source_hashes = {name: digest(repo / name) for name in source_paths}
    binary_hash = digest(binary)
    fixture_hash = digest(fixture)
    results = {
        "source_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
        "source_hashes": source_hashes,
        "binary": {"path": str(binary), "sha256": binary_hash},
        "fixture_sha256": fixture_hash,
        "runs": [],
    }

    def run(name, scenario, run_profile, width, expected_captures):
        folder = output / name
        folder.mkdir()
        receipt = folder / "scenario.done"
        overrides = {
            "APPDATA": str(run_profile),
            "XDG_CONFIG_HOME": str(output / "xdg-config"),
            "XDG_DATA_HOME": str(output / "xdg-data"),
            "MERE_ROOT": str(output / "mere"),
            "KNOT_SCENARIO": str(repo / "scenarios" / scenario),
            "KNOT_CAPTURE_DIR": str(folder),
            "KNOT_RECEIPT": str(receipt),
            "KNOT_CAPTURE_CORRELATION": "1",
            "KNOT_TEST_WIDTH": str(width),
            "KNOT_TEST_HEIGHT": "800",
        }
        command = [str(binary), str(fixture)]
        with (folder / "native.log").open("w") as log:
            process = subprocess.run(command, cwd=repo, env=os.environ | overrides,
                                     stdout=log, stderr=subprocess.STDOUT, timeout=120)
        text = receipt.read_text() if receipt.exists() else ""
        captures = []
        for path in sorted(folder.glob("*.png")):
            header = path.read_bytes()[:24]
            assert header[:8] == b"\x89PNG\r\n\x1a\n"
            captures.append({"path": str(path.relative_to(output)), "sha256": digest(path),
                             "pixels": list(struct.unpack(">II", header[16:24]))})
        item = {"name": name, "command": command, "environment": overrides,
                "exit_code": process.returncode, "result_ok": text.startswith("RESULT ok\n"),
                "captures": captures,
                "presentation": next((s for s in text.splitlines() if s.startswith("presentation:")), None)}
        results["runs"].append(item)
        (output / "verification.json").write_text(json.dumps(results, indent=2) + "\n")
        assert process.returncode == 0 and item["result_ok"], item
        assert len(captures) == expected_captures, captures
        assert digest(binary) == binary_hash
        assert digest(fixture) == fixture_hash
        print(json.dumps({"name": name, "exit_code": process.returncode,
                          "result_ok": item["result_ok"], "captures": len(captures),
                          "presentation": item["presentation"]}), flush=True)

    run("seed", "workspace_forme.scn", profile, 1100, 3)
    saved = slot(profile)
    run("reopen", "workspace_forme_reopen.scn", profile, 1100, 1)
    assert slot(profile) == saved
    compact_profile = output / "profile-default-compact"
    shutil.copytree(profile, compact_profile)
    prefs = compact_profile / "Knot/preferences.json"
    value = json.loads(prefs.read_text())
    value["appearance"].update({"font-size": 16, "measure": "medium", "follow-hard-wrap": True})
    prefs.write_text(json.dumps(value, indent=2) + "\n")
    run("default-compact", "workspace_forme_reopen.scn", compact_profile, 420, 1)
    assert slot(compact_profile) == saved
    assert all(digest(repo / name) == sha for name, sha in source_hashes.items())
    results["saved_slot_sha256"] = saved
    results["fixture_unchanged"] = True
    results["source_and_binary_unchanged"] = True
    (output / "verification.json").write_text(json.dumps(results, indent=2) + "\n")


if __name__ == "__main__":
    main()
