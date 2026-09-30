// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A typed host dispatch seam for readings, independent of script value types.
//!
//! Implementing this trait does not sandbox a runtime. It is an in-process
//! host extension; each backend must enforce the budget and authority it
//! advertises. Hosts decide which backend may run a given script.

use crate::{ReadingBudget, ReadingError, ReadingInput, ReadingResult, ReadingScript, run};

/// How a backend enforces the `ReadingBudget::max_ops` bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetSupport {
    /// The evaluator itself counts and refuses excess operations.
    NativeOperations,
    /// A separate worker/process enforces execution and resource limits.
    ExternalIsolation,
    /// No reliable runaway bound; the host must use this only for trusted work.
    None,
}

/// Stable identity and version for receipts and host routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadingBackendDescriptor {
    pub id: &'static str,
    pub version: &'static str,
    pub budget_support: BudgetSupport,
}

/// Evaluate a typed reading input into owned rows, notes and provenance.
///
/// This trait carries no Rhai `Dynamic` or any language-specific VM value.
pub trait ReadingBackend: Send {
    fn descriptor(&self) -> ReadingBackendDescriptor;

    fn run(
        &mut self,
        script: &ReadingScript,
        input: &ReadingInput,
        budget: ReadingBudget,
    ) -> Result<ReadingResult, ReadingError>;
}

/// Existing Rhai reading semantics behind the typed dispatch seam.
#[derive(Default)]
pub struct RhaiReadingBackend;

impl ReadingBackend for RhaiReadingBackend {
    fn descriptor(&self) -> ReadingBackendDescriptor {
        ReadingBackendDescriptor {
            id: "knot.rhai.reading",
            version: env!("CARGO_PKG_VERSION"),
            budget_support: BudgetSupport::NativeOperations,
        }
    }

    fn run(
        &mut self,
        script: &ReadingScript,
        input: &ReadingInput,
        budget: ReadingBudget,
    ) -> Result<ReadingResult, ReadingError> {
        run(script, input, budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ReadingProvenanceV1, ReadingRowV1};

    struct FakeBackend;

    impl ReadingBackend for FakeBackend {
        fn descriptor(&self) -> ReadingBackendDescriptor {
            ReadingBackendDescriptor {
                id: "test.fake",
                version: "1",
                budget_support: BudgetSupport::ExternalIsolation,
            }
        }

        fn run(
            &mut self,
            script: &ReadingScript,
            input: &ReadingInput,
            budget: ReadingBudget,
        ) -> Result<ReadingResult, ReadingError> {
            Ok(ReadingResult {
                rows: vec![ReadingRowV1 {
                    label: input.text.clone(),
                    span: Some((0, input.text.len())),
                    document: None,
                }],
                notes: Vec::new(),
                provenance: ReadingProvenanceV1 {
                    source: input.source.clone(),
                    script_name: script.name.clone(),
                    script_hash: script.hash,
                    ops_used: 0,
                    elapsed_micros: 0,
                    budget,
                },
            })
        }
    }

    #[test]
    fn host_dispatches_a_typed_fake_without_rhai_values() {
        let input = ReadingInput::from_text("memory:fake", "djot", "one reading");
        let script = ReadingScript::new("fake", "unused");
        let budget = ReadingBudget::default();
        let mut backend: Box<dyn ReadingBackend> = Box::new(FakeBackend);
        assert_eq!(backend.descriptor().id, "test.fake");
        let result = backend.run(&script, &input, budget).unwrap();
        assert_eq!(result.rows[0].label, "one reading");
        assert_eq!(result.rows[0].span, Some((0, 11)));
        assert_eq!(result.provenance.source, input.source);
        assert_eq!(result.provenance.script_hash, script.hash);
    }

    #[test]
    fn rhai_adapter_retains_existing_budgeted_run() {
        let input = ReadingInput::from_text("memory:rhai", "djot", "hello");
        let script = ReadingScript::new("rhai", "row(\"ok\")");
        let mut backend = RhaiReadingBackend;
        assert_eq!(
            backend.descriptor().budget_support,
            BudgetSupport::NativeOperations
        );
        let result = backend
            .run(&script, &input, ReadingBudget::default())
            .unwrap();
        assert_eq!(result.rows[0].label, "ok");
    }
}
