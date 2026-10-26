#!/usr/bin/env python3
"""Check which engines a Knot build profile links.

Reads the dependency graph of one package at a feature selection, over its
normal (non-dev, non-build) edges, and reports the crates found against a named
set. The live reading is `cargo tree -p <package> -e normal`, which resolves
features for that package alone. `cargo metadata` does not: it unifies features
across the whole workspace, dev-dependencies included, so a dev-dependency that
turns `stickleback/net` on for its own tests would show a network edge under a
library that never links one. `--metadata` still reads a saved metadata file.

The graph is read, never built. The offline profile is `knot-desktop --no-default-features`: no peer
transport, no HTTP client, no Reticulum, no p2p stack may be reachable from it.
The library profile is `knot-editor` at its default features, which leave `peers`
off: the library alone links no iroh, quinn, p2panda-net or reqwest.

    tools/check_profile.py offline
    tools/check_profile.py offline --features retinue --expect retinue
    tools/check_profile.py library
    tools/check_profile.py library --features peers --expect mere-p2panda-net
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
# would sit behind, so a failure says what kind of link crept in. A crate that
# is renamed needs its new name listed beside the old one, or the check passes
# without seeing it.
FORBIDDEN = {
    "peer transport (mere-transport)": [
        "mere-transport",
        "mere-p2panda-net",
        "p2panda-sync",
        "p2panda-discovery",
        "iroh",
        "iroh-base",
        "iroh-blobs",
        "iroh-gossip",
        "iroh-relay",
        "quinn",
        "quinn-proto",
        "quinn-udp",
        "h3-quinn",
        "snow",
        "mere-webrtc-carrier",
        "murm-webrtc",  # mere-webrtc-carrier's name after the 2026-10-25 rename
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
    # The product default (`mere-retention`): knot-editor without `peers`, so the
    # desktop with its resident retention is offline too. Stickleback is allowed
    # for the reason given under `library`.
    "default": {
        "package": "knot-desktop",
        "manifest": "apps/desktop/Cargo.toml",
        "args": [],
        "allow": ["stickleback"],
    },
    # The library at its own defaults. `peers` is off, so mark, publish, resident
    # sync and the iroh blob store are not compiled. Stickleback is allowed: with
    # its `net` feature off it is the local half (store, causal index, group
    # crypto), which holds no p2panda-net. What must stay out is the network
    # itself, and the p2panda crates that carry it.
    "library": {
        "package": "knot-editor",
        "manifest": "crates/knot-editor/Cargo.toml",
        "args": [],
        "allow": ["stickleback"],
    },
}

# Linked by the offline profile today and not forbidden: the loopback site
# server in knot-site (Gemini and Scroll over rustls) and errand's grammar,
# which drags the smolweb protocol clients along with it. Printed so a reader
# sees what the profile still carries.
KNOWN_INCLUSIONS = [
    "gemini-protocol",
    "spartan-protocol",
    "rustls",
    "errand",
    # The signed-operation, local-store and group-crypto crates stickleback and
    # knot-editor build on. They hold no sockets; p2panda-net does, and is
    # forbidden above.
    "p2panda-core",
    "p2panda-store",
    "p2panda-encryption",
    "stickleback",
]


def load_metadata(saved):
    return json.loads(Path(saved).read_text(encoding="utf-8"))


def cargo_tree(profile, features, *extra):
    command = [
        "cargo",
        "tree",
        "--manifest-path",
        str(ROOT / profile["manifest"]),
        "--package",
        profile["package"],
        "--edges",
        "normal",
        "--target",
        "all",
        "--format",
        "{p}",
        *profile["args"],
        *extra,
    ]
    if features:
        command += ["--features", features]
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        sys.exit(f"cargo tree failed:\n{result.stderr}")
    return result.stdout.splitlines()


def tree_reachable(profile, features):
    """The linked crate names, and a path-to-root explainer, from `cargo tree`."""
    by_name = collections.defaultdict(list)
    for line in cargo_tree(profile, features, "--prefix", "none"):
        if line.strip() and not line.endswith("(*)") and line not in by_name[line.split()[0]]:
            by_name[line.split()[0]].append(line)

    def path(name):
        chain = []
        for line in cargo_tree(profile, features, "--invert", name, "--prefix", "depth"):
            depth = int(line[: len(line) - len(line.lstrip("0123456789"))])
            if depth != len(chain):
                break
            chain.append(line.lstrip("0123456789").split()[0])
        return " <- ".join(chain)

    return by_name, path


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
    if args.metadata:
        by_name, path = reachable(load_metadata(args.metadata), profile["package"])
    else:
        by_name, path = tree_reachable(profile, args.features)
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
                if name in profile.get("allow", []):
                    continue
                if name in by_name:
                    print(f"  FORBIDDEN {name} [{contract}]: {path(name)}")
                    failed = True
    carried = [name for name in KNOWN_INCLUSIONS if name in by_name]
    if carried:
        print("  still carried: " + ", ".join(carried))
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
