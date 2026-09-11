# Copyright 2026 Mark Alan Boykin
# SPDX-License-Identifier: MPL-2.0
"""Verify a local Knot publication with an independently obtained Scroll client.

Obtain smolnet-portal ac926d2ab37b53443795439a7181e36416253909 separately;
install its Python dependencies in a disposable Python 3.13+ environment.
No upstream client implementation is copied into this repository.
Usage: python verify_scroll_client.py UPSTREAM_CHECKOUT SITE_FOLDER PORT
The site's saved bytes must be the revision currently published by Knot.
"""
import asyncio
from datetime import datetime
import json
from pathlib import Path
import sys


async def verify(upstream, site, port):
    sys.path.insert(0, str(upstream.resolve()))
    from geminiportal.protocols.base import ALLOWED_PORTS
    from geminiportal.protocols.scroll import ScrollRequest
    from geminiportal.urls import URLReference
    from geminiportal.utils import ProxyOptions

    # Configure the proxy client's port policy for an ephemeral loopback port.
    # Its request generation, TLS exchange, and response parsing stay unchanged.
    ALLOWED_PORTS.add(port)
    config = json.loads((site / "site.json").read_text(encoding="utf-8"))
    for page in config["pages"]:
        for abstract in (False, True):
            response = await ScrollRequest(
                URLReference(f"scroll://localhost:{port}/{page['path']}"),
                ProxyOptions(meta=abstract, lang="en"),
            ).fetch()
            body = await response.get_body()
            expected = (
                page["abstract_source"].encode("utf-8")
                if abstract else (site / page["path"]).read_bytes()
            )
            assert body == expected, (page["path"], abstract, "body mismatch")
            assert response.status == f"2{page['classification']}"
            assert response.document_meta.author == (page["author"] or None)
            assert response.lang == (page["language"] or None)
            assert response.mimetype == "text/scroll"
            for field, actual in (
                ("published", response.document_meta.publish_date),
                ("modified", response.document_meta.modification_date),
            ):
                expected_date = datetime.fromisoformat(page[field]) if page[field] else None
                assert actual == expected_date
            assert response.tls_close_notify_received
            print(json.dumps({
                "path": page["path"], "abstract": abstract,
                "status": response.status, "author": response.document_meta.author,
                "language": response.lang, "bytes": len(body), "tls": response.tls_version,
                "close_notify": response.tls_close_notify_received,
            }))


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    asyncio.run(verify(Path(sys.argv[1]), Path(sys.argv[2]), int(sys.argv[3])))
