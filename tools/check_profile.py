#!/usr/bin/env python3
"""Check which engines a Knot build profile links.

Reads `cargo metadata` for a package at a feature selection, walks its normal
(non-dev, non-build) dependency edges, and reports the crates found against a
named set. The offline profile is `knot-desktop --no-default-features`: no peer
transport, no HTTP client, no Reticulum, no p2p stack may be reachable from it.

    tools/check_profile.py offline
    tools/check_profile.py offline --features retinue --expect retinue
    tools/check_profile.py offline --metadata saved.json

`--expect NAME` is the positive control: it checks only that NAME is reachable,
so CI knows the walker sees an optional engine when the feature that links it
is on.
"""

import argparse
import collections
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Network engines an offline Knot must not link. Grouped by the contract each
# would sit behind, so a failure says what kind of link crept in.
FORBIDDEN = {
    "peer transport (mere-transport)": [
        "mere-transport",
        "mere-p2panda-net",
        "iroh",
        "iroh-blobs",
        "iroh-gossip",
        "quinn",
        "snow",
        "mere-webrtc-carrier",
        "stickleback",
    ],
    "Reticulum and mesh engines": [
        "retinue",
        "tulle",
        "radio-hand",
        "outrider",
        "sennet",
        "tucket",
        "murm-i2p",
        "emissary",
    ],
    "HTTP fetch (netfetcher)": [
        "netfetcher",
        "mere-fetch",
        "reqwest",
        "hyper",
    ],
    "anonymity networks": ["arti-client", "arti"],
}

PROFILES = {
    "offline": {
        "package": "knot-desktop",
        "manifest": "apps/desktop/Cargo.toml",
        "args": ["--no-default-features"],
    },
    # The product default (`mere-retention`), for reading what it carries. It is
    # expected to fail the forbidden check until knot-editor's peer modules sit
    # behind a feature.
    "default": {
        "package": "knot-desktop",
        "manifest": "apps/desktop/Cargo.toml",
        "args": [],
    },
}

# Linked by the offline profile today and not forbidden: the loopback site
# server in knot-site (Gemini and Scroll over rustls) and errand's grammar,
# which drags the smolweb protocol clients along with it. Printed so a reader
# sees what the profile still carries.
KNOWN_INCLUSIONS = ["gemini-protocol", "spartan-protocol", "rustls", "errand"]


def load_metadata(profile, features, saved):
    if saved:
        return json.loads(Path(saved).read_text(encoding="utf-8"))
    command = [
        "cargo",
        "metadata",
        "--format-version",
        "1",
        "--manifest-path",
        str(ROOT / profile["manifest"]),
        *profile["args"],
    ]
    if features:
        command += ["--features", features]
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        sys.exit(f"cargo metadata failed:\n{result.stderr}")
    return json.loads(result.stdout)


def reachable(metadata, root_name):
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    roots = [i for i, p in packages.items() if p["name"] == root_name and p["source"] is None]
    if not roots:
        sys.exit(f"package {root_name} is not a workspace member")
    parent = {roots[0]: None}
    queue = collections.deque(roots)
    while queue:
        current = queue.popleft()
        for dep in nodes[current]["deps"]:
            if not any(kind["kind"] is None for kind in dep["dep_kinds"]):
                continue
            if dep["pkg"] not in parent:
                parent[dep["pkg"]] = current
                queue.append(dep["pkg"])
    by_name = collections.defaultdict(list)
    for identifier in parent:
        by_name[packages[identifier]["name"]].append(identifier)

    def path(name):
        chain, identifier = [], by_name[name][0]
        while identifier is not None:
            chain.append(packages[identifier]["name"])
            identifier = parent[identifier]
        return " <- ".join(chain)

    return by_name, path


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("profile", choices=sorted(PROFILES))
    parser.add_argument("--features", help="extra features to enable on the package")
    parser.add_argument("--expect", action="append", default=[], help="crate that must be linked")
    parser.add_argument("--metadata", help="analyse saved `cargo metadata` output")
    args = parser.parse_args()

    profile = PROFILES[args.profile]
    metadata = load_metadata(profile, args.features, args.metadata)
    by_name, path = reachable(metadata, profile["package"])
    print(f"{profile['package']} {' '.join(profile['args'])}"
          f"{' --features ' + args.features if args.features else ''}: "
          f"{sum(len(v) for v in by_name.values())} packages linked")

    failed = False
    for name in args.expect:
        if name in by_name:
            print(f"  expected {name}: linked ({path(name)})")
        else:
            print(f"  expected {name}: NOT linked")
            failed = True
    if not args.expect:
        for contract, names in FORBIDDEN.items():
            for name in names:
                if name in by_name:
                    print(f"  FORBIDDEN {name} [{contract}]: {path(name)}")
                    failed = True
    carried = [name for name in KNOWN_INCLUSIONS if name in by_name]
    if carried:
        print("  still carried: " + ", ".join(carried))
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
