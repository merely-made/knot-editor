#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
"""Headless check of stock NomadNet Browser/MicronParser over loopback RNS."""

import argparse
import pathlib
import subprocess
import sys
import tempfile
import time

from loopback_smoke import free_udp_port, run_server, stop_server


def wait_for_page(browser, expected, timeout):
    from nomadnet.ui.textui.Browser import Browser

    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if browser.status == Browser.DONE and browser.page_data == expected:
            if browser.page_data != expected:
                raise RuntimeError(f"Browser page bytes mismatch at {browser.path}")
            return
        if browser.status in (Browser.REQUEST_FAILED, Browser.REQUEST_TIMEOUT, Browser.LINK_TIMEOUT):
            raise RuntimeError(f"NomadNet Browser failed at {browser.path}: status={browser.status_text()}")
        time.sleep(0.05)
    raise RuntimeError(f"NomadNet Browser timed out at {browser.path}")


def render_text(browser):
    canvas = browser.display_widget.render((100, 24), focus=False)
    return "\n".join(line.decode("utf-8", errors="replace") for line in canvas.text)


def navigate_browser(browser, target_path, visible_label):
    from nomadnet.ui.textui.MicronParser import LinkSpec, LinkableText

    if visible_label not in render_text(browser):
        raise RuntimeError(f"NomadNet did not render the {visible_label!r} navigation link")
    found = []

    def visit(widget):
        if isinstance(widget, (list, tuple)):
            for child in widget:
                visit(child)
        elif hasattr(widget, "original_widget"):
            visit(widget.original_widget)
        elif isinstance(widget, LinkableText):
            _text, parts = widget.get_text()
            for spec, _length in parts:
                if isinstance(spec, LinkSpec):
                    found.append((widget, spec))
        elif hasattr(widget, "contents"):
            visit(widget.contents)

    visit(browser.attr_maps)
    for widget, spec in found:
        if spec.link_target == ":" + target_path:
            # Use the actual Micron link object and target parsed from the page.
            widget.handle_link(spec.link_target, spec.link_fields)
            return
    raise RuntimeError(f"NomadNet MicronParser did not expose link target {':' + target_path!r}")


