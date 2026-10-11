# Copyright 2026 Mark Alan Boykin
# SPDX-License-Identifier: MPL-2.0
"""Read-only verification of this Windows consumer's marked native evidence."""
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
runs = []
authored = None
executable_sha256 = None
for index, name in enumerate(("kept-seed", "kept-wide-reopen", "kept-compact-reopen")):
    directory = root / name
    receipt = (directory / "scenario.done").read_text(encoding="utf-8")
    assert receipt.startswith("RESULT ok\n"), name
    assert "FAIL:" not in receipt, name
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
        assert fields["scene_authored_positions"] == "1"
        assert fields["scene_placement_steps"] == "2"
        assert fields["scene_motion_roles"] == "1"
        assert fields["scene_motion_slot"] == "true"
        authored = authored or fields["scene_positions_blake3"]
        assert fields["scene_positions_blake3"] == authored
        pixels = (directory / (match[1] + ".png")).read_bytes()
        assert pixels[:8] == b"\x89PNG\r\n\x1a\n"
        width, height = struct.unpack(">II", pixels[16:24])
        assert (width, height) == (item["width"], item["height"])
        assert width == (840 if index == 2 else 2200)
        captures[match[1]] = dict(width=width, height=height, png_sha256=sha(pixels),
            presentation_sequence=item["pairing"]["presentation"]["sequence"], fields=fields)
    assert len(captures) == (7, 2, 6)[index]
    suffix = "-compact" if index == 2 else ""
    if index != 1:
        started = captures["scene-motion-started" + suffix]["fields"]
        moving = captures["scene-motion-running" + suffix]["fields"]
        paused = captures["scene-motion-paused" + suffix]["fields"]
        stable = captures["scene-motion-paused-stable" + suffix]["fields"]
        assert moving["scene_motion_running"] == "true"
        assert int(moving["scene_motion_ticks"]) > int(started["scene_motion_ticks"])
        assert moving["scene_motion_digest"] != started["scene_motion_digest"]
        assert paused["scene_motion_running"] == stable["scene_motion_running"] == "false"
        assert paused["scene_motion_ticks"] == stable["scene_motion_ticks"]
        assert paused["scene_motion_digest"] == stable["scene_motion_digest"]
    if index:
        key = "relationship-live-dynamics-reopened" if index == 1 else "relationship-live-dynamics-compact-reopened"
        reopened = captures[key]["fields"]
        assert reopened["scene_motion_running"] == "false"
        assert reopened["scene_motion_ticks"] == reopened["scene_motion_positions"] == "0"
    else:
        revealed = captures["scene-motion-revealed-paused"]["fields"]
        retained = captures["relationship-live-dynamics-retained"]["fields"]
        assert revealed["scene_motion_running"] == retained["scene_motion_running"] == "false"
        assert revealed["scene_motion_ticks"] == retained["scene_motion_ticks"]
        assert revealed["scene_motion_digest"] == retained["scene_motion_digest"]
    presentation = re.search(r"^presentation: (\d+) frames, (\d+) redraws, (\d+) unpresented redraws$", receipt, re.M)
    assert presentation and int(presentation[3]) == 0
    identity = json.loads((directory / "binary.json").read_text(encoding="utf-8-sig"))
    assert identity["Algorithm"] == "SHA256"
    recorded_hash = identity["Hash"].lower()
    assert re.fullmatch(r"[0-9a-f]{64}", recorded_hash)
    executable_sha256 = executable_sha256 or recorded_hash
    assert recorded_hash == executable_sha256
    runs.append(dict(name=name, presented_frames=int(presentation[1]), redraws=int(presentation[2]),
        binary_sha256=recorded_hash, source_phase="keep-runtime",
        unpresented_redraws=int(presentation[3]), captures=captures))
print(json.dumps(dict(schema="knot.relationship-consumer-windows/v1", evidence_root=str(root),
    base_source="9790938",
    binary=str(binary), binary_sha256=executable_sha256, source_bytes=len(source), source_sha256=sha(source),
    authored_positions_blake3=authored, captures=sum(len(run["captures"]) for run in runs),
    tests=["motion advances", "pause freezes ticks and poses", "hide/reveal does not auto-resume",
        "fresh process discards runtime", "authored settings survive", "source bytes unchanged"], runs=runs), indent=2))
