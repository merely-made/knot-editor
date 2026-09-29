// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func fixture(t *testing.T) string {
	t.Helper()
	root := t.TempDir()
	if err := os.Mkdir(filepath.Join(root, "pages"), 0700); err != nil {
		t.Fatal(err)
	}
	manifest := []byte(`{"version":1,"format":"micron","pages":{"/index.mu":{"metadata":{"path":"index.mu","author":"A","language":"en","classification":0,"published":"","modified":"","abstract_source":"Title"},"source":[65,0,66]}}}`)
	if err := os.WriteFile(filepath.Join(root, "snapshot.json"), manifest, 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "pages", "index.mu"), []byte{'A', 0, 'B'}, 0600); err != nil {
		t.Fatal(err)
	}
	return root
}

func TestVerifyExportPreservesByteArray(t *testing.T) {
	root := fixture(t)
	s, err := verifyExport(root)
	if err != nil {
		t.Fatal(err)
	}
	if got, want := string(s.Pages["index.mu"]), string([]byte{'A', 0, 'B'}); got != want {
		t.Fatalf("page bytes = %v, want %v", []byte(got), []byte(want))
	}
}

func TestSourceMustBeJSONIntegerArray(t *testing.T) {
	if _, err := decodeByteArray(json.RawMessage(`"QQ=="`)); err == nil {
		t.Fatal("expected base64-encoded JSON string to be rejected")
	}
	if _, err := decodeByteArray(json.RawMessage(`[256]`)); err == nil {
		t.Fatal("expected out-of-range byte to be rejected")
	}
	if got, err := decodeByteArray(json.RawMessage(`[0,255]`)); err != nil || len(got) != 2 || got[1] != 255 {
		t.Fatalf("decodeByteArray = %v, %v", got, err)
	}
}

func TestVerifyExportRejectsAlteredPage(t *testing.T) {
	root := fixture(t)
	if err := os.WriteFile(filepath.Join(root, "pages", "index.mu"), []byte("different"), 0600); err != nil {
		t.Fatal(err)
	}
	if _, err := verifyExport(root); err == nil {
		t.Fatal("expected manifest mismatch to be rejected")
	}
}

func TestVerifyExportRejectsUnlistedAndExecutablePages(t *testing.T) {
	t.Run("unlisted", func(t *testing.T) {
		root := fixture(t)
		if err := os.WriteFile(filepath.Join(root, "pages", "extra.mu"), []byte("x"), 0600); err != nil {
			t.Fatal(err)
		}
		if _, err := verifyExport(root); err == nil {
			t.Fatal("expected unlisted page to be rejected")
		}
	})
	t.Run("executable", func(t *testing.T) {
		root := fixture(t)
		if err := os.Chmod(filepath.Join(root, "pages", "index.mu"), 0700); err != nil {
			t.Fatal(err)
		}
		if _, err := verifyExport(root); err == nil {
			t.Fatal("expected executable page to be rejected")
		}
	})
}

func TestVerifyExportRejectsUnsafePathAndSymlink(t *testing.T) {
	t.Run("unsafe", func(t *testing.T) {
		root := fixture(t)
		path := filepath.Join(root, "snapshot.json")
		manifest, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		var data map[string]any
		if err := json.Unmarshal(manifest, &data); err != nil {
			t.Fatal(err)
		}
		pages := data["pages"].(map[string]any)
		page := pages["/index.mu"].(map[string]any)
		metadata := page["metadata"].(map[string]any)
		metadata["path"] = "../index.mu"
		encoded, err := json.Marshal(data)
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, encoded, 0600); err != nil {
			t.Fatal(err)
		}
		if _, err := verifyExport(root); err == nil {
			t.Fatal("expected unsafe path to be rejected")
		}
	})
	t.Run("symlink", func(t *testing.T) {
		root := fixture(t)
		page := filepath.Join(root, "pages", "index.mu")
		if err := os.Remove(page); err != nil {
			t.Fatal(err)
		}
		if err := os.Symlink(filepath.Join(root, "snapshot.json"), page); err != nil {
			t.Fatal(err)
		}
		if _, err := verifyExport(root); err == nil {
			t.Fatal("expected page symlink to be rejected")
		}
	})
}

func TestIdentityMustBeOutsideExportAndParentMustExist(t *testing.T) {
	root := fixture(t)
	if _, err := prepareIdentityPath(filepath.Join(root, "identity"), root); err == nil {
		t.Fatal("expected identity inside export to be rejected")
	}
	if _, err := prepareIdentityPath(filepath.Join(t.TempDir(), "missing", "identity"), root); err == nil {
		t.Fatal("expected missing identity parent to be rejected")
	}
}