def browser_client(args):
    import RNS
    import urwid
    import nomadnet
    from nomadnet.ui.TextUI import THEMES, THEME_DARK
    from nomadnet.ui.textui.Browser import Browser

    class Loop:
        def __init__(self, screen):
            self.screen = screen

        def set_alarm_in(self, *_args, **_kwargs):
            return None

    class Directory:
        def simplest_display_str(self, destination_hash):
            return RNS.prettyhexrep(destination_hash)

        def should_identify_on_connect(self, _destination_hash):
            return False

    class App:
        pass

    config_root = pathlib.Path(args.config_dir)
    cache = config_root / "cache"
    cache.mkdir(parents=True, exist_ok=True)
    RNS.Reticulum(args.rns_config)
    screen = urwid.raw_display.Screen()
    screen.register_palette(THEMES[THEME_DARK]["urwid_theme"])
    app = App()
    app.config = {"textui": {"theme": THEME_DARK, "sanitize_names": True}}
    app.cachepath = str(cache)
    app.pagespath = ""
    app.filespath = ""
    app.downloads_path = str(config_root)
    app.identity = None
    app.node = None
    app.directory = Directory()
    app.ui = App()
    app.ui.glyphs = {
        "arrow_l": "<", "arrow_r": ">", "arrow_d": "v", "node": "N",
        "page": "P", "speed": "S", "divider1": "-", "copy": "[C]",
    }
    app.ui.screen = screen
    app.ui.colormode = 16
    app.ui.loop = Loop(screen)
    nomadnet.NomadNetworkApp._shared_instance = app
    delegate = App()
    delegate.given_list_width = 20

    snapshot = pathlib.Path(args.snapshot)
    destination = bytes.fromhex(args.destination)
    expected_pages = {
        "index.mu": (snapshot / "pages" / "index.mu").read_bytes(),
        "about.mu": (snapshot / "pages" / "about.mu").read_bytes(),
        "notes.mu": (snapshot / "pages" / "notes.mu").read_bytes(),
    }
    browser = Browser(app, "nomadnetwork", "node", destination_hash=destination, path="/page/index.mu", delegate=delegate)
    wait_for_page(browser, expected_pages["index.mu"], args.timeout)
    if "Knot mesh test" not in render_text(browser):
        raise RuntimeError("NomadNet's rendered index widget did not contain its title")
    print("NomadNet Browser rendered /page/index.mu")

    navigate_browser(browser, "/page/about.mu", "About")
    wait_for_page(browser, expected_pages["about.mu"], args.timeout)
    if "About" not in render_text(browser):
        raise RuntimeError("NomadNet's rendered About widget did not contain its heading")
    print("NomadNet Browser followed the rendered About link")

    navigate_browser(browser, "/page/index.mu", "Home")
    wait_for_page(browser, expected_pages["index.mu"], args.timeout)
    navigate_browser(browser, "/page/notes.mu", "Notes")
    wait_for_page(browser, expected_pages["notes.mu"], args.timeout)
    if "Notes" not in render_text(browser):
        raise RuntimeError("NomadNet's rendered Notes widget did not contain its heading")
    print("NomadNet Browser followed the rendered Notes link")

    # This upstream CLI registers only known paths; an unknown path has no
    # handler and may leave the stock Browser request pending. Do not claim a
    # Not Found document or include this non-deterministic case in the smoke.


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--launcher")
    parser.add_argument("--reticulum-go")
    parser.add_argument("--snapshot")
    parser.add_argument("--python", default=sys.executable, help="isolated Python with nomadnet installed")
    parser.add_argument("--timeout", type=int, default=45)
    parser.add_argument("--client", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--rns-config")
    parser.add_argument("--config-dir")
    parser.add_argument("--destination")
    args = parser.parse_args()
    if args.client:
        browser_client(args)
        return
    if not args.launcher or not args.reticulum_go or not args.snapshot:
        parser.error("--launcher, --reticulum-go, and --snapshot are required")

    snapshot = pathlib.Path(args.snapshot).resolve()
    for page in ("index.mu", "about.mu", "notes.mu"):
        if not (snapshot / "pages" / page).is_file():
            raise RuntimeError(f"smoke fixture requires pages/{page}")
    go_port, py_port = free_udp_port(), free_udp_port()
    while py_port == go_port:
        py_port = free_udp_port()

    with tempfile.TemporaryDirectory(prefix="knot-nomadnet-browser-") as temporary:
        base = pathlib.Path(temporary)
        config = base / "reticulum.conf"
        rns_config = base / "python-rns"
        rns_config.mkdir(mode=0o700)
        (rns_config / "config").write_text(
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
        (base / "nomadnet").mkdir(mode=0o700)
        process, log, destination = run_server(
            args.launcher, args.reticulum_go, str(snapshot), str(config),
            str(base / "reticulum-identity"), str(base / "server.log"),
        )
        try:
            command = [
                args.python, __file__, "--client", "--snapshot", str(snapshot),
                "--rns-config", str(rns_config), "--config-dir", str(base / "nomadnet"),
                "--destination", destination, "--timeout", str(args.timeout),
            ]
            result = subprocess.run(command, text=True, capture_output=True, timeout=args.timeout * 5)
            if result.returncode != 0:
                raise RuntimeError(f"stock NomadNet Browser failed:\n{result.stdout}\n{result.stderr}")
            print(result.stdout.strip())
        finally:
            code = stop_server(process)
            log.close()
        if code != 0:
            raise RuntimeError("Reticulum-Go launcher did not stop cleanly after SIGTERM")
    print("PASS: stock NomadNet Browser/MicronParser rendered and navigated loopback pages")


if __name__ == "__main__":
    main()
