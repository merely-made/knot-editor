#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
"""Fetch every page from a stock NomadNet Node over paired loopback UDP."""

import argparse
import importlib.metadata
import pathlib
import signal
import socket
import subprocess
import sys
import tempfile
import time


def free_udp_port():
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(("127.0.0.1", 0))
    port = sock.getsockname()[1]
    sock.close()
    return port


def config_text(listen_port, peer_port):
    return (
        "[reticulum]\n enable_transport = no\n share_instance = no\n\n"
        "[logging]\n loglevel = 2\n\n[interfaces]\n\n[[Loopback UDP]]\n"
        " type = UDPInterface\n enabled = yes\n"
        " listen_ip = 127.0.0.1\n"
        f" listen_port = {listen_port}\n"
        " forward_ip = 127.0.0.1\n"
        f" forward_port = {peer_port}\n"
    )


def client(args):
    import RNS

    RNS.Reticulum(configdir=args.rnsconfig)
    target_hash = bytes.fromhex(args.destination)
    identity = None
    deadline = time.monotonic() + args.timeout
    while time.monotonic() < deadline:
        identity = RNS.Identity.recall(target_hash)
        if identity is not None:
            break
        RNS.Transport.request_path(target_hash)
        time.sleep(0.1)
    if identity is None:
        raise RuntimeError("timed out waiting for the NomadNet Node announce")

    destination = RNS.Destination(
        identity, RNS.Destination.OUT, RNS.Destination.SINGLE,
        "nomadnetwork", "node",
    )
    state = {"link": None, "error": None}
    link_ready = False

    def on_link(link):
        nonlocal link_ready
        state["link"] = link
        link_ready = True

    RNS.Link(destination, on_link)
    link_deadline = time.monotonic() + args.timeout
    while not link_ready and time.monotonic() < link_deadline:
        time.sleep(0.05)
    if state["link"] is None:
        raise RuntimeError("timed out establishing a link to the NomadNet Node")

    for name in args.pages:
        expected = pathlib.Path(args.export, "pages", name).read_bytes()
        state["done"] = False
        state["error"] = None

        def on_response(receipt, expected=expected, name=name):
            body = receipt.response
            if not isinstance(body, (bytes, bytearray)):
                state["error"] = f"{name}: unexpected response type {type(body).__name__}"
            elif bytes(body) != expected:
                state["error"] = f"{name}: body differs from exported bytes"
            state["done"] = True

        state["link"].request(
            "/page/" + name, None, response_callback=on_response
        )
        request_deadline = time.monotonic() + args.timeout
        while not state["done"] and time.monotonic() < request_deadline:
            time.sleep(0.05)
        if not state["done"]:
            raise RuntimeError(f"timed out requesting /page/{name}")
        if state["error"]:
            raise RuntimeError(state["error"])
        print(f"byte-exact /page/{name} received from stock NomadNet Node")

    state["link"].teardown()


def stop_server(process):
    if process.poll() is None:
        process.send_signal(signal.SIGINT)
    try:
        return process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
        return process.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--export")
    parser.add_argument("--timeout", type=int, default=45)
    parser.add_argument("--client", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--rnsconfig", help=argparse.SUPPRESS)
    parser.add_argument("--destination", help=argparse.SUPPRESS)
    parser.add_argument("--pages", nargs="*", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.client:
        client(args)
        return
    if not args.export:
        parser.error("--export is required")

    export = pathlib.Path(args.export).resolve()
    pages_dir = export / "pages"
    names = sorted(path.name for path in pages_dir.iterdir() if path.is_file())
    if not names or "index.mu" not in names:
        raise RuntimeError("export must contain index.mu and at least one page")

    server_port, client_port = free_udp_port(), free_udp_port()
    while client_port == server_port:
        client_port = free_udp_port()

    with tempfile.TemporaryDirectory(prefix="knot-nomadnet-node-") as temp:
        base = pathlib.Path(temp)
        server_config = base / "nomad"
        server_rns = base / "server-rns"
        client_rns = base / "client-rns"
        for directory in (server_config, server_rns, client_rns):
            directory.mkdir(mode=0o700)
        (server_rns / "config").write_text(config_text(server_port, client_port))
        (client_rns / "config").write_text(config_text(client_port, server_port))
        storage_files = server_config / "storage" / "files"
        storage_files.mkdir(parents=True, mode=0o700)
        nomad_config = (
            "[node]\n enable_node = yes\n node_name = Knot export smoke\n"
            " disable_propagation = yes\n announce_at_start = yes\n"
            " announce_interval = 60\n page_refresh_interval = 0\n"
            f" pages_path = {pages_dir}\n files_path = {storage_files}\n"
        )
        (server_config / "config").write_text(nomad_config)

        command = [
            sys.executable, "-m", "nomadnet.nomadnet", "--daemon", "--console",
            "--config", str(server_config), "--rnsconfig", str(server_rns),
        ]
        log_path = base / "nomadnet.log"
        with log_path.open("wb") as log:
            server = subprocess.Popen(command, stdout=log, stderr=log)
            try:
                identity_path = server_config / "storage" / "identity"
                deadline = time.monotonic() + 20
                destination = None
                while time.monotonic() < deadline:
                    log.flush()
                    output = log_path.read_text(errors="replace")
                    if identity_path.is_file():
                        import RNS
                        identity = RNS.Identity.from_file(str(identity_path))
                        if identity is not None:
                            destination = RNS.Destination.hash_from_name_and_identity(
                                "nomadnetwork.node", identity
                            ).hex()
                            break
                    if server.poll() is not None:
                        raise RuntimeError("NomadNet exited during startup:\n" + output)
                    time.sleep(0.1)
                if destination is None:
                    raise RuntimeError("NomadNet Node identity did not appear:\n" + output)

                client_command = [
                    sys.executable, str(pathlib.Path(__file__).resolve()), "--client",
                    "--rnsconfig", str(client_rns), "--destination", destination,
                    "--export", str(export), "--pages", *names,
                    "--timeout", str(args.timeout),
                ]
                result = subprocess.run(
                    client_command, text=True, capture_output=True,
                    timeout=(args.timeout + 20) * (len(names) + 1), check=False,
                )
                if result.stdout:
                    print(result.stdout, end="")
                if result.returncode != 0:
                    raise RuntimeError(
                        "independent RNS client failed:\n" + result.stderr
                    )
            finally:
                code = stop_server(server)
            if code != 0:
                raise RuntimeError(
                    f"NomadNet did not stop cleanly (exit {code}):\n"
                    + log_path.read_text(errors="replace")
                )

    print(f"PASS: stock NomadNet {importlib.metadata.version('nomadnet')}, "
          "RNS loopback UDP, all exported page bytes exact")


if __name__ == "__main__":
    main()
