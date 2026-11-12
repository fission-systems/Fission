//! `fission_cli skill` prints the agent guide that is compiled into the binary.

use std::process::Command;

#[test]
fn skill_prints_the_guide_in_the_repository() {
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .arg("skill")
        .env_remove("FISSION_CONTRACT")
        .output()
        .expect("run fission_cli skill");
    assert_eq!(output.status.code(), Some(0));

    let on_disk = std::fs::read_to_string(format!(
        "{}/../../skills/fission/SKILL.md",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("read skills/fission/SKILL.md");
    assert_eq!(String::from_utf8_lossy(&output.stdout), on_disk);
    assert!(on_disk.starts_with("---\nname: fission-cli\n"));
}
