"""Independent stdlib Gemini receipt: SITE_FOLDER CERT_PEM PORT.

Uses the exported local certificate as the trust anchor; no shared Rust client
or parser. Requires exact saved bodies and a clean TLS close notification.
"""
import json
from pathlib import Path
import socket
import ssl
import sys


def verify(root: Path, certificate: Path, port: int):
    context = ssl.create_default_context(cafile=str(certificate))
    manifest = json.loads((root / "site.json").read_text(encoding="utf-8"))

    def exchange(path):
        with socket.create_connection(("127.0.0.1", port), timeout=5) as raw:
            with context.wrap_socket(raw, server_hostname="localhost", suppress_ragged_eofs=False) as tls:
                tls.sendall(f"gemini://localhost:{port}{path}\r\n".encode())
                chunks = []
                while chunk := tls.recv(65536):
                    chunks.append(chunk)
                header, body = b"".join(chunks).split(b"\r\n", 1)
                code, meta = header.decode().split(" ", 1)
                return code, meta, body, tls.version()

    for page in manifest["pages"]:
        path = "/" + page["path"]
        code, meta, body, version = exchange(path)
        assert code == "20", (path, code)
        assert meta.split(";")[0] == "text/gemini", meta
        assert body == (root / page["path"]).read_bytes(), path
        print(json.dumps({"path": path, "code": code, "meta": meta, "bytes": len(body), "tls": version, "clean_tls_eof": True}))
    assert exchange("/")[2] == (root / "index.gmi").read_bytes()
    for path in ["/site.json", "/unpublished.gmi", "/%2e%2e/secret.gmi"]:
        assert exchange(path)[0] == "51", path
    print(json.dumps({"unpublished_paths": "refused", "root_index": "exact"}))


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    verify(Path(sys.argv[1]), Path(sys.argv[2]), int(sys.argv[3]))
