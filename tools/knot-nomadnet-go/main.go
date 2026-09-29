// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// knot-nomadnet-go validates a Knot export and launches an external
// Reticulum-Go pageserver against a private copy of its verified page bytes.
package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"time"
	"unicode/utf8"
)

const (
	maxSnapshotBytes = 24 << 20
	maxPageBytes     = 1 << 20
	maxSiteBytes     = 16 << 20
	maxPages         = 64
)

type snapshot struct {
	Version uint8                    `json:"version"`
	Format  string                   `json:"format"`
	Pages   map[string]publishedPage `json:"pages"`
}
type publishedPage struct {
	Metadata pageMetadata    `json:"metadata"`
	Source   json.RawMessage `json:"source"`
}
type pageMetadata struct {
	Path           string `json:"path"`
	Author         string `json:"author"`
	Language       string `json:"language"`
	Classification uint8  `json:"classification"`
	Published      string `json:"published"`
	Modified       string `json:"modified"`
	AbstractSource string `json:"abstract_source"`
}
type verifiedSnapshot struct {
	Root        string
	Pages       map[string][]byte
	SourceBytes int
}

func main() {
	if len(os.Args) < 2 {
		usage(os.Stderr)
		os.Exit(2)
	}
	var err error
	switch os.Args[1] {
	case "validate":
		err = runValidate(os.Args[2:])
	case "serve":
		err = runServe(os.Args[2:])
	case "help", "-h", "--help":
		usage(os.Stdout)
		return
	default:
		usage(os.Stderr)
		err = fmt.Errorf("unknown command %q", os.Args[1])
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "knot-nomadnet-go:", err)
		os.Exit(1)
	}
}

func usage(w io.Writer) {
	fmt.Fprintln(w, "Usage:")
	fmt.Fprintln(w, "  knot-nomadnet-go validate --snapshot DIR")
	fmt.Fprintln(w, "  knot-nomadnet-go serve --snapshot DIR --config FILE --identity FILE [--reticulum-go PATH] [--node-name NAME]")
}

func runValidate(args []string) error {
	fs := flag.NewFlagSet("validate", flag.ContinueOnError)
	fs.SetOutput(os.Stderr)
	dir := fs.String("snapshot", "", "Knot export directory")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if fs.NArg() != 0 || *dir == "" {
		return errors.New("validate requires --snapshot DIR")
	}
	s, err := verifyExport(*dir)
	if err != nil {
		return err
	}
	fmt.Printf("verified Micron page export: %d pages, %d source bytes\n", len(s.Pages), s.SourceBytes)
	return nil
}

func runServe(args []string) error {
	fs := flag.NewFlagSet("serve", flag.ContinueOnError)
	fs.SetOutput(os.Stderr)
	dir := fs.String("snapshot", "", "Knot export directory")
	configPath := fs.String("config", "", "existing Reticulum-Go configuration file")
	identityPath := fs.String("identity", "", "persistent identity file; upstream creates it if absent")
	binary := fs.String("reticulum-go", "reticulum-go", "Reticulum-Go executable")
	nodeName := fs.String("node-name", "Knot", "announce display name")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if fs.NArg() != 0 || *dir == "" || *configPath == "" || *identityPath == "" {
		return errors.New("serve requires --snapshot DIR, --config FILE, and --identity FILE")
	}
	s, err := verifyExport(*dir)
	if err != nil {
		return err
	}
	config, err := existingRegularFile(*configPath, "Reticulum config")
	if err != nil {
		return err
	}
	identity, err := prepareIdentityPath(*identityPath, s.Root)
	if err != nil {
		return err
	}
	if filepath.Clean(config) == filepath.Clean(identity) {
		return errors.New("Reticulum config and identity paths must be different")
	}
	program, err := exec.LookPath(*binary)
	if err != nil {
		return fmt.Errorf("find Reticulum-Go executable: %w", err)
	}
	stage, err := os.MkdirTemp("", "knot-nomadnet-go-")
	if err != nil {
		return fmt.Errorf("create private pageserver tree: %w", err)
	}
	defer os.RemoveAll(stage)
	pagesDir, filesDir := filepath.Join(stage, "pages"), filepath.Join(stage, "files")
	if err := os.Mkdir(pagesDir, 0700); err != nil {
		return err
	}
	if err := os.Mkdir(filesDir, 0700); err != nil {
		return err
	}
	for name, data := range s.Pages {
		if err := writePrivate(filepath.Join(pagesDir, name), data); err != nil {
			return fmt.Errorf("stage %q: %w", name, err)
		}
	}
	signals := make(chan os.Signal, 2)
	signal.Notify(signals, os.Interrupt, syscall.SIGTERM)
	defer signal.Stop(signals)
	cmd := exec.Command(program,
		"pageserver", "--config", config, "--identity", identity,
		"--pages-dir", pagesDir, "--files-dir", filesDir,
		"--node-name", *nodeName, "--announce-interval", "0",
		"--page-refresh", "0", "--file-refresh", "0", "--no-page-stats")
	cmd.Stdin, cmd.Stdout, cmd.Stderr = os.Stdin, os.Stdout, os.Stderr
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("start Reticulum-Go pageserver: %w", err)
	}
	return supervise(cmd, signals)
}

