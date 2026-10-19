#![cfg(feature = "x86")]
use fission_fsl::pipeline::{owned_x86_package, project_x86_binary, project_x86_entry};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "fsl-pipeline-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let mut b = vec![0; 512];
        b[..7].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1]);
        b[16..18].copy_from_slice(&2u16.to_le_bytes());
        b[18..20].copy_from_slice(&62u16.to_le_bytes());
        b[20..24].copy_from_slice(&1u32.to_le_bytes());
        b[24..32].copy_from_slice(&0x400100u64.to_le_bytes());
        b[32..40].copy_from_slice(&64u64.to_le_bytes());
        b[52..54].copy_from_slice(&64u16.to_le_bytes());
        b[54..56].copy_from_slice(&56u16.to_le_bytes());
        b[56..58].copy_from_slice(&1u16.to_le_bytes());
        b[64..68].copy_from_slice(&1u32.to_le_bytes());
        b[68..72].copy_from_slice(&5u32.to_le_bytes());
        b[80..88].copy_from_slice(&0x400000u64.to_le_bytes());
        b[96..104].copy_from_slice(&512u64.to_le_bytes());
        b[104..112].copy_from_slice(&512u64.to_le_bytes());
        b[112..120].copy_from_slice(&4096u64.to_le_bytes());
        b[256..260].copy_from_slice(&[0x48, 0x89, 0xf8, 0xc3]);
        fs::write(dir.join("input.elf"), b).unwrap();
        Self(dir)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn embedded_spec_to_source_bundle_uses_only_owned_pipeline() {
    let input = Fixture::new();
    for lang in ["c", "rust", "go"] {
        let result = project_x86_binary(&input.0.join("input.elf"), 0x400100, 4, lang).unwrap();
        assert!(result.pipeline_report.contains("legacy-fallback=none"));
        assert!(result.pipeline_report.contains("no legacy LoadSpec"));
        assert!(result.fir.contains("raw=4889f8"));
        assert_eq!(
            result.profile,
            owned_x86_package().unwrap().encode_binary().unwrap()
        );
        let out = input.0.join(lang);
        result.write_new(&out).unwrap();
        assert_eq!(fs::read(out.join("profile.fslxc")).unwrap(), result.profile);
        assert_eq!(fs::read_dir(&out).unwrap().count(), 6);
        assert!(result.write_new(&out).is_err());
    }
}
#[test]
fn unsupported_selection_and_bytes_refuse_before_output_creation() {
    let input = Fixture::new();
    let path = input.0.join("input.elf");
    assert!(project_x86_binary(&path, 0x400100, 4, "python").is_err());
    assert!(project_x86_binary(&path, 0x400100, 0, "c").is_err());
    assert!(project_x86_binary(&path, 0x400300, 4, "c").is_err());
    let mut b = fs::read(&path).unwrap();
    b[256] = 0xcc;
    fs::write(&path, b).unwrap();
    assert!(project_x86_binary(&path, 0x400100, 4, "c").is_err());
    assert_eq!(fs::read_dir(&input.0).unwrap().count(), 1);
}
#[test]
fn entry_selection_stops_at_ret_and_preserves_span_semantics() {
    let input = Fixture::new();
    let path = input.0.join("input.elf");
    let mut b = fs::read(&path).unwrap();
    b[260] = 0xcc; // Unsupported padding after RET must not become part of FIR.
    fs::write(&path, b).unwrap();
    for lang in ["c", "rust", "go"] {
        let entry = project_x86_entry(&path, 0x400100, lang).unwrap();
        let span = project_x86_binary(&path, 0x400100, 4, lang).unwrap();
        assert_eq!(entry.fir, span.fir);
        assert_eq!(entry.projection.sources, span.projection.sources);
        assert!(entry.pipeline_report.contains("size=4 bound=65536"));
        assert!(entry
            .pipeline_report
            .contains("source-function-boundary=unproven"));
    }
}
#[test]
fn entry_selection_refuses_branch_unsupported_and_unterminated_input() {
    let input = Fixture::new();
    let path = input.0.join("input.elf");
    let original = fs::read(&path).unwrap();
    for (bytes, reason) in [
        (vec![0x74, 0x00, 0xc3], "branch refused"),
        (vec![0xcc, 0xc3], "entry decode"),
        (vec![0x90; 256], "found no near RET"),
    ] {
        let mut b = original.clone();
        b[256..256 + bytes.len()].copy_from_slice(&bytes);
        fs::write(&path, b).unwrap();
        let err = project_x86_entry(&path, 0x400100, "c").unwrap_err();
        assert!(err.to_string().contains(reason), "{err}");
    }
    assert!(project_x86_entry(&path, 0x400300, "c").is_err());
    assert!(project_x86_entry(&path, 0x400100, "python").is_err());
}

#[test]
fn owned_catalog_selects_binds_and_projects_a_scalar_function() {
    use fission_fsl::{pipeline::project_profiled_x86_function, profiles::ProfilePackage};
    let fixture = Fixture::new();
    let path = fixture.0.join("input.elf");
    let text = include_str!("../specs/x86/elf.fslprofiles");
    let profiles = ProfilePackage::compile(text).unwrap();
    let p = project_profiled_x86_function(
        &profiles,
        &path,
        0x400100,
        4,
        "sysv-amd64-u64-leaf-v1",
        "recovered_input",
    )
    .unwrap();
    assert!(p.pipeline_report.contains("loader-route=owned-catalog"));
    assert!(p
        .pipeline_report
        .contains("selected-profile=fission.x86.elf64.scalar"));
    assert!(p.projection.report.contains("parameter-prefix=1"));
    assert!(p.projection.sources[0]
        .source
        .contains("uint64_t recovered_input(uint64_t arg0)"));
    for origin in &p.projection.origins {
        assert!(p.projection.sources[0]
            .source
            .contains(&origin.source_symbol));
    }
    p.write_new(&fixture.0.join("function")).unwrap();
    let bad = ProfilePackage::compile(&text.replace(
        "4f21befdc46ff60cb74913f0e2dea88b770cc1edbcb414590c99a72b90a67470",
        &"0".repeat(64),
    ))
    .unwrap();
    assert!(project_profiled_x86_function(
        &bad,
        &path,
        0x400100,
        4,
        "sysv-amd64-u64-leaf-v1",
        "recovered_bad"
    )
    .unwrap_err()
    .message
    .contains("binding mismatch"));
    let mut bytes = fs::read(&path).unwrap();
    bytes[18..20].copy_from_slice(&183u16.to_le_bytes());
    fs::write(&path, bytes).unwrap();
    assert!(project_profiled_x86_function(
        &profiles,
        &path,
        0x400100,
        4,
        "sysv-amd64-u64-leaf-v1",
        "recovered_bad"
    )
    .is_err());
}
