// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::fs;
use std::io::{self, Write};
use std::path::Path;
#[cfg(feature = "engine")]
use std::path::PathBuf;

/// Formats Knot can author directly. Djot is the native current format; `.knot`
/// remains a compatibility format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentFormat {
    Knot,
    Markdown,
    Djot,
    Scroll,
    Json,
}

impl DocumentFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("knot") => Some(Self::Knot),
            Some("md" | "markdown") => Some(Self::Markdown),
            Some("djot") => Some(Self::Djot),
            Some("scroll") => Some(Self::Scroll),
            Some("json") => Some(Self::Json),
            _ => None,
        }
    }
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Knot => "text/vnd.knot",
            Self::Markdown => "text/markdown",
            Self::Djot => "text/djot",
            Self::Scroll => "text/scroll",
            Self::Json => "application/vnd.knot.document+json",
        }
    }
    pub fn from_media_type(value: &str) -> Option<Self> {
        match value {
            "text/vnd.knot" => Some(Self::Knot),
            "text/markdown" => Some(Self::Markdown),
            "text/djot" => Some(Self::Djot),
            "text/scroll" | "text/x-scroll" => Some(Self::Scroll),
            "application/vnd.knot.document+json" | "application/json" => Some(Self::Json),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Unchanged,
    Written,
}

#[doc(hidden)]
pub fn write_if_distinct(path: &Path, before: &[u8], after: &[u8]) -> Result<SaveOutcome, String> {
    if before == after {
        return Ok(SaveOutcome::Unchanged);
    }
    #[cfg(windows)]
    if !path.exists() {
        return create_bytes_new(path, after).map(|()| SaveOutcome::Written);
    }
    replace_bytes_atomically(path, after)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    Ok(SaveOutcome::Written)
}

/// Replace a Unix file through a fully written sibling temporary file.
///
/// The temporary receives the target's basic permissions before `rename`.
/// Extended ACL, stream, and encryption metadata remain filesystem-specific on
/// this path; Windows uses `ReplaceFileW` below instead.
#[cfg(not(windows))]
fn replace_bytes_atomically(path: &Path, contents: &[u8]) -> io::Result<()> {
    let (temporary, mut file) = create_sibling_temporary(path)?;
    let result = (|| {
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);

        if let Ok(metadata) = fs::metadata(path) {
            fs::set_permissions(&temporary, metadata.permissions())?;
        }
        replace_temporary(path, &temporary, |from, to| fs::rename(from, to))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn replace_bytes_atomically(path: &Path, contents: &[u8]) -> io::Result<()> {
    let (temporary, mut file) = create_sibling_temporary(path)?;
    if let Err(error) = (|| -> io::Result<()> {
        file.write_all(contents)?;
        file.sync_all()
    })() {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    drop(file);

    // ReplaceFileW preserves the replaced file's security descriptor,
    // alternate streams, and encryption attributes. Its backup records the
    // recoverable old bytes if the replacement reaches a partial state.
    // https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew
    replace_temporary_windows(path, &temporary)
}

/// Create an exclusive output target without replacing an existing file.
pub(crate) fn create_bytes_new(path: &Path, contents: &[u8]) -> Result<(), String> {
    let (temporary, mut file) = create_sibling_temporary(path)
        .map_err(|error| format!("could not prepare {}: {error}", path.display()))?;
    let result = (|| -> io::Result<()> {
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);

        // A hard link fails if the destination appeared after the picker
        // returned. That makes Save As a non-clobbering operation without a
        // check-then-write race.
        fs::hard_link(&temporary, path)?;
        // The new target has committed. Retaining an unreachable temporary is
        // preferable to reporting a failed Save As after that point.
        let _ = fs::remove_file(&temporary);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| format!("could not create {}: {error}", path.display()))
}

fn create_sibling_temporary(path: &Path) -> io::Result<(std::path::PathBuf, fs::File)> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "document target must have a parent directory",
        )
    })?;
    for nonce in 0..128_u32 {
        let candidate = parent.join(format!(".knot-write-{}-{nonce}.tmp", std::process::id()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a document temporary path",
    ))
}

#[cfg(windows)]
fn create_backup_location(path: &Path) -> io::Result<(std::path::PathBuf, std::path::PathBuf)> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "document target must have a parent directory",
        )
    })?;
    for nonce in 0..128_u32 {
        let directory = parent.join(format!(
            ".knot-replace-recovery-{}-{nonce}",
            std::process::id()
        ));
        match fs::create_dir(&directory) {
            Ok(()) => return Ok((directory.clone(), directory.join("original.bak"))),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a document replacement recovery directory",
    ))
}

