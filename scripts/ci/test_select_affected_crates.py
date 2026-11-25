import unittest
from pathlib import Path

from select_affected_crates import CORE_FALLBACK, affected_packages

ROOT = Path("/repo")


def package(name, directory, deps=()):
    return {
        "name": name,
        "manifest_path": str(ROOT / directory / "Cargo.toml"),
        "dependencies": [{"name": dep} for dep in deps],
    }


# leaf <- mid <- top, plus an unrelated crate and the core fallback set.
METADATA = {
    "packages": [
        package("leaf", "crates/leaf"),
        package("mid", "crates/mid", ["leaf"]),
        package("top", "crates/top", ["mid"]),
        package("alone", "crates/alone"),
        package("xtask", "xtask"),
        *(package(name, f"crates/{name}") for name in sorted(CORE_FALLBACK)),
    ]
}


def select(*paths):
    return affected_packages(list(paths), METADATA, ROOT)


class AffectedPackagesTests(unittest.TestCase):
    def test_a_crate_change_selects_its_reverse_dependents(self):
        self.assertEqual(select("crates/leaf/src/lib.rs"), {"leaf", "mid", "top"})
        # Positive control for the closure: the top crate has no dependents.
        self.assertEqual(select("crates/top/src/lib.rs"), {"top"})

    def test_a_top_level_member_is_its_own_owner(self):
        self.assertEqual(select("xtask/src/main.rs"), {"xtask"})

    def test_workspace_files_select_every_package(self):
        everything = {p["name"] for p in METADATA["packages"]}
        self.assertEqual(select("Cargo.lock"), everything)
        self.assertEqual(select(".cargo/config.toml"), everything)
        self.assertEqual(select("crates/new-crate/src/lib.rs"), everything)

    def test_non_rust_paths_fall_back_to_the_core_floor(self):
        self.assertEqual(select("docs/x.md", "scripts/ci/clippy.sh"), CORE_FALLBACK)


if __name__ == "__main__":
    unittest.main()
