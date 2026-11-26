//! ADR 0020 fixtures: ASLR, remapping and modified code.

mod common;

use common::{Image, at, plain};
use fission_evidence::{
    Attribution, CodeLocation, DecodeContext, Mapping, MappingBacking, MappingError, MappingTable,
    RuntimeCodeOrigin, UnattributedReason, attribute,
};

fn code(attribution: Attribution) -> CodeLocation {
    match attribution {
        Attribution::Image(location) => location,
        other => panic!("expected image code, got {other:?}"),
    }
}

#[test]
fn two_load_addresses_of_one_image_name_the_same_code() {
    let image = Image::new(0x10);
    let low = MappingTable::new([image.mapped_at(0x5555_0000_0000, 0)]).unwrap();
    let high = MappingTable::new([image.mapped_at(0x7f12_3456_0000, 0)]).unwrap();

    let first = code(attribute(
        at("aslr-1", 0, 0x5555_0000_0040),
        image.at(0x40, 4),
        plain(),
        &low,
        &image,
    ));
    let second = code(attribute(
        at("aslr-2", 0, 0x7f12_3456_0040),
        image.at(0x40, 4),
        plain(),
        &high,
        &image,
    ));

    assert_eq!(first, second);
    assert_eq!(first.offset, 0x40);
}

#[test]
fn a_segment_mapping_is_relative_to_its_image_offset() {
    let image = Image::new(0x10);
    let segment = Mapping {
        start: 0x9000,
        size: 0x40,
        from_epoch: 0,
        until_epoch: None,
        backing: MappingBacking::Image {
            image: image.identity.clone(),
            image_offset: 0x80,
        },
    };
    let maps = MappingTable::new([segment]).unwrap();
    let location = code(attribute(
        at("segment", 0, 0x9010),
        image.at(0x90, 2),
        plain(),
        &maps,
        &image,
    ));
    assert_eq!(location.offset, 0x90);

    let straddling = attribute(
        at("segment", 0, 0x903f),
        image.at(0xbf, 2),
        plain(),
        &maps,
        &image,
    );
    assert!(matches!(
        straddling,
        Attribution::Unattributed {
            reason: UnattributedReason::CrossesMapping,
            ..
        }
    ));
}

#[test]
fn the_same_address_after_a_remap_belongs_to_the_new_image() {
    let first = Image::new(0x10);
    let second = Image::new(0x80);
    let base = 0x7000_0000;
    let mut maps = MappingTable::new([first.mapped_at(base, 0)]).unwrap();

    assert_eq!(
        maps.map(second.mapped_at(base + 0x80, 0)),
        Err(MappingError::Overlap {
            start: base + 0x80,
            other: base
        }),
        "two images cannot share an address in one epoch"
    );

    maps.unmap(base, 2).unwrap();
    maps.map(second.mapped_at(base, 3)).unwrap();

    let before = code(attribute(
        at("reload", 1, base + 8),
        first.at(8, 4),
        plain(),
        &maps,
        &first,
    ));
    assert_eq!(before.image, first.identity);

    let between = attribute(
        at("reload", 2, base + 8),
        first.at(8, 4),
        plain(),
        &maps,
        &first,
    );
    assert!(matches!(
        between,
        Attribution::Unattributed {
            reason: UnattributedReason::Unmapped,
            ..
        }
    ));

    // After the reload the old image must not claim the address, even though it
    // is the one being analysed and the observed bytes are offered to it.
    let foreign = attribute(
        at("reload", 3, base + 8),
        first.at(8, 4),
        plain(),
        &maps,
        &first,
    );
    assert!(matches!(
        foreign,
        Attribution::Unattributed {
            reason: UnattributedReason::ForeignImage,
            ..
        }
    ));
    let after = code(attribute(
        at("reload", 3, base + 8),
        second.at(8, 4),
        plain(),
        &maps,
        &second,
    ));
    assert_eq!(after.image, second.identity);
}