#[cfg(windows)]
fn replace_temporary_windows(target: &Path, temporary: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    let (recovery_directory, backup) = create_backup_location(target)?;
    let target_wide = wide(target);
    let temporary_wide = wide(temporary);
    let backup_wide = wide(&backup);
    // SAFETY: all three paths are NUL-terminated UTF-16 strings that live for
    // the duration of this synchronous Win32 call; the optional buffers are null.
    let replaced = unsafe {
        ReplaceFileW(
            target_wide.as_ptr(),
            temporary_wide.as_ptr(),
            backup_wide.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced != 0 {
        // Success has committed the new source. A leftover backup is safe to
        // remove later, but failure to clean it cannot turn this save into a
        // failed one.
        let _ = fs::remove_file(&backup);
        let _ = fs::remove_dir(&recovery_directory);
        return Ok(());
    }

    let error = io::Error::last_os_error();
    let recovery = match error.raw_os_error() {
        Some(1176) => "the original retains its target filename",
        Some(1177) => "the original may be available at the backup path",
        _ => "the temporary and backup directory may contain recoverable source",
    };
    // Do not delete either artifact here. ReplaceFileW documents partial
    // failure states, so the caller must retain both paths for recovery.
    Err(io::Error::new(
        error.kind(),
        format!(
            "ReplaceFileW failed for {}; {recovery}; temporary: {}; backup: {}; recovery directory: {}; {error}",
            target.display(),
            temporary.display(),
            backup.display(),
            recovery_directory.display(),
        ),
    ))
}

#[cfg(any(not(windows), test))]
fn replace_temporary(
    target: &Path,
    temporary: &Path,
    rename: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    rename(temporary, target)
}

#[cfg(test)]
mod atomic_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn failed_replacement_leaves_the_old_document_bytes_intact() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("field.djot");
        let replacement = temp.path().join("replacement.tmp");
        fs::write(&target, b"old bytes").unwrap();
        fs::write(&replacement, b"new bytes").unwrap();

        let error = replace_temporary(&target, &replacement, |_from, _to| {
            Err(io::Error::other("simulated replacement failure"))
        })
        .expect_err("replacement fails");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(fs::read(&target).unwrap(), b"old bytes");
        assert_eq!(fs::read(&replacement).unwrap(), b"new bytes");
    }

    #[cfg(windows)]
    #[test]
    fn windows_replace_file_preserves_an_alternate_data_stream() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("field.djot");
        fs::write(&target, b"old bytes").unwrap();
        let stream = std::path::PathBuf::from(format!("{}:knot_metadata", target.display()));
        match fs::write(&stream, b"old metadata") {
            Ok(()) => {},
            Err(error) if matches!(error.raw_os_error(), Some(50 | 123)) => {
                // ERROR_NOT_SUPPORTED / ERROR_INVALID_NAME: this filesystem
                // does not expose NTFS alternate data streams.
                return;
            },
            Err(error) => panic!("could not write alternate data stream: {error}"),
        }

        // A real session keeps its open-time identity handle alive through
        // replacement and backup cleanup.
        let _held_original = fs::File::open(&target).unwrap();

        assert_eq!(
            write_if_distinct(&target, b"old bytes", b"new bytes").unwrap(),
            SaveOutcome::Written
        );
        assert_eq!(fs::read(&target).unwrap(), b"new bytes");
        assert_eq!(fs::read(&stream).unwrap(), b"old metadata");
        assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("knot-replace-recovery")
        }));
    }

    #[cfg(windows)]
    #[test]
    fn windows_replace_file_failure_retains_recovery_artifacts() {
        use std::os::windows::fs::OpenOptionsExt;

        let temp = tempdir().unwrap();
        let target = temp.path().join("field.djot");
        fs::write(&target, b"old bytes").unwrap();
        let locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&target)
            .unwrap();

        let error = write_if_distinct(&target, b"old bytes", b"new bytes")
            .expect_err("ReplaceFileW must fail while the target denies sharing");
        assert!(error.contains("ReplaceFileW failed"));
        assert!(error.contains("temporary:"));
        assert!(error.contains("recovery directory:"));

        drop(locked);
        assert_eq!(fs::read(&target).unwrap(), b"old bytes");
        let names: Vec<_> = fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().any(|name| name.contains("knot-write")));
        assert!(
            names
                .iter()
                .any(|name| name.contains("knot-replace-recovery"))
        );
    }
}

