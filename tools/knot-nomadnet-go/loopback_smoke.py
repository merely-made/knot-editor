#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
"""Optional Python RNS interoperability check, isolated to UDP loopback."""

import argparse
import pathlib
import re
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


def client(args):
    import RNS

    RNS.Reticulum(args.config_dir)
    target_hash = bytes.fromhex(args.destination)
    expected = pathlib.Path(args.expected).read_bytes()
    deadline = time.monotonic() + args.timeout
    destination = None
    while time.monotonic() < deadline:
        identity = RNS.Identity.recall(target_hash)
        if identity is not None:
            destination = RNS.Destination(
                identity, RNS.Destination.OUT, RNS.Destination.SINGLE,
                "nomadnetwork", "node",
            )
            break
        RNS.Transport.request_path(target_hash)
        time.sleep(0.12)
    if destination is None:
        raise RuntimeError("timed out waiting for the pageserver announce")

    result = {"done": False, "error": None}

    def on_response(receipt):
        response = receipt.response
        if not isinstance(response, (bytes, bytearray)):
            result["error"] = f"unexpected response type: {type(response).__name__}"
        elif bytes(response) != expected:
            result["error"] = f"response differs from exported source ({len(response)} bytes)"
        result["done"] = True

    def on_link(link):
        link.request("/page/index.mu", None, response_callback=on_response)

    RNS.Link(destination, on_link)
    while time.monotonic() < deadline:
        if result["done"]:
            if result["error"]:
                raise RuntimeError(result["error"])
            print("byte-exact /page/index.mu response received via Python RNS")
            return
        time.sleep(0.1)
    raise RuntimeError("timed out waiting for the NomadNet page response")


def stop_server(process):
    if process.poll() is None:
        process.send_signal(signal.SIGTERM)
    try:
        return process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
        return process.wait(timeout=5)


def run_server(launcher, upstream, snapshot, config, identity, log_path):
    log = open(log_path, "wb")
    process = subprocess.Popen(
        [launcher, "serve", "--snapshot", snapshot, "--config", config,
         "--identity", identity, "--reticulum-go", upstream],
        stdout=log, stderr=log,
    )
    deadline = time.monotonic() + 20
    try:
        while time.monotonic() < deadline:
            log.flush()
            output = pathlib.Path(log_path).read_text(errors="replace")
            match = re.search(r"pageserver node destination hash:\s*([0-9a-f]{32})", output)
            if match:
                return process, log, match.group(1)
            if process.poll() is not None:
                raise RuntimeError("launcher exited early:\n" + output)
            time.sleep(0.1)
        raise RuntimeError("pageserver startup timed out:\n" + pathlib.Path(log_path).read_text(errors="replace"))
    except Exception:
        stop_server(process)
        log.close()
        raise


def fetch(python, expected, destination, py_config_dir, timeout):
    command = [python, __file__, "--client", "--expected", expected,
               "--destination", destination, "--config-dir", py_config_dir,
               "--timeout", str(timeout)]
    result = subprocess.run(command, text=True, capture_output=True, timeout=timeout + 5)
    if result.returncode != 0:
        raise RuntimeError(f"Python RNS client failed:\n{result.stdout}\n{result.stderr}")
    print(result.stdout.strip())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--launcher")
    parser.add_argument("--reticulum-go")
    parser.add_argument("--snapshot")
    parser.add_argument("--python", default=sys.executable, help="Python with RNS installed")
    parser.add_argument("--timeout", type=int, default=45)
    parser.add_argument("--client", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--expected")
    parser.add_argument("--destination")
    parser.add_argument("--config-dir")
    args = parser.parse_args()
    if args.client:
        client(args)
        return
    if not args.launcher or not args.reticulum_go or not args.snapshot:
        parser.error("--launcher, --reticulum-go, and --snapshot are required")

    snapshot = pathlib.Path(args.snapshot).resolve()
    expected = snapshot / "pages" / "index.mu"
    if not expected.is_file():
        raise RuntimeError("snapshot has no pages/index.mu")
    go_port = free_udp_port()
    py_port = free_udp_port()
    while py_port == go_port:
        py_port = free_udp_port()
    with tempfile.TemporaryDirectory(prefix="knot-nomadnet-rns-") as temp:
        base = pathlib.Path(temp)
        config = base / "reticulum.conf"
        identity = base / "reticulum-identity"
        py_config = base / "python-rns"
        py_config.mkdir(mode=0o700)
        (py_config / "config").write_text(
            "[reticulum]\n enable_transport = no\n share_instance = no\n loglevel = 2\n"
            "\n[interfaces]\n\n[[Loopback UDP]]\n type = UDPInterface\n enabled = yes\n"
            " interface_enabled = yes\n listen_ip = 127.0.0.1\n"
            f" listen_port = {py_port}\n forward_ip = 127.0.0.1\n forward_port = {go_port}\n"
        )
        config.write_text(
            "[reticulum]\n enable_transport = no\n share_instance = no\n"
            " enable_sandbox = no\n enable_seccomp = no\n\n[logging]\n loglevel = 2\n"
            "\n[interfaces]\n\n[[Loopback UDP]]\n type = UDPInterface\n enabled = yes\n"
            f" address = 127.0.0.1:{go_port}\n target_host = 127.0.0.1\n target_port = {py_port}\n"
        )
        first, first_log, first_hash = run_server(
            args.launcher, args.reticulum_go, str(snapshot), str(config), str(identity), str(base / "server1.log")
        )
        try:
            print("server 1 destination:", first_hash)
            fetch(args.python, str(expected), first_hash, str(py_config), args.timeout)
        finally:
            code = stop_server(first)
            first_log.close()
        if code != 0:
            raise RuntimeError("launcher did not shut down cleanly after SIGTERM")

        second, second_log, second_hash = run_server(
            args.launcher, args.reticulum_go, str(snapshot), str(config), str(identity), str(base / "server2.log")
        )
        try:
            print("server 2 destination:", second_hash)
            if first_hash != second_hash:
                raise RuntimeError("destination changed across restart with persistent identity")
            fetch(args.python, str(expected), second_hash, str(py_config), args.timeout)
        finally:
            code = stop_server(second)
            second_log.close()
        if code != 0:
            raise RuntimeError("launcher did not shut down cleanly after SIGTERM")
    print("PASS: loopback-only request, exact bytes, persistent identity, signal shutdown")


if __name__ == "__main__":
    main()
