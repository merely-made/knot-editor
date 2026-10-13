# Copyright 2026 Mark Alan Boykin
# SPDX-License-Identifier: MPL-2.0
"""Read-only verification of isolated font-role native evidence.

Usage: python3 verify-native.py /absolute/evidence/root /absolute/binary SOURCE_SHA
The verifier checks recorded state and file identity, not visual quality.
"""
import hashlib
import json
from pathlib import Path
import re
import struct
import sys

root, binary = map(Path, sys.argv[1:3])
runtime = sys.argv[3]
assert root.is_absolute() and binary.is_absolute()
sha = lambda data: hashlib.sha256(data).hexdigest()
fixture = root / "qualified-fixture"
assert (fixture / "PUBLIC-TEST-KEYS-NOT-A-PERSONAL-WALLET").read_bytes() == (
    b"Synthetic acceptance authority only; all keys are public test constants.\n"
)
source = (fixture / "essay.djot").read_bytes()
assert source == b"# Night light\n\nNight light by the river.\n"
expected = {
    "reading": ["IBM Plex Mono"],
    "heading": ["Source Serif 4"],
    "label": ["IBM Plex Mono"],
    "code": ["Knot Missing Acceptance Face"],
}
pre = json.loads((root / "pre-reopen-files.json").read_text())
for path, digest in pre.items():
    file = Path(path)
    assert sha((file if file.is_absolute() else root / file).read_bytes()) == digest, path
preferences = json.loads((fixture / "preferences.json").read_text())
assert preferences["appearance"]["theme-choice"]["fonts"] == expected
binary_sha = sha(binary.read_bytes())
runs = []
captures = {}
for name, count, width in (("qualified-wide", 9, 2200),
                           ("qualified-reopen", 2, 2200),
                           ("qualified-compact", 2, 840)):
    directory = root / name
    process = json.loads((directory / "process.json").read_text())
    assert process["success"] and not process["timeout"]
    assert process["launcher_exit"] == 0
    assert process["binary_sha256"] == binary_sha
    scenario = {"qualified-wide": "font_roles_apply.scn",
                "qualified-reopen": "font_roles_reopen.scn",
                "qualified-compact": "font_roles_compact.scn"}[name]
    repo = Path(__file__).resolve().parents[3]
    assert process["scenario_sha256"] == sha((repo / "scenarios" / scenario).read_bytes())
    receipt = (directory / "scenario.done").read_text()
    assert receipt.startswith("RESULT ok\n") and "FAIL:" not in receipt
    log = (directory / "native.log").read_text()
    assert "initial presentation grace 60s" in log
    assert log.count("normal 10s presentation timeout restored") == 1
    current = {}
    for line in receipt.splitlines():
        match = re.fullmatch(r"capture-pairing (\S+) (\{.*\})", line)
        if not match:
            continue
        item = json.loads(match[2])
        fields = item["fields"]
        assert fields == item["pairing"]["product_projection"]["fields"]
        assert fields["source_bytes"] == "41"
        assert fields["source_face"] == "ibm-plex-mono"
        assert fields["font_size"] == "16" and fields["measure"] == "medium"
        assert fields["theme_id"] == "user:role-fixture"
        assert fields["theme_mode"] == (
            "light" if match[1] in ("font-roles-staged", "font-roles-applied") else "dark"
        )
        pixels = (directory / (match[1] + ".png")).read_bytes()
        assert pixels[:8] == b"\x89PNG\r\n\x1a\n"
        assert struct.unpack(">II", pixels[16:24]) == (item["width"], item["height"])
        assert item["width"] == width
        current[match[1]] = dict(width=item["width"], height=item["height"],
                                sha256=sha(pixels), fields=fields)
    assert len(current) == count == process["captures"]
    frame = re.search(r"^presentation: (\d+) frames", receipt, re.M)
    assert frame and int(frame[1]) > 0
    captures.update(current)
    runs.append(dict(name=name, frames=int(frame[1]), captures=current))

fields = lambda name: captures[name]["fields"]
staged = fields("font-roles-staged")
assert json.loads(staged["requested_fonts"]) == {}
applied = fields("font-roles-applied")
initial = {**expected, "label": ["Source Serif 4"]}
for name in ("font-roles-applied", "font-roles-dark", "font-roles-workshop-applied"):
    assert json.loads(fields(name)["requested_fonts"]) == initial
before = fields("font-roles-scene-before-play")
running = fields("font-roles-scene-running")
assert running["scene_motion_running"] == "true"
assert int(running["scene_motion_ticks"]) > 0
assert int(running["scene_motion_positions"]) > 0
for name in ("font-roles-scene-metrics-reset", "font-roles-scene-cold",
             "font-roles-recipe-retained", "font-roles-recipe-fresh-process-reopen"):
    item = fields(name)
    assert item["scene_motion_running"] == "false"
    assert item["scene_motion_ticks"] == item["scene_motion_positions"] == "0"
    assert item["scene_motion_roles"] == item["scene_authored_positions"] == "1"
    assert item["scene_motion_slot"] == "true" and item["scene_placement_steps"] == "1"
    assert item["scene_positions_blake3"] == before["scene_positions_blake3"]
    assert json.loads(item["requested_fonts"]) == expected
reset = fields("font-roles-scene-metrics-reset")
assert int(reset["composition_recipe_measurements"]) > int(before["composition_recipe_measurements"])
for name in ("font-roles-fresh-process-reopen", "font-roles-compact-search", "font-roles-compact-staged"):
    assert json.loads(fields(name)["requested_fonts"]) == expected
assert len({item["fields"]["source_blake3"] for item in captures.values()}) == 1
print(json.dumps(dict(schema="knot.font-roles-native/v1", runtime=runtime,
    startup_policy="marked fixture only: initial 60s, normal 10s after first presentation",
    binary_sha256=binary_sha, source_sha256=sha(source), source_bytes=len(source),
    capture_count=len(captures), presented_frames=sum(run["frames"] for run in runs),
    font_requests=expected, reopen_file_digests=pre, runs=runs), indent=2))