pub(crate) fn file_address(path: &Path) -> Result<String, String> {
    let path = fs::canonicalize(path)
        .map_err(|error| format!("could not resolve {}: {error}", path.display()))?;
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        Ok(format!(
            "file:///{}",
            text.strip_prefix(r"\\?\")
                .unwrap_or(&text)
                .replace('\\', "/")
        ))
    }
    #[cfg(not(windows))]
    {
        Ok(format!("file://{}", path.to_string_lossy()))
    }
}

#[cfg(feature = "engine")]
mod engine {
    use super::*;
    use inker::{DocumentTrustState, Engine, EngineDocument, EngineInput};
    use nematic::knot::djot::blocks_to_djot;
    use nematic::{DjotKnotEngine, MarkdownEngine};
    use std::io;

    impl DocumentFormat {
        pub fn validate_source(self, address: &str, source: &str) -> Result<(), String> {
            self.parse(address, source.as_bytes()).map(|_| ())
        }
        pub fn to_commonmark(self, address: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
            self.parse(address, bytes)
                .map(|document| document.to_markdown().into_bytes())
        }
        fn parse(self, address: &str, bytes: &[u8]) -> Result<EngineDocument, String> {
            if self == Self::Json {
                return serde_json::from_slice(bytes)
                    .map_err(|error| format!("invalid Knot document JSON: {error}"));
            }
            let input = EngineInput {
                address: address.to_owned(),
                body: std::str::from_utf8(bytes)
                    .map_err(|error| format!("document is not UTF-8: {error}"))?
                    .to_owned(),
                content_type: Some(self.media_type().to_owned()),
            };
            match self {
                Self::Knot | Self::Djot => DjotKnotEngine::new()
                    .render(&input)
                    .map_err(|error| format!("could not parse Djot document: {error}")),
                Self::Markdown => MarkdownEngine::new()
                    .render(&input)
                    .map_err(|error| format!("could not parse Markdown document: {error}")),
                Self::Scroll => nematic::ScrollEngine::new()
                    .render(&input)
                    .map_err(|error| error.to_string()),
                Self::Json => unreachable!(),
            }
        }
        fn serialize(self, document: &EngineDocument) -> Result<Vec<u8>, String> {
            let text = match self {
                Self::Knot => document_to_knot(document),
                Self::Markdown => document.to_markdown(),
                Self::Djot => blocks_to_djot(&document.blocks),
                Self::Scroll => return Err("Scroll source must be saved through the native source editor; block serialization is unsupported".into()),
                Self::Json => {
                    serde_json::to_string_pretty(document)
                        .map_err(|error| format!("could not encode Knot document JSON: {error}"))?
                        + "\n"
                },
            };
            Ok(text.into_bytes())
        }
    }
    pub struct AuthoredFile {
        path: PathBuf,
        format: DocumentFormat,
        document: EngineDocument,
        original: Vec<u8>,
        dirty: bool,
    }
    impl AuthoredFile {
        pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
            let path = path.into();
            let format = DocumentFormat::from_path(&path)
                .ok_or_else(|| format!("unsupported Knot authoring format: {}", path.display()))?;
            let original = fs::read(&path)
                .map_err(|error| format!("could not read {}: {error}", path.display()))?;
            let document = format.parse(&file_address(&path)?, &original)?;
            Ok(Self {
                path,
                format,
                document,
                original,
                dirty: false,
            })
        }
        pub fn path(&self) -> &Path {
            &self.path
        }
        pub fn format(&self) -> DocumentFormat {
            self.format
        }
        pub fn document(&self) -> &EngineDocument {
            &self.document
        }
        pub fn document_mut(&mut self) -> &mut EngineDocument {
            self.dirty = true;
            &mut self.document
        }
        pub fn save(&mut self) -> Result<SaveOutcome, String> {
            if !self.dirty {
                return Ok(SaveOutcome::Unchanged);
            }
            let encoded = self.format.serialize(&self.document)?;
            let outcome = write_if_distinct(&self.path, &self.original, &encoded)?;
            self.original = encoded;
            self.dirty = false;
            Ok(outcome)
        }
        pub fn save_as(
            &self,
            path: impl AsRef<Path>,
            format: DocumentFormat,
        ) -> Result<SaveOutcome, String> {
            let path = path.as_ref();
            let existing = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
                Err(error) => return Err(format!("could not read {}: {error}", path.display())),
            };
            write_if_distinct(path, &existing, &format.serialize(&self.document)?)
        }
        pub fn canonicalize(
            format: DocumentFormat,
            address: &str,
            bytes: &[u8],
        ) -> Result<Vec<u8>, String> {
            format
                .parse(address, bytes)
                .and_then(|document| format.serialize(&document))
        }
    }
    fn document_to_knot(document: &EngineDocument) -> String {
        let mut output = String::new();
        let frontmatter = document.title.is_some()
            || document.provenance.canonical_uri.is_some()
            || document.provenance.fetched_at.is_some()
            || document.provenance.source_label.is_some()
            || document.trust != DocumentTrustState::Unknown;
        if frontmatter {
            output.push_str("---\n");
            if let Some(value) = &document.title {
                output.push_str(&format!("title: {value}\n"));
            }
            if let Some(value) = &document.provenance.canonical_uri {
                output.push_str(&format!("source: {value}\n"));
            }
            if let Some(value) = &document.provenance.fetched_at {
                output.push_str(&format!("captured: {value}\n"));
            }
            if let Some(value) = &document.provenance.source_label {
                output.push_str(&format!("source_label: {value}\n"));
            }
            let trust = match document.trust {
                DocumentTrustState::Trusted => Some("trusted"),
                DocumentTrustState::Tofu => Some("tofu"),
                DocumentTrustState::Insecure => Some("insecure"),
                DocumentTrustState::Broken => Some("broken"),
                DocumentTrustState::Unknown => None,
            };
            if let Some(value) = trust {
                output.push_str(&format!("trust: {value}\n"));
            }
            output.push_str("---\n\n");
        }
        output.push_str(&blocks_to_djot(&document.blocks));
        output
    }
}
#[cfg(feature = "engine")]
pub use engine::AuthoredFile;

