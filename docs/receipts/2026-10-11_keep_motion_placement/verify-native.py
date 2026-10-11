# Copyright 2026 Mark Alan Boykin
# SPDX-License-Identifier: MPL-2.0
"""Verify the explicit Keep/Undo/Redo/Retain and fresh-reopen native receipts."""
import hashlib
import json
from pathlib import Path
import re
import struct
import sys

root, binary = map(Path, sys.argv[1:3])
assert root.is_absolute() and binary.is_absolute()
assert (root / "kept-fixture" / "PUBLIC-TEST-KEYS-NOT-A-PERSONAL-WALLET").is_file()
source = (root / "kept-fixture" / "essay.djot").read_bytes()
assert source == b"# Night light\n\nNight light by the river.\n"
sha = lambda data: hashlib.sha256(data).hexdigest()
runs = []
captures = {}
for index, name in enumerate(("kept-promotion", "kept-identity-reopen")):
    directory = root / name
    receipt = (directory / "scenario.done").read_text(encoding="utf-8")
    assert receipt.startswith("RESULT ok\n") and "FAIL:" not in receipt
    current = {}
    for line in receipt.splitlines():
        if not line.startswith("capture-pairing "):
            continue
        match = re.fullmatch(r"capture-pairing (\S+) (\{.*\})", line)
        assert match
        item = json.loads(match[2])
        fields = item["fields"]
        assert fields == item["pairing"]["product_projection"]["fields"]
        assert fields["source_bytes"] == "41"
        assert fields["scene_motion_roles"] == "1"
        assert fields["scene_motion_slot"] == "true"
        assert fields["scene_motion_running"] == "false"
        pixels = (directory / (match[1] + ".png")).read_bytes()
        assert pixels[:8] == b"\x89PNG\r\n\x1a\n"
        assert struct.unpack(">II", pixels[16:24]) == (item["width"], item["height"])
        current[match[1]] = dict(width=item["width"], height=item["height"],
            png_sha256=sha(pixels), fields=fields,
            presentation_sequence=item["pairing"]["presentation"]["sequence"])
    assert len(current) == (5, 1)[index]
    presentation = re.search(r"^presentation: (\d+) frames, (\d+) redraws, (\d+) unpresented redraws$", receipt, re.M)
    assert presentation and int(presentation[3]) == 0
    identity = json.loads((directory / "binary.json").read_text(encoding="utf-8-sig"))
    assert identity["Algorithm"] == "SHA256"
    executable_sha256 = identity["Hash"].lower()
    assert re.fullmatch(r"[0-9a-f]{64}", executable_sha256)
    if index == 1:
        assert executable_sha256 == sha(binary.read_bytes())
    runs.append(dict(name=name, presented_frames=int(presentation[1]), redraws=int(presentation[2]),
        binary_sha256=executable_sha256,
        source_phase=("keep-runtime", "immutable-item-selector-and-fixture-inspection")[index], captures=current))
    captures.update(current)
before = captures["placement-preview-paused"]["fields"]
kept = captures["placement-kept"]["fields"]
undone = captures["placement-undone"]["fields"]
assert before["scene_authored_positions"] == undone["scene_authored_positions"] == "1"
assert before["scene_placement_steps"] == undone["scene_placement_steps"] == "2"
assert before["scene_positions_blake3"] == undone["scene_positions_blake3"]
assert int(before["scene_motion_ticks"]) > 0 and before["scene_motion_positions"] == "2"
assert before["scene_motion_digest"] == kept["scene_positions_blake3"]
for name in ("placement-kept", "placement-redone", "placement-kept-retained", "placement-kept-reopened"):
    fields = captures[name]["fields"]
    assert fields["scene_authored_positions"] == "2"
    assert int(fields["scene_placement_steps"]) >= 3
    assert fields["scene_placement_steps"] == kept["scene_placement_steps"]
    assert fields["scene_positions_blake3"] == kept["scene_positions_blake3"]
    assert fields["scene_motion_ticks"] == fields["scene_motion_positions"] == "0"
assert undone["scene_motion_ticks"] == undone["scene_motion_positions"] == "0"
print(json.dumps(dict(schema="knot.explicit-kept-placement-windows/v1", base_source="9790938",
    evidence_root=str(root), binary=str(binary), binary_sha256=sha(binary.read_bytes()),
    source_bytes=len(source), source_sha256=sha(source), captures=len(captures),
    tests=["displayed preview becomes exact authored pose", "one Undo restores all prior placement",
        "Redo restores exact kept pose", "explicit retention survives fresh reopen",
        "runtime discarded", "source bytes unchanged", "slot and roles preserved"], runs=runs), indent=2))