func verifyExport(root string) (verifiedSnapshot, error) {
	absRoot, err := filepath.Abs(root)
	if err != nil {
		return verifiedSnapshot{}, err
	}
	if err := rejectSymlinkComponents(absRoot); err != nil {
		return verifiedSnapshot{}, fmt.Errorf("snapshot path: %w", err)
	}
	info, err := os.Lstat(absRoot)
	if err != nil || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return verifiedSnapshot{}, errors.New("snapshot root must be a real directory")
	}
	entries, err := os.ReadDir(absRoot)
	if err != nil {
		return verifiedSnapshot{}, err
	}
	if len(entries) != 2 || !hasNames(entries, "pages", "snapshot.json") {
		return verifiedSnapshot{}, errors.New("snapshot root must contain only snapshot.json and pages/")
	}
	manifestPath := filepath.Join(absRoot, "snapshot.json")
	info, err = os.Lstat(manifestPath)
	if err != nil || !info.Mode().IsRegular() || info.Mode()&0111 != 0 {
		return verifiedSnapshot{}, errors.New("snapshot.json must be a non-executable regular file")
	}
	f, err := os.Open(manifestPath)
	if err != nil {
		return verifiedSnapshot{}, err
	}
	manifest, readErr := io.ReadAll(io.LimitReader(f, maxSnapshotBytes+1))
	closeErr := f.Close()
	if readErr != nil {
		return verifiedSnapshot{}, readErr
	}
	if closeErr != nil {
		return verifiedSnapshot{}, closeErr
	}
	if len(manifest) > maxSnapshotBytes {
		return verifiedSnapshot{}, errors.New("snapshot.json exceeds 24 MiB")
	}
	var data snapshot
	decoder := json.NewDecoder(bytes.NewReader(manifest))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&data); err != nil {
		return verifiedSnapshot{}, fmt.Errorf("decode snapshot.json: %w", err)
	}
	if err := decoder.Decode(new(any)); err != io.EOF {
		return verifiedSnapshot{}, errors.New("snapshot.json contains trailing JSON data")
	}
	if data.Version != 1 || data.Format != "micron" {
		return verifiedSnapshot{}, errors.New("snapshot must be PublishedSnapshotV1 with format micron")
	}
	if len(data.Pages) == 0 || len(data.Pages) > maxPages {
		return verifiedSnapshot{}, errors.New("snapshot must contain 1–64 pages")
	}
	pagesDir := filepath.Join(absRoot, "pages")
	info, err = os.Lstat(pagesDir)
	if err != nil || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return verifiedSnapshot{}, errors.New("pages must be a real directory")
	}
	files, err := os.ReadDir(pagesDir)
	if err != nil {
		return verifiedSnapshot{}, err
	}
	if len(files) != len(data.Pages) {
		return verifiedSnapshot{}, errors.New("pages directory contains missing or unlisted page files")
	}
	listed := make(map[string][]byte, len(data.Pages))
	names := make(map[string]struct{}, len(data.Pages))
	total := 0
	for key, page := range data.Pages {
		name := page.Metadata.Path
		if key != "/"+name || !validPageName(name) {
			return verifiedSnapshot{}, fmt.Errorf("invalid published page path %q", key)
		}
		if page.Metadata.Classification > 9 || len(page.Metadata.Author) > 1024 || strings.ContainsAny(page.Metadata.Author, "\r\n\x00") || len(page.Metadata.Language) > 128 {
			return verifiedSnapshot{}, fmt.Errorf("invalid metadata for %q", name)
		}
		folded := strings.ToLower(name)
		if _, ok := names[folded]; ok {
			return verifiedSnapshot{}, fmt.Errorf("duplicate page name %q", name)
		}
		names[folded] = struct{}{}
		source, err := decodeByteArray(page.Source)
		if err != nil {
			return verifiedSnapshot{}, fmt.Errorf("page %q source: %w", name, err)
		}
		if len(source) > maxPageBytes {
			return verifiedSnapshot{}, fmt.Errorf("page %q exceeds 1 MiB", name)
		}
		if !utf8.Valid(source) || !utf8.ValidString(page.Metadata.AbstractSource) {
			return verifiedSnapshot{}, fmt.Errorf("page %q source and abstract must be UTF-8", name)
		}
		if len(page.Metadata.AbstractSource) > maxPageBytes {
			return verifiedSnapshot{}, fmt.Errorf("page %q abstract exceeds 1 MiB", name)
		}
		total += len(source) + len(page.Metadata.AbstractSource)
		if total > maxSiteBytes {
			return verifiedSnapshot{}, errors.New("snapshot exceeds 16 MiB of source and abstract data")
		}
		path := filepath.Join(pagesDir, name)
		info, err := os.Lstat(path)
		if err != nil || !info.Mode().IsRegular() || info.Mode()&0111 != 0 || info.Size() != int64(len(source)) {
			return verifiedSnapshot{}, fmt.Errorf("page %q must be a non-executable regular file matching source size", name)
		}
		f, err := os.Open(path)
		if err != nil {
			return verifiedSnapshot{}, err
		}
		openedInfo, err := f.Stat()
		if err != nil || !os.SameFile(info, openedInfo) {
			f.Close()
			return verifiedSnapshot{}, fmt.Errorf("page %q changed during validation", name)
		}
		content, readErr := io.ReadAll(io.LimitReader(f, maxPageBytes+1))
		closeErr := f.Close()
		if readErr != nil {
			return verifiedSnapshot{}, readErr
		}
		if closeErr != nil {
			return verifiedSnapshot{}, closeErr
		}
		if len(content) > maxPageBytes || !bytes.Equal(content, source) {
			return verifiedSnapshot{}, fmt.Errorf("page %q differs from manifest source bytes", name)
		}
		listed[name] = append([]byte(nil), source...)
	}
	for _, entry := range files {
		info, err := entry.Info()
		if err != nil || !info.Mode().IsRegular() || info.Mode()&0111 != 0 {
			return verifiedSnapshot{}, fmt.Errorf("pages entry %q is not a non-executable regular file", entry.Name())
		}
		if _, ok := listed[entry.Name()]; !ok {
			return verifiedSnapshot{}, fmt.Errorf("unlisted page file %q", entry.Name())
		}
	}
	if _, ok := listed["index.mu"]; !ok {
		return verifiedSnapshot{}, errors.New("Micron snapshot is missing /index.mu")
	}
	return verifiedSnapshot{Root: absRoot, Pages: listed, SourceBytes: total}, nil
}