#[cfg(all(test, feature = "engine"))]
mod tests {
    use std::fs;

    use inker::EngineDocument;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn foreign_formats_reach_a_fixed_point_after_one_parse_write() {
        let cases = [
            (
                DocumentFormat::Markdown,
                b"# Heading\n\nA *small* note.\n".as_slice(),
            ),
            (
                DocumentFormat::Djot,
                b"# Heading\n\nA small note.\n".as_slice(),
            ),
            (
                DocumentFormat::Json,
                br#"{"address":"memory:test","title":null,"content_type":"text/plain","lang":null,"provenance":{},"trust":"Unknown","diagnostics":[],"blocks":[]}"#,
            ),
        ];
        for (format, source) in cases {
            let once = AuthoredFile::canonicalize(format, "memory:test", source).unwrap();
            let twice = AuthoredFile::canonicalize(format, "memory:test", &once).unwrap();
            assert_eq!(once, twice, "{format:?} did not reach a fixed point");
        }
    }

    #[test]
    fn a_canonical_knot_round_trip_is_byte_exact() {
        let source =
            b"---\ntitle: Field note\ntrust: tofu\n---\n\n# Field note\n\norchard observations\n";
        let canonical =
            AuthoredFile::canonicalize(DocumentFormat::Knot, "memory:field", source).unwrap();
        assert_eq!(
            AuthoredFile::canonicalize(DocumentFormat::Knot, "memory:field", &canonical).unwrap(),
            canonical
        );
    }

    #[test]
    fn untouched_files_never_enter_the_write_path() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("foreign.md");
        fs::write(&path, "#  Deliberately foreign spacing\n").unwrap();
        let mut file = AuthoredFile::open(&path).unwrap();

        let original_permissions = fs::metadata(&path).unwrap().permissions();
        let mut read_only = original_permissions.clone();
        read_only.set_readonly(true);
        fs::set_permissions(&path, read_only).unwrap();
        assert_eq!(file.save().unwrap(), SaveOutcome::Unchanged);

        fs::set_permissions(&path, original_permissions).unwrap();
    }

    #[test]
    fn caller_selects_the_output_format() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("note.md");
        let target = temp.path().join("note.json");
        fs::write(&source, "# Note\n\nA body.\n").unwrap();
        let file = AuthoredFile::open(&source).unwrap();
        assert_eq!(
            file.save_as(&target, DocumentFormat::Json).unwrap(),
            SaveOutcome::Written
        );
        let document: EngineDocument = serde_json::from_slice(&fs::read(target).unwrap()).unwrap();
        assert_eq!(document.title.as_deref(), Some("Note"));
    }
}