#[test]
fn modified_code_is_runtime_code_and_never_the_files() {
    let image = Image::new(0x10);
    let maps = MappingTable::new([image.mapped_at(0x40_0000, 0)]).unwrap();
    let mut patched = image.at(0x20, 4).to_vec();
    patched[1] ^= 0xff;

    let original = code(attribute(
        at("smc", 0, 0x40_0020),
        image.at(0x20, 4),
        plain(),
        &maps,
        &image,
    ));
    let rewritten = attribute(at("smc", 0, 0x40_0020), &patched, plain(), &maps, &image);

    let Attribution::RuntimeCode(runtime) = rewritten else {
        panic!("patched bytes were attributed to the image: {rewritten:?}");
    };
    assert_eq!(
        runtime.origin,
        RuntimeCodeOrigin::ModifiedImage {
            image: image.identity.clone(),
            offset: 0x20
        }
    );
    assert_ne!(runtime.version, original.version);
    assert!(original.version.matches(image.at(0x20, 4)));
    assert!(!original.version.matches(&patched));
}

#[test]
fn each_generation_of_anonymous_code_is_its_own_entity() {
    let image = Image::new(0x10);
    let jit = Mapping {
        start: 0x2000_0000,
        size: 0x1000,
        from_epoch: 0,
        until_epoch: None,
        backing: MappingBacking::Anonymous,
    };
    let maps = MappingTable::new([jit]).unwrap();
    let observe =
        |bytes: &[u8]| match attribute(at("jit", 0, 0x2000_0000), bytes, plain(), &maps, &image) {
            Attribution::RuntimeCode(code) => code,
            other => panic!("anonymous code was attributed: {other:?}"),
        };

    // Even bytes identical to the image's stay runtime code: no mapping says
    // this memory shows the image.
    let first = observe(image.at(0, 8));
    let second = observe(&[0x90; 8]);
    assert_eq!(first.origin, RuntimeCodeOrigin::Anonymous);
    assert_ne!(first, second);
    assert_eq!(first, observe(image.at(0, 8)));
}

#[test]
fn unmapped_and_unreadable_addresses_stay_unattributed() {
    let image = Image::new(0x10);
    let maps = MappingTable::new([image.mapped_at(0x40_0000, 0)]).unwrap();
    let reason = |attribution| match attribution {
        Attribution::Unattributed { reason, .. } => reason,
        other => panic!("attributed: {other:?}"),
    };
    assert_eq!(
        reason(attribute(
            at("gap", 0, 0x50_0000),
            &[1],
            plain(),
            &maps,
            &image
        )),
        UnattributedReason::Unmapped
    );
    assert_eq!(
        reason(attribute(
            at("gap", 0, 0x40_0000),
            &[],
            plain(),
            &maps,
            &image
        )),
        UnattributedReason::NoBytes
    );
}

#[test]
fn another_decode_spec_or_context_is_another_location() {
    let image = Image::new(0x10);
    let mut other_spec = Image::new(0x10);
    other_spec.identity.spec.compiler_spec_id = Some("windows".into());
    let maps = MappingTable::new([image.mapped_at(0x40_0000, 0)]).unwrap();

    let under_other_spec = attribute(
        at("spec", 0, 0x40_0000),
        image.at(0, 4),
        plain(),
        &maps,
        &other_spec,
    );
    assert!(matches!(
        under_other_spec,
        Attribution::Unattributed {
            reason: UnattributedReason::ForeignImage,
            ..
        }
    ));

    let default_mode = code(attribute(
        at("spec", 0, 0x40_0000),
        image.at(0, 4),
        plain(),
        &maps,
        &image,
    ));
    let other_mode = code(attribute(
        at("spec", 0, 0x40_0000),
        image.at(0, 4),
        DecodeContext {
            context_register: Some(1),
        },
        &maps,
        &image,
    ));
    assert_ne!(default_mode, other_mode);
}

#[test]
fn a_mapping_that_wraps_or_lives_nowhere_is_refused() {
    let image = Image::new(0x10);
    assert!(matches!(
        MappingTable::new([image.mapped_at(u64::MAX - 4, 0)]),
        Err(MappingError::InvalidRange { .. })
    ));
    let mut empty_life = image.mapped_at(0x1000, 5);
    empty_life.until_epoch = Some(5);
    assert!(matches!(
        MappingTable::new([empty_life]),
        Err(MappingError::InvalidEpochs { .. })
    ));
}
