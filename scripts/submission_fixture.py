"""One-shot independent localhost Titan/Spartan receiver for headed testing.

Usage: submission_fixture.py titan|spartan OUTPUT_DIRECTORY [--port PORT]
       [--reply-body TEXT] [--expect-tls-refusal]
Requires cryptography for Titan only. Writes receipt.json and body.bin after
one request; returns a redirect without serving its destination. Never uses
the shared Rust client or server. All generated keys are fixture-local.
"""
from datetime import datetime, timedelta, timezone
import argparse
import hashlib
import json
from pathlib import Path
import socket
import ssl


def run(protocol, root, port=0, reply_body=None, expect_tls_refusal=False):
    root.mkdir(parents=True, exist_ok=False)
    context = None
    if protocol == "titan":
        from cryptography import x509
        from cryptography.hazmat.primitives import hashes, serialization
        from cryptography.hazmat.primitives.asymmetric import rsa
        from cryptography.x509.oid import NameOID
        key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
        name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "localhost")])
        now = datetime.now(timezone.utc)
        cert = (x509.CertificateBuilder().subject_name(name).issuer_name(name)
                .public_key(key.public_key()).serial_number(x509.random_serial_number())
                .not_valid_before(now - timedelta(minutes=1)).not_valid_after(now + timedelta(days=1))
                .add_extension(x509.SubjectAlternativeName([x509.DNSName("localhost")]), critical=False)
                .sign(key, hashes.SHA256()))
        (root / "key.pem").write_bytes(key.private_bytes(serialization.Encoding.PEM,
            serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
        (root / "cert.pem").write_bytes(cert.public_bytes(serialization.Encoding.PEM))
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(root / "cert.pem", root / "key.pem")
    with socket.socket() as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(("127.0.0.1", port))
        listener.listen(1)
        listener.settimeout(600)
        port = listener.getsockname()[1]
        print(f"{protocol}://localhost:{port}/upload", flush=True)
        socket_, _ = listener.accept()
        socket_.settimeout(10)
        try:
            stream = context.wrap_socket(socket_, server_side=True) if context else socket_
        except (ssl.SSLError, ConnectionResetError) as error:
            socket_.close()
            if not expect_tls_refusal:
                raise
            receipt = {"protocol": protocol, "tls_refused": True,
                       "application_bytes": 0, "error": str(error)}
            (root / "receipt.json").write_text(json.dumps(receipt, indent=2))
            print(json.dumps(receipt), flush=True)
            return
        with stream:
            header = bytearray()
            while not header.endswith(b"\r\n"):
                chunk = stream.recv(1)
                if not chunk and expect_tls_refusal and not header:
                    receipt = {"protocol": protocol, "tls_refused": True,
                               "application_bytes": 0, "closed_before_request": True}
                    (root / "receipt.json").write_text(json.dumps(receipt, indent=2))
                    print(json.dumps(receipt), flush=True)
                    return
                assert chunk and len(header) < 4096
                header.extend(chunk)
            assert not expect_tls_refusal, "Client sent application bytes despite certificate change"
            fields = header[:-2].decode()
            if protocol == "titan":
                target, *parameters = fields.split(";")
                params = dict(field.split("=", 1) for field in parameters)
                size = int(params["size"])
                assert target == f"titan://localhost:{port}/upload"
                receipt = {"protocol": protocol, "target": target, "mime": params.get("mime"), "has_token": "token" in params}
            else:
                host, path, length = fields.split(" ")
                assert host == "localhost" and path == "/upload"
                size = int(length)
                receipt = {"protocol": protocol, "host": host, "path": path}
            assert 0 <= size <= 1024 * 1024
            body = bytearray()
            while len(body) < size:
                chunk = stream.recv(size - len(body))
                assert chunk
                body.extend(chunk)
            (root / "body.bin").write_bytes(body)
            receipt.update(bytes=len(body), sha256=hashlib.sha256(body).hexdigest(), requests=1)
            (root / "receipt.json").write_text(json.dumps(receipt, indent=2))
            if reply_body is None:
                response = b"30 gemini://localhost/receipt\r\n" if protocol == "titan" else b"3 /receipt\r\n"
            else:
                response = (b"20 text/plain\r\n" if protocol == "titan" else b"2 text/plain\r\n") + reply_body.encode()
            stream.sendall(response)
            if context:
                # Independent client should finish TLS cleanly as well.
                try:
                    stream.unwrap().close()
                except (ssl.SSLError, OSError):
                    pass
            print(json.dumps(receipt), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("protocol", choices=["titan", "spartan"])
    parser.add_argument("output_directory", type=Path)
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--reply-body")
    parser.add_argument("--expect-tls-refusal", action="store_true")
    args = parser.parse_args()
    if args.expect_tls_refusal and args.protocol != "titan":
        parser.error("TLS refusal requires Titan")
    run(args.protocol, args.output_directory, args.port, args.reply_body, args.expect_tls_refusal)
