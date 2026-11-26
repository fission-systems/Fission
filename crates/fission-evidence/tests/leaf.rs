//! ADR 0020: every engine may depend on this crate, so it depends on none.

#[test]
fn the_contract_crate_has_no_workspace_dependency() {
    let manifest = include_str!("../Cargo.toml");
    let edges: Vec<&str> = manifest
        .lines()
        .filter(|line| line.trim_start().starts_with("fission-") || line.contains("path = \"../"))
        .collect();
    assert_eq!(edges, Vec::<&str>::new());
}
