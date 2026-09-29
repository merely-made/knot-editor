# Reticulum-Go page-server launcher

`knot-nomadnet-go` validates a Knot Micron export and can run an external
Reticulum-Go pageserver against a private copy of its saved page bytes. It is a
small Go command with no non-standard-library dependencies. It does not
implement Reticulum or NomadNet request handling, and it does not install or
bundle the upstream executable.

Build and validate an export:

```sh
cd tools/knot-nomadnet-go
go test ./...
go build -o knot-nomadnet-go .
./knot-nomadnet-go validate --snapshot /path/to/export
```

Start serving with an existing Reticulum-Go config and an explicit durable
identity path outside the export:

```sh
./knot-nomadnet-go serve \
  --snapshot /path/to/export \
  --config /path/to/reticulum-go/config \
  --identity /path/to/reticulum-go/storage/identities/knot-pageserver \
  --reticulum-go /path/to/reticulum-go
```

The identity file may be new; Reticulum-Go creates it on first start. Its
parent directory must already exist. Reuse that same file on later starts to
keep the NomadNet destination stable. The launcher does not choose a default
config or identity, edit Reticulum configuration, or create network
interfaces. Starting the server sends the upstream initial announce over the
interfaces in the supplied config. The launcher sets periodic announces to
zero, disables page statistics, and disables rescans. Upstream still sends its
initial announce when serving starts.

The export must contain only `snapshot.json` and `pages/`. The snapshot must be
version 1 Micron with 1–64 safe `.mu` filenames and `/index.mu`. Source bytes
must be JSON integer arrays, valid UTF-8, at most 1 MiB per page, and match the
corresponding non-executable regular file byte for byte. The total source plus
abstract data is limited to 16 MiB and the manifest to 24 MiB. Symlinks,
executable pages, missing or unlisted files, and extra export-root entries are
rejected. The server reads a private temporary copy of the validated files and
an empty private `files/` directory; the launcher removes both when the server
exits.

The Go checks cover the fields and files needed for static serving. Full
canonical `PublishedSnapshotV1` validation belongs to the Rust exporter; this
helper does not duplicate its language-tag/date validation or authenticate
the snapshot. Generate exports through Knot's exporter before serving them.

## Checks

The unit tests need only Go. The optional interoperability check needs Python
with the independent RNS package (`python -m pip install 'rns==1.5.4'`), an
installed Reticulum-Go executable, and a built launcher. It creates private
configs with one UDP interface on `127.0.0.1`, fetches `/page/index.mu`, checks
the complete response against the exported file bytes, shuts down by signal,
then restarts with the same identity and checks that the destination hash is
unchanged:

```sh
python tools/knot-nomadnet-go/loopback_smoke.py \
  --launcher /path/to/knot-nomadnet-go \
  --reticulum-go /path/to/reticulum-go \
  --snapshot /path/to/export \
  --python /path/to/python-with-rns
```

The recorded Linux amd64 run used Reticulum-Go v1.3.2 (commit
`b0f46f88f09a`), whose release binary SHA-256 was
`10e82e43fff5bf2e5272463be12007b673b2d29d73c7633cdcbd578326334f00`, Python
RNS 1.5.4, and Go 1.27.1. The loopback fetch matched the full saved page body
on two starts and kept the same destination hash. This verifies the static
page request path with those versions; it does not claim full NomadNet UI
parity or compatibility for other RNS releases.

Two additional checks use stock NomadNet 1.2.0 installed in an isolated Python
environment (`python -m pip install 'nomadnet==1.2.0' 'rns==1.5.4'`). From the
repository root, use the included fixture:

```sh
python tools/knot-nomadnet-go/nomadnet_node_smoke.py \
  --export tools/knot-nomadnet-go/testdata/three-pages --timeout 25
python tools/knot-nomadnet-go/nomadnet_browser_smoke.py \
  --launcher /path/to/knot-nomadnet-go \
  --reticulum-go /path/to/reticulum-go \
  --snapshot tools/knot-nomadnet-go/testdata/three-pages \
  --python /path/to/python-with-nomadnet
```

The node check starts the real `nomadnet --daemon` CLI and fetches all three
pages with an independent RNS client. The browser check uses stock Browser,
MicronParser and urwid widgets with a minimal headless application scaffold;
it verifies exact page bytes, rendered headings and activation of parsed Micron
links. It does not exercise a full-screen terminal session. Both checks use
only paired UDP interfaces on `127.0.0.1`.

The three-page checks passed. A separate attempt to request an unknown page
did not yield a rendered “Page Not Found” response within the test window;
missing-page behavior remains an acceptance gap, outside the passing smoke.

Reticulum-Go is an external runtime under its own Reticulum License. A product
that distributes its binary must review that license and preserve its notices.