func hasNames(entries []os.DirEntry, a, b string) bool {
	return entries[0].Name() == a && entries[1].Name() == b || entries[0].Name() == b && entries[1].Name() == a
}

func validPageName(name string) bool {
	if len(name) == 0 || len(name) > 128 || !strings.HasSuffix(name, ".mu") || strings.HasPrefix(name, ".") {
		return false
	}
	for _, c := range []byte(name) {
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '-' || c == '_' || c == '.') {
			return false
		}
	}
	return true
}

func decodeByteArray(raw json.RawMessage) ([]byte, error) {
	trimmed := bytes.TrimSpace(raw)
	if len(trimmed) == 0 || trimmed[0] != '[' {
		return nil, errors.New("must be a JSON integer array")
	}
	var source []byte
	if err := json.Unmarshal(trimmed, &source); err != nil {
		return nil, err
	}
	return source, nil
}

func existingRegularFile(path, description string) (string, error) {
	abs, err := filepath.Abs(path)
	if err != nil {
		return "", err
	}
	if err := rejectSymlinkComponents(abs); err != nil {
		return "", fmt.Errorf("%s: %w", description, err)
	}
	info, err := os.Lstat(abs)
	if err != nil || !info.Mode().IsRegular() || info.Mode()&os.ModeSymlink != 0 {
		return "", fmt.Errorf("%s must be an existing regular file", description)
	}
	return abs, nil
}

