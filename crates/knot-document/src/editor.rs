// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use cambium::{CaretSelection, TextCommand, TextInput};
#[cfg(feature = "engine")]
use illume::{Fold, OutlineItem, Span};
#[cfg(feature = "engine")]
use inker::EngineDocument;
pub use knot_editor_host::EditOutcome;
use knot_editor_host::KnotEditor as SharedKnotEditor;
use same_file::Handle;

use crate::{DocumentFormat, SaveOutcome, write_if_distinct};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnotEditorSaveError {
    ExternalChange,
    Failed(String),
}

#[derive(Debug)]
struct FileBaseline {
    bytes: Vec<u8>,
    handle: Handle,
}

impl FileBaseline {
    fn observe(path: &Path) -> Result<Self, String> {
        let mut file = fs::File::open(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let handle = Handle::from_file(file)
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
        Ok(Self { bytes, handle })
    }

    fn still_matches(&self, path: &Path) -> Result<bool, String> {
        let handle = Handle::from_path(path)
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
        let bytes = fs::read(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        Ok(handle == self.handle && bytes == self.bytes)
    }
}

/// One Djot or legacy `.knot` session. Its Cambium input is the only source buffer.
pub struct KnotEditor {
    path: Option<PathBuf>,
    address: String,
    format: DocumentFormat,
    editor: SharedKnotEditor,
    baseline: Option<FileBaseline>,
}

impl KnotEditor {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let requested_path = path.into();
        let format = DocumentFormat::from_path(&requested_path)
            .filter(|format| matches!(format, DocumentFormat::Knot | DocumentFormat::Djot))
            .ok_or_else(|| {
                format!(
                    "KnotEditor requires a .djot or .knot file: {}",
                    requested_path.display()
                )
            })?;
        // A document opened through a symlink keeps editing the resolved file,
        // rather than later replacing the symlink itself during atomic save.
        let path = fs::canonicalize(&requested_path)
            .map_err(|error| format!("could not resolve {}: {error}", requested_path.display()))?;
        let baseline = FileBaseline::observe(&path)?;
        let source = String::from_utf8(baseline.bytes.clone())
            .map_err(|error| format!("{} is not UTF-8: {error}", path.display()))?;
        let address = crate::writer::file_address(&path)?;
        Ok(Self {
            path: Some(path),
            editor: SharedKnotEditor::scratch(address.clone(), source),
            address,
            format,
            baseline: Some(baseline),
        })
    }

    pub fn scratch(address: impl Into<String>, source: impl Into<String>) -> Self {
        let address = address.into();
        Self {
            path: None,
            editor: SharedKnotEditor::scratch(address.clone(), source),
            address,
            format: DocumentFormat::Djot,
            baseline: None,
        }
    }

    pub fn input(&self) -> &TextInput {
        self.editor.input()
    }
    pub fn input_mut(&mut self) -> &mut TextInput {
        self.editor.input_mut()
    }
    pub fn source(&self) -> &str {
        self.editor.source()
    }
    pub fn address(&self) -> &str {
        &self.address
    }
    pub fn format(&self) -> DocumentFormat {
        self.format
    }
    pub fn selection(&self) -> CaretSelection {
        self.editor.selection()
    }
    pub fn apply(&mut self, command: TextCommand) -> EditOutcome {
        self.editor.apply(command)
    }
    pub fn apply_layout_selection(&mut self, selection: CaretSelection) -> EditOutcome {
        self.apply(TextCommand::SetSelection(selection))
    }
    pub fn is_dirty(&self) -> bool {
        self.editor.is_dirty()
    }
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub fn is_file_read_only(&self) -> bool {
        self.path.as_ref().is_some_and(|path| {
            fs::metadata(path).is_ok_and(|metadata| metadata.permissions().readonly())
        })
    }

    pub fn save(&mut self) -> Result<SaveOutcome, String> {
        self.save_guarded().map_err(|error| match error {
            KnotEditorSaveError::ExternalChange => "file changed on disk since opening".to_owned(),
            KnotEditorSaveError::Failed(message) => message,
        })
    }

    pub fn save_guarded(&mut self) -> Result<SaveOutcome, KnotEditorSaveError> {
        let path = self.path.as_deref().ok_or_else(|| {
            KnotEditorSaveError::Failed("scratch Knot editor has no save path".to_owned())
        })?;
        if !self.editor.is_dirty() {
            return Ok(SaveOutcome::Unchanged);
        }
        let baseline = self.baseline.as_ref().ok_or_else(|| {
            KnotEditorSaveError::Failed("file document has no disk baseline".to_owned())
        })?;
        match fs::metadata(path) {
            Ok(metadata) if metadata.permissions().readonly() => {
                return Err(KnotEditorSaveError::Failed(
                    "document target is read-only".to_owned(),
                ));
            },
            Ok(_) => {},
            Err(error) => {
                return match error.kind() {
                    std::io::ErrorKind::NotFound => Err(KnotEditorSaveError::ExternalChange),
                    _ => Err(KnotEditorSaveError::Failed(format!(
                        "could not inspect {}: {error}",
                        path.display()
                    ))),
                };
            },
        }
        let matches_baseline = baseline
            .still_matches(path)
            .map_err(KnotEditorSaveError::Failed)?;
        if !matches_baseline {
            return Err(KnotEditorSaveError::ExternalChange);
        }
        let source = self.editor.source().to_owned();
        let outcome = write_if_distinct(path, &baseline.bytes, source.as_bytes())
            .map_err(KnotEditorSaveError::Failed)?;
        let updated_baseline = FileBaseline::observe(path).map_err(KnotEditorSaveError::Failed)?;
        if updated_baseline.bytes != source.as_bytes() {
            return Err(KnotEditorSaveError::ExternalChange);
        }
        self.editor.accept_saved_source(&source);
        self.baseline = Some(updated_baseline);
        Ok(outcome)
    }

    pub fn save_as(
        &mut self,
        target: impl Into<PathBuf>,
    ) -> Result<SaveOutcome, KnotEditorSaveError> {
        let target = target.into();
        let format = DocumentFormat::from_path(&target)
            .filter(|format| matches!(format, DocumentFormat::Knot | DocumentFormat::Djot))
            .ok_or_else(|| {
                KnotEditorSaveError::Failed(format!(
                    "KnotEditor requires a .djot or .knot save target: {}",
                    target.display()
                ))
            })?;
        if self.is_current_path(&target) {
            return self.save_guarded();
        }
        let source = self.editor.source().to_owned();
        crate::writer::create_bytes_new(&target, source.as_bytes())
            .map_err(KnotEditorSaveError::Failed)?;
        let target = fs::canonicalize(&target).map_err(|error| {
            KnotEditorSaveError::Failed(format!(
                "could not resolve saved target {}: {error}",
                target.display()
            ))
        })?;
        let address = crate::writer::file_address(&target).map_err(KnotEditorSaveError::Failed)?;
        let baseline = FileBaseline::observe(&target).map_err(KnotEditorSaveError::Failed)?;
        if baseline.bytes != source.as_bytes() {
            return Err(KnotEditorSaveError::ExternalChange);
        }
        self.path = Some(target);
        self.address = address.clone();
        self.format = format;
        let mut replacement = SharedKnotEditor::scratch(address, "");
        std::mem::swap(replacement.input_mut(), self.editor.input_mut());
        replacement.accept_saved_source(&source);
        self.editor = replacement;
        self.baseline = Some(baseline);
        Ok(SaveOutcome::Written)
    }

    pub fn reload(&mut self) -> Result<(), String> {
        let path = self
            .path
            .as_deref()
            .ok_or_else(|| "scratch Knot editor has no reload path".to_owned())?;
        let baseline = FileBaseline::observe(path)?;
        let source = String::from_utf8(baseline.bytes.clone())
            .map_err(|error| format!("{} is not UTF-8: {error}", path.display()))?;
        let address = crate::writer::file_address(path)?;
        self.address = address.clone();
        self.editor = SharedKnotEditor::scratch(address, source);
        self.baseline = Some(baseline);
        Ok(())
    }

    fn is_current_path(&self, target: &Path) -> bool {
        self.path.as_ref().is_some_and(|path| {
            path == target
                || matches!(
                    (fs::canonicalize(path).ok(), fs::canonicalize(target).ok()),
                    (Some(path), Some(target)) if path == target
                )
        })
    }

    #[cfg(feature = "engine")]
    pub fn highlights(&self) -> Vec<Span> {
        self.editor.highlights()
    }
    #[cfg(feature = "engine")]
    pub fn outline(&self) -> Vec<OutlineItem> {
        self.editor.outline()
    }
    #[cfg(feature = "engine")]
    pub fn folds(&self) -> Vec<Fold> {
        self.editor.folds()
    }
    #[cfg(feature = "engine")]
    pub fn preview(&self) -> Result<EngineDocument, String> {
        self.editor.preview()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use cambium::{CaretAffinity, CaretPosition};
    use tempfile::tempdir;

    use super::*;

    #[cfg(feature = "engine")]
    #[test]
    fn commands_drive_the_one_source_used_by_every_readout() {
        let mut editor = KnotEditor::scratch("memory:note", "# One\n");
        assert_eq!(editor.outline().len(), 1);

        let outcome = editor.apply(TextCommand::Insert("\n## Two\n".into()));
        assert_eq!(
            outcome,
            EditOutcome {
                state_changed: true,
                source_changed: true,
            }
        );
        assert_eq!(editor.outline().len(), 2);
        assert!(!editor.highlights().is_empty());
        assert!(!editor.preview().unwrap().blocks.is_empty());

        editor.apply(TextCommand::Undo);
        assert_eq!(editor.source(), "# One\n");
        assert_eq!(editor.outline().len(), 1);
    }

    #[test]
    fn layout_selection_preserves_byte_affinity() {
        let mut editor = KnotEditor::scratch("memory:note", "abc");
        let selection = CaretSelection {
            anchor: CaretPosition {
                byte: 0,
                affinity: CaretAffinity::Downstream,
            },
            focus: CaretPosition {
                byte: 2,
                affinity: CaretAffinity::Upstream,
            },
        };
        editor.apply_layout_selection(selection);
        assert_eq!(editor.selection(), selection);
    }

    #[cfg(feature = "engine")]
    #[test]
    fn ime_preedit_is_not_committed_or_fed_to_the_readout() {
        let mut editor = KnotEditor::scratch("memory:note", "# One\n");
        let before = editor.preview().unwrap();
        let outcome = editor.apply(TextCommand::SetComposition {
            text: "仮".into(),
            selection: Some((3, 3)),
        });
        assert!(outcome.state_changed);
        assert!(!outcome.source_changed);
        assert_eq!(editor.source(), "# One\n");
        assert_eq!(editor.preview().unwrap(), before);
        assert_eq!(editor.input().render_text(), "# One\n仮");
    }

    #[test]
    fn committed_commands_write_back_to_the_native_file() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("note.djot");
        fs::write(&path, "# One\n").unwrap();
        let mut editor = KnotEditor::open(&path).unwrap();
        editor.apply(TextCommand::Insert("\n## Two\n".into()));
        assert!(editor.is_dirty());
        assert_eq!(editor.save().unwrap(), SaveOutcome::Written);
        assert_eq!(editor.save().unwrap(), SaveOutcome::Unchanged);
        assert_eq!(fs::read_to_string(path).unwrap(), "# One\n\n## Two\n");
    }
}
