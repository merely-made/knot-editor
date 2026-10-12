# Copyright 2026 Mark Alan Boykin
# SPDX-License-Identifier: MPL-2.0
"""Read-only verification of completed-preview native receipts."""
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
for index, name in enumerate(("status-qualified-wide", "status-qualified-compact")):
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
    assert len(captures) == 9
    fields = lambda name: captures[name]["fields"]
    ready, started, finished, stable, restarted, refinished, kept, undone, redone = map(fields, (
        "status-ready", "status-started", "status-finished", "status-finished-stable",
        "status-restarted", "status-refinished", "status-kept", "status-undone", "status-redone"))
    assert ready["scene_motion_ticks"] == ready["scene_motion_positions"] == "0"
    for entry in (ready, kept, undone, redone):
        assert entry["scene_motion_status"] == "ready"
        assert entry["scene_motion_running"] == "false"
    for entry in (started, restarted):
        assert entry["scene_motion_status"] == "running"
        assert entry["scene_motion_running"] == "true"
        # Commands run on subsequent presented frames, after binding at tick 0.
        # The host test checks the immediate reset; these captures prove replay
        # began a fresh run and has not retained the completed 120-tick state.
        assert 0 < int(entry["scene_motion_ticks"]) < 120
    for entry in (finished, stable, refinished):
        assert entry["scene_motion_status"] == "finished"
        assert entry["scene_motion_running"] == "false"
        assert entry["scene_motion_ticks"] == "120"
        assert entry["scene_motion_digest"] == finished["scene_motion_digest"]
    assert finished["scene_motion_digest"] != started["scene_motion_digest"]
    assert refinished["scene_motion_digest"] == kept["scene_positions_blake3"]
    assert ready["scene_positions_blake3"] == undone["scene_positions_blake3"]
    assert ready["scene_placement_steps"] == undone["scene_placement_steps"] == "2"
    for entry in (kept, redone):
        assert entry["scene_positions_blake3"] == kept["scene_positions_blake3"]
        assert entry["scene_motion_positions"] == entry["scene_motion_ticks"] == "0"
        assert entry["scene_authored_positions"] == "2"
        assert int(entry["scene_placement_steps"]) >= 3
    for capture in ("status-started", "status-finished", "status-finished-stable", "status-restarted", "status-refinished", "status-kept"):
        entry = fields(capture)
        painted = json.loads(entry["scene_motion_canvas_painted"])
        visible = json.loads(entry["scene_motion_canvas_visible"])
        assert painted and visible and len(painted) == len(visible) == 4
        assert all(abs(a-b) < 1.0 for a,b in zip(painted, visible)), (name, capture, painted, visible)
        assert abs(painted[2]-222.0) < 1.0 and abs(painted[3]-202.0) < 1.0
    presentation = re.search(r"^presentation: (\d+) frames, (\d+) redraws, (\d+) unpresented redraws$", receipt, re.M)
    assert presentation and int(presentation[3]) == 0
    runs.append(dict(name=name, presented_frames=int(presentation[1]),
        redraws=int(presentation[2]), captures=captures))
print(json.dumps(dict(schema="knot.dynamics-status-windows/v1", base_source="156626f",
    evidence_root=str(root), binary=str(binary), binary_sha256=binary_sha256,
    source_bytes=len(source), source_sha256=sha(source), captures=18,
    tests=["fresh process discards runtime", "authored schedule finishes at 120 ticks",
        "finished pose stays fixed", "Play explicitly reseeds a finished controller",
        "repeat run returns the same final pose", "Keep records that exact pose",
        "one Undo restores authored placement", "Redo restores kept placement",
        "source unchanged", "roles and slot preserved", "complete camera visible"], runs=runs), indent=2))
