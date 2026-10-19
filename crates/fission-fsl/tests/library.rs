use fission_fsl::library::{LibraryCatalog, ParameterForm, VariadicEvidence};

const FIXTURE: &[u8] = include_bytes!("../specs/library-candidates.fsldb");

#[test]
fn owned_library_preserves_candidates_and_uncertainty() {
    let catalog = LibraryCatalog::decode_binary(FIXTURE).unwrap();
    assert_eq!(catalog.candidates.len(), 3);
    let alpha = catalog.lookup("alpha").unwrap();
    assert_eq!(alpha.parameter_form, ParameterForm::DeclaredEmpty);
    assert_eq!(alpha.variadic, VariadicEvidence::Unknown);
    assert!(alpha.parameters.is_empty());
    let beta = catalog.lookup("beta").unwrap();
    assert_eq!(beta.return_spelling, "T*");
    assert_eq!(beta.parameters[0].type_spelling, "T*");
    assert_eq!(beta.variadic, VariadicEvidence::Unknown);
    assert_eq!(
        catalog.lookup("gamma").unwrap().variadic,
        VariadicEvidence::Explicit
    );
    assert!(catalog.lookup("bet").is_none());
}

#[test]
fn owned_library_refuses_truncation_and_unknown_schema() {
    for end in 0..FIXTURE.len() {
        assert!(LibraryCatalog::decode_binary(&FIXTURE[..end]).is_err());
    }
    let mut trailing = FIXTURE.to_vec();
    trailing.push(0);
    assert!(LibraryCatalog::decode_binary(&trailing).is_err());
    for position in [0, 4, 6] {
        let mut changed = FIXTURE.to_vec();
        changed[position] = 255;
        assert!(LibraryCatalog::decode_binary(&changed).is_err());
    }
    let mut changed = FIXTURE.to_vec();
    changed[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(LibraryCatalog::decode_binary(&changed).is_err());
    for (needle, replacement) in [
        (b"beta".as_slice(), b"aaaa".as_slice()),
        (b"alpha".as_slice(), b"\nlpha".as_slice()),
        (b"alpha".as_slice(), b"\xfflpha".as_slice()),
        (
            b"pipe-signatures-v1".as_slice(),
            b"pipe-signatures-v2".as_slice(),
        ),
    ] {
        let position = FIXTURE
            .windows(needle.len())
            .position(|s| s == needle)
            .unwrap();
        let mut changed = FIXTURE.to_vec();
        changed[position..position + needle.len()].copy_from_slice(replacement);
        assert!(LibraryCatalog::decode_binary(&changed).is_err());
    }
}
