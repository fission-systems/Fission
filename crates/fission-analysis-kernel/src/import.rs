//! Runtime trace to evidence claims.

use crate::KernelError;
use crate::image::ImageView;
use fission_evidence::{
    ArtifactRef, Attribution, DecodeContext, Dependency, Entity, Evidence, EvidenceLedger,
    ImageBytes, InsertOutcome, MappingTable, Predicate, RUNTIME_TRACE_SCHEMA, RunId,
    RuntimeLocation, RuntimeTrace, Scope, SourceKind, TransferKind, Value, attribute,
};
use std::collections::BTreeSet;

/// What importing one trace did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub run: RunId,
    pub added: usize,
    /// Claims the ledger already held.
    pub duplicate: usize,
    /// Transfers whose site could not be attributed to any code, so there is
    /// nothing to make a claim about.
    pub unattributed_sites: usize,
    /// The trace recorded everything up to the guest's own exit.
    pub complete: bool,
}

/// Add a trace's observations to `ledger`.
///
/// The trace must name this exact image. Every address is attributed here,
/// against the binary's bytes; whatever cannot be validated is claimed as
/// runtime code or a bare address, never as the image's code. Nothing is added
/// unless the whole trace is acceptable.
pub fn import_trace(
    image: &ImageView<'_>,
    trace: &RuntimeTrace,
    ledger: &mut EvidenceLedger,
) -> Result<ImportReport, KernelError> {
    if trace.schema != RUNTIME_TRACE_SCHEMA {
        return Err(fission_evidence::TraceError::Schema {
            found: trace.schema.clone(),
        }
        .into());
    }
    if &trace.image != image.identity() {
        let name = |identity: &fission_evidence::ImageIdentity| {
            format!("{} ({})", identity.content, identity.spec.language_id)
        };
        return Err(KernelError::IncompatibleImage {
            expected: name(image.identity()),
            found: name(&trace.image),
        });
    }
    let mappings = MappingTable::new(trace.mappings.iter().cloned())?;
    let run = trace.run_id();
    let mut assumptions = BTreeSet::new();
    if !trace.completeness.is_complete() {
        assumptions.insert(format!(
            "capture incomplete: {} ({} transfers dropped)",
            trace.completeness.stop_reason, trace.completeness.dropped_transfers
        ));
    }

    let mut claims = Vec::with_capacity(trace.transfers.len());
    let mut unattributed_sites = 0;
    for (record, transfer) in trace.transfers.iter().enumerate() {
        if transfer.count == 0 {
            return Err(KernelError::EmptyTransfer { record });
        }
        let at = |address| RuntimeLocation {
            run: run.clone(),
            process: transfer.process,
            thread: transfer.thread,
            epoch: transfer.epoch,
            address,
        };
        let entity = match attribute(
            at(transfer.site),
            &transfer.site_bytes.0,
            transfer.decode.clone(),
            &mappings,
            image,
        ) {
            Attribution::Image(location) => Entity::Code(location),
            Attribution::RuntimeCode(code) => Entity::RuntimeCode(code),
            Attribution::Unattributed { .. } => {
                unattributed_sites += 1;
                continue;
            }
        };
        let value = match attribute(
            at(transfer.target),
            &transfer.target_bytes.0,
            DecodeContext::default(),
            &mappings,
            image,
        ) {
            Attribution::Image(location) => Value::Code(location),
            Attribution::RuntimeCode(code) => Value::RuntimeCode(code),
            Attribution::Unattributed { at, reason } => Value::Address { at, reason },
        };
        claims.push(Evidence {
            entity,
            predicate: match transfer.kind {
                TransferKind::IndirectCall => Predicate::IndirectCallTarget,
                TransferKind::IndirectJump => Predicate::IndirectJumpTarget,
            },
            value,
            source: SourceKind::RuntimeObservation,
            scope: Scope::Run {
                run: run.clone(),
                process: transfer.process,
                thread: transfer.thread,
                epoch: transfer.epoch,
            },
            producer: trace.producer.clone(),
            dependencies: BTreeSet::from([Dependency::Artifact(ArtifactRef {
                schema: trace.schema.clone(),
                digest: run.0.clone(),
                record: Some(record as u64),
            })]),
            assumptions: assumptions.clone(),
        });
    }
    // Validate every claim before the first insertion.
    for claim in &claims {
        claim
            .validate()
            .map_err(fission_evidence::LedgerError::from)?;
    }
    let mut report = ImportReport {
        run,
        added: 0,
        duplicate: 0,
        unattributed_sites,
        complete: trace.completeness.is_complete(),
    };
    for claim in claims {
        match ledger.insert(claim)? {
            InsertOutcome::Added => report.added += 1,
            InsertOutcome::Duplicate => report.duplicate += 1,
        }
    }
    Ok(report)
}
