//! ADR 0020 fixtures: claims are content-addressed, additive and scoped.

mod common;

use common::{Image, at, plain, run};
use fission_evidence::{
    CodeLocation, CodeVersion, Entity, Evidence, EvidenceError, EvidenceLedger, InsertOutcome,
    LedgerError, Lifecycle, Predicate, Producer, Scope, SourceKind, StatusChange, StatusEvent,
    Value, VerificationStatus,
};

fn location(image: &Image, offset: usize) -> CodeLocation {
    CodeLocation {
        image: image.identity.clone(),
        offset: offset as u64,
        version: CodeVersion::of(image.at(offset, 4)).unwrap(),
        decode: plain(),
    }
}

fn producer() -> Producer {
    Producer {
        name: "fixture".into(),
        version: "1".into(),
    }
}

fn observed(image: &Image, run_label: &str, target: usize) -> Evidence {
    Evidence {
        entity: Entity::Code(location(image, 0x10)),
        predicate: Predicate::IndirectCallTarget,
        value: Value::Code(location(image, target)),
        source: SourceKind::RuntimeObservation,
        scope: Scope::Run {
            run: run(run_label),
            process: 1,
            thread: 1,
            epoch: 0,
        },
        producer: producer(),
        dependencies: Default::default(),
        assumptions: Default::default(),
    }
}

#[test]
fn a_second_run_adds_and_a_repeated_import_does_not() {
    let image = Image::new(0x10);
    let mut ledger = EvidenceLedger::default();
    assert_eq!(
        ledger.insert(observed(&image, "one", 0x40)).unwrap(),
        InsertOutcome::Added
    );
    let after_first = ledger.to_json().unwrap();
    assert_eq!(
        ledger.insert(observed(&image, "one", 0x40)).unwrap(),
        InsertOutcome::Duplicate
    );
    assert_eq!(ledger.to_json().unwrap(), after_first);

    assert_eq!(
        ledger.insert(observed(&image, "two", 0x80)).unwrap(),
        InsertOutcome::Added
    );
    assert_eq!(ledger.len(), 2);
    assert!(
        ledger
            .claims()
            .any(|(_, c)| *c == observed(&image, "one", 0x40))
    );
}

#[test]
fn serialized_form_does_not_depend_on_insertion_order() {
    let image = Image::new(0x10);
    let mut forward = EvidenceLedger::default();
    let mut backward = EvidenceLedger::default();
    for target in [0x40, 0x80, 0xc0] {
        forward.insert(observed(&image, "run", target)).unwrap();
    }
    for target in [0xc0, 0x80, 0x40] {
        backward.insert(observed(&image, "run", target)).unwrap();
    }
    let text = forward.to_json().unwrap();
    assert_eq!(text, backward.to_json().unwrap());
    assert_eq!(EvidenceLedger::from_json(&text).unwrap(), forward);
}

/// Pins the claim encoding. A change here renames every stored claim, so it
/// needs a new ledger schema, not an edited constant.
#[test]
fn claim_ids_are_pinned() {
    let image = Image::new(0x10);
    assert_eq!(
        observed(&image, "one", 0x40).id().to_string(),
        "blake3:8c137daef136a236a85b563131f4f3452ca6d7fb9204ff82f621afc2605a68ad"
    );
}

#[test]
fn withdrawal_and_verification_are_separate_and_keep_the_claim() {
    let image = Image::new(0x10);
    let claim = observed(&image, "one", 0x40);
    let id = claim.id();
    let mut ledger = EvidenceLedger::default();
    ledger.insert(claim.clone()).unwrap();
    assert_eq!(ledger.lifecycle(&id), Lifecycle::Active);
    assert_eq!(ledger.verification(&id), VerificationStatus::Unchecked);

    let event = |change| StatusEvent {
        target: id.clone(),
        change,
        by: producer(),
    };
    ledger
        .record(event(StatusChange::Verification(
            VerificationStatus::ReplayValidated,
        )))
        .unwrap();
    let withdrawn = Lifecycle::Withdrawn {
        reason: "producer defect".into(),
    };
    ledger
        .record(event(StatusChange::Lifecycle(withdrawn.clone())))
        .unwrap();
    assert_eq!(
        ledger
            .record(event(StatusChange::Lifecycle(withdrawn.clone())))
            .unwrap(),
        InsertOutcome::Duplicate
    );

    assert_eq!(ledger.lifecycle(&id), withdrawn);
    assert_eq!(
        ledger.verification(&id),
        VerificationStatus::ReplayValidated
    );
    assert_eq!(ledger.history().len(), 2);
    assert_eq!(ledger.claims().count(), 1);
    let reopened = EvidenceLedger::from_json(&ledger.to_json().unwrap()).unwrap();
    assert_eq!(reopened, ledger);
}

#[test]
fn an_observation_cannot_claim_the_whole_image() {
    let image = Image::new(0x10);
    let mut claim = observed(&image, "one", 0x40);
    claim.scope = Scope::Image;
    assert!(matches!(
        EvidenceLedger::default().insert(claim),
        Err(LedgerError::Evidence(EvidenceError::UnscopedObservation))
    ));
}

#[test]
fn runtime_values_stay_inside_their_run() {
    let image = Image::new(0x10);
    let mut claim = observed(&image, "one", 0x40);
    claim.value = Value::Address {
        at: at("two", 0, 0x1234),
        reason: fission_evidence::UnattributedReason::Unmapped,
    };
    assert_eq!(claim.validate(), Err(EvidenceError::ScopeMismatch));
}

#[test]
fn an_edited_or_foreign_ledger_is_refused() {
    let image = Image::new(0x10);
    let mut ledger = EvidenceLedger::default();
    ledger.insert(observed(&image, "one", 0x40)).unwrap();
    let text = ledger.to_json().unwrap();

    let edited = text.replace("\"offset\": 64", "\"offset\": 65");
    assert_ne!(edited, text);
    assert!(matches!(
        EvidenceLedger::from_json(&edited),
        Err(LedgerError::IdMismatch { .. })
    ));
    let newer = text.replace("evidence-ledger-v1", "evidence-ledger-v9");
    assert!(matches!(
        EvidenceLedger::from_json(&newer),
        Err(LedgerError::Schema { .. })
    ));
}