func prepareIdentityPath(path, exportRoot string) (string, error) {
	abs, err := filepath.Abs(path)
	if err != nil {
		return "", err
	}
	if err := rejectSymlinkComponents(abs); err != nil {
		return "", fmt.Errorf("identity path: %w", err)
	}
	parent := filepath.Dir(abs)
	info, err := os.Stat(parent)
	if err != nil || !info.IsDir() {
		return "", errors.New("identity parent directory must already exist")
	}
	if within(exportRoot, abs) {
		return "", errors.New("identity path must be outside the exported snapshot")
	}
	if info, err := os.Lstat(abs); err == nil {
		if !info.Mode().IsRegular() || info.Mode()&os.ModeSymlink != 0 {
			return "", errors.New("existing identity path must be a regular file")
		}
	} else if !os.IsNotExist(err) {
		return "", err
	}
	return abs, nil
}

func within(parent, child string) bool {
	rel, err := filepath.Rel(parent, child)
	return err == nil && (rel == "." || rel != ".." && !strings.HasPrefix(rel, ".."+string(os.PathSeparator)))
}

func rejectSymlinkComponents(path string) error {
	abs, err := filepath.Abs(path)
	if err != nil {
		return err
	}
	root := filepath.VolumeName(abs) + string(os.PathSeparator)
	current := root
	for _, part := range strings.Split(strings.TrimPrefix(abs, root), string(os.PathSeparator)) {
		if part == "" {
			continue
		}
		current = filepath.Join(current, part)
		info, err := os.Lstat(current)
		if os.IsNotExist(err) {
			continue
		}
		if err != nil {
			return err
		}
		if info.Mode()&os.ModeSymlink != 0 {
			return fmt.Errorf("path passes through symlink %s", current)
		}
	}
	return nil
}

func writePrivate(path string, data []byte) error {
	f, err := os.OpenFile(path, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0600)
	if err != nil {
		return err
	}
	if _, err := f.Write(data); err != nil {
		f.Close()
		return err
	}
	return f.Close()
}

func supervise(cmd *exec.Cmd, signals <-chan os.Signal) error {
	wait := make(chan error, 1)
	go func() { wait <- cmd.Wait() }()
	select {
	case err := <-wait:
		return err
	case sig := <-signals:
		if processSignal, ok := sig.(syscall.Signal); ok {
			_ = cmd.Process.Signal(processSignal)
		}
		select {
		case err := <-wait:
			return err
		case <-time.After(5 * time.Second):
			_ = cmd.Process.Kill()
			return <-wait
		}
	}
}
