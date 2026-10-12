# Copyright 2026 Mark Alan Boykin
# SPDX-License-Identifier: MPL-2.0
"""Read-only verification of native motion/Keep receipts on the revised layout."""
import hashlib
import json
from pathlib import Path
import re
import struct
import sys

root, binary = map(Path, sys.argv[1:3])
assert root.is_absolute() and binary.is_absolute()
fixture = root / "kept-fixture"
assert (fixture / "PUBLIC-TEST-KEYS-NOT-A-PERSONAL-WALLET").is_file()
source = (fixture / "essay.djot").read_bytes()
assert source == b"# Night light\n\nNight light by the river.\n"
sha = lambda data: hashlib.sha256(data).hexdigest()
binary_sha256 = sha(binary.read_bytes())
runs = []
for index, name in enumerate(("viewport-revealed-wide", "viewport-revealed-compact")):
    directory = root / name
    receipt = (directory / "scenario.done").read_text(encoding="utf-8")
    assert receipt.startswith("RESULT ok\n") and "FAIL:" not in receipt
    identity = json.loads((directory / "binary.json").read_text(encoding="utf-8-sig"))
    assert identity["Algorithm"] == "SHA256" and identity["Hash"].lower() == binary_sha256
    captures = {}
    for line in receipt.splitlines():
        if not line.startswith("capture-pairing "):
            continue
        match = re.fullmatch(r"capture-pairing (\S+) (\{.*\})", line)
        assert match
        item = json.loads(match[2])
        fields = item["fields"]
        assert fields == item["pairing"]["product_projection"]["fields"]
        assert fields["source_bytes"] == "41"
        assert fields["scene_motion_roles"] == "1" and fields["scene_motion_slot"] == "true"
        pixels = (directory / (match[1] + ".png")).read_bytes()
        assert pixels[:8] == b"\x89PNG\r\n\x1a\n"
        assert struct.unpack(">II", pixels[16:24]) == (item["width"], item["height"])
        assert item["width"] == (2200, 840)[index]
        captures[match[1]] = dict(width=item["width"], height=item["height"],
            png_sha256=sha(pixels), fields=fields,
            presentation_sequence=item["pairing"]["presentation"]["sequence"])
    assert len(captures) == 8
    for name in ("viewport-started", "viewport-running", "viewport-paused", "viewport-paused-stable", "viewport-kept"):
        fields = captures[name]["fields"]
        painted = json.loads(fields["scene_motion_canvas_painted"])
        visible = json.loads(fields["scene_motion_canvas_visible"])
        assert painted and visible and len(painted) == len(visible) == 4
        assert all(abs(a-b) < 1.0 for a,b in zip(painted,visible)), (name,painted,visible)
        assert abs(painted[2]-222.0) < 1.0 and abs(painted[3]-202.0) < 1.0
    fields = lambda name: captures[name]["fields"]
    authored, started, moving = map(fields, ("viewport-authored", "viewport-started", "viewport-running"))
    paused, stable, kept = map(fields, ("viewport-paused", "viewport-paused-stable", "viewport-kept"))
    undone, redone = map(fields, ("viewport-undone", "viewport-redone"))
    assert authored["scene_motion_ticks"] == authored["scene_motion_positions"] == "0"
    assert started["scene_motion_running"] == moving["scene_motion_running"] == "true"
    assert int(moving["scene_motion_ticks"]) > int(started["scene_motion_ticks"])
    assert moving["scene_motion_digest"] != started["scene_motion_digest"]
    assert paused["scene_motion_running"] == stable["scene_motion_running"] == "false"
    assert paused["scene_motion_ticks"] == stable["scene_motion_ticks"]
    assert paused["scene_motion_digest"] == stable["scene_motion_digest"]
    assert paused["scene_motion_digest"] == kept["scene_positions_blake3"]
    assert authored["scene_positions_blake3"] == undone["scene_positions_blake3"]
    assert authored["scene_placement_steps"] == undone["scene_placement_steps"] == "2"
    for entry in (kept, redone):
        assert entry["scene_positions_blake3"] == kept["scene_positions_blake3"]
        assert entry["scene_motion_positions"] == entry["scene_motion_ticks"] == "0"
        assert entry["scene_authored_positions"] == "2"
        assert int(entry["scene_placement_steps"]) >= 3
    presentation = re.search(r"^presentation: (\d+) frames, (\d+) redraws, (\d+) unpresented redraws$", receipt, re.M)
    assert presentation and int(presentation[3]) == 0
    runs.append(dict(name=name, presented_frames=int(presentation[1]),
        redraws=int(presentation[2]), captures=captures))
print(json.dumps(dict(schema="knot.dynamics-viewport-windows/v1", base_source="63aa695",
    evidence_root=str(root), binary=str(binary), binary_sha256=binary_sha256,
    source_bytes=len(source), source_sha256=sha(source), captures=16,
    tests=["fresh process discards runtime", "motion advances", "pause freezes pose and ticks",
        "Keep records the exact displayed pose", "one Undo restores authored placement",
        "Redo restores kept placement", "source unchanged", "roles and slot preserved",
        "complete camera box visible after playback actions"], runs=runs), indent=2))
