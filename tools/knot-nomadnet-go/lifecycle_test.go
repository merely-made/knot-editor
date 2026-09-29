// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

package main

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

func TestServeStagesPrivateCopyAndCleansItAfterChildExit(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("shell mock requires Unix")
	}
	root := fixture(t)
	parent := t.TempDir()
	config, identity := filepath.Join(parent, "config"), filepath.Join(parent, "identity")
	if err := os.WriteFile(config, []byte("explicit test config"), 0600); err != nil {
		t.Fatal(err)
	}
	ready, release := filepath.Join(parent, "ready"), filepath.Join(parent, "release")
	staged, captured := filepath.Join(parent, "staged"), filepath.Join(parent, "captured")
	mock := filepath.Join(parent, "reticulum-go")
	script := "#!/bin/sh\n" +
		"set -eu\n" +
		"[ \"$1\" = pageserver ]; shift\n" +
		"pages=; files=\n" +
		"while [ \"$#\" -gt 0 ]; do\n" +
		"  case \"$1\" in\n" +
		"    --pages-dir) pages=$2; shift 2 ;;\n" +
		"    --files-dir) files=$2; shift 2 ;;\n" +
		"    --announce-interval|--page-refresh|--file-refresh) [ \"$2\" = 0 ]; shift 2 ;;\n" +
		"    --no-page-stats) shift ;;\n" +
		"    --config|--identity|--node-name) shift 2 ;;\n" +
		"    *) exit 21 ;;\n" +
		"  esac\n" +
		"done\n" +
		"test -f \"$pages/index.mu\"; test -z \"$(ls -A \"$files\")\"\n" +
		"cat \"$pages/index.mu\" > \"$KNOT_CAPTURE\"\n" +
		"printf '%s' \"$pages\" > \"$KNOT_STAGED\"\n" +
		"printf ready > \"$KNOT_READY\"\n" +
		"while [ ! -f \"$KNOT_RELEASE\" ]; do sleep 0.02; done\n" +
		"cmp \"$pages/index.mu\" \"$KNOT_CAPTURE\"\n"
	if err := os.WriteFile(mock, []byte(script), 0700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("KNOT_READY", ready)
	t.Setenv("KNOT_RELEASE", release)
	t.Setenv("KNOT_STAGED", staged)
	t.Setenv("KNOT_CAPTURE", captured)
	done := make(chan error, 1)
	go func() {
		done <- runServe([]string{"--snapshot", root, "--config", config, "--identity", identity, "--reticulum-go", mock})
	}()
	deadline := time.After(4 * time.Second)
	for {
		if _, err := os.Stat(ready); err == nil {
			break
		}
		select {
		case err := <-done:
			t.Fatalf("mock server exited before ready: %v", err)
		case <-deadline:
			t.Fatal("mock server did not start")
		case <-time.After(10 * time.Millisecond):
		}
	}
	stagePath, err := os.ReadFile(staged)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "pages", "index.mu"), []byte("changed"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(release, nil, 0600); err != nil {
		t.Fatal(err)
	}
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(4 * time.Second):
		t.Fatal("mock server did not stop")
	}
	if _, err := os.Stat(string(stagePath)); !os.IsNotExist(err) {
		t.Fatalf("private stage still exists after child exit: %s", stagePath)
	}
}

func TestServePropagatesChildFailure(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("shell mock requires Unix")
	}
	root := fixture(t)
	parent := t.TempDir()
	config, mock := filepath.Join(parent, "config"), filepath.Join(parent, "reticulum-go")
	if err := os.WriteFile(config, nil, 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(mock, []byte("#!/bin/sh\nexit 7\n"), 0700); err != nil {
		t.Fatal(err)
	}
	err := runServe([]string{"--snapshot", root, "--config", config, "--identity", filepath.Join(parent, "identity"), "--reticulum-go", mock})
	if err == nil || !strings.Contains(err.Error(), "exit status 7") {
		t.Fatalf("runServe error = %v, want propagated child exit status", err)
	}
}
