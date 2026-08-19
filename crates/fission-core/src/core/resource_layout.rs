//! Where each kind of bundled resource lives, in one place.
//!
//! Directory names used to be spelled inline at every accessor -- `join("typeinfo")`
//! 24 times, `join("signatures")` 18 -- so the layout was a convention spread
//! across a thousand lines rather than something stated once. Moving a
//! directory meant finding every spelling of it.
//!
//! The split is between what ships and what does not, which is the distinction
//! the bundle has to make and the one the old tree could not express:
//!
//! ```text
//! utils/
//!   packed/    the .fpk the runtime reads
//!   source/    what the packers read; never shipped
//!   ...        everything else stays where it was
//! ```
//!
//! Only the packed tables and their sources moved. `sleigh-specs/`,
//! `ghidra-data/`, `signatures/die` and the rest stayed: their paths reach into
//! build scripts and a dozen call sites, and moving them buys nothing the
//! bundle needs, since none of them has a packed form to be confused with.
//!

use std::path::{Path, PathBuf};

/// A packer input, kept out of the shipped bundle.
///
/// Only the inputs moved. The `.fpk` the runtime reads stayed beside the data
/// it belongs with, because the loaders find them as siblings -- a signature
/// table's `.fpk` is `path.with_extension("fpk")`, a Go snapshot's is
/// `go1.X.{fn,ty}.fpk` next to `go1.X.json` -- and breaking that to group them
/// by lifecycle would have meant teaching every loader a second location.
///
/// What the split does buy is the bundle rule: exclude `source/`, rather than
/// list extensions and pair each source with the `.fpk` that replaces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    /// `.fidbf` databases, read by `bin/pack_fid`.
    SourceFid,
    /// Ghidra's Java-packed `.fidb`, unreachable at runtime.
    SourceFidbJava,
    /// `.gdt` archives, read by the extractors under `scripts/`.
    SourceGdt,
    /// `*_ordinals.json`, read by `scripts/pack_ordinals.py`.
    SourceOrdinals,
    /// `go1.X.json` and `*_signatures.txt`, read by their packers.
    SourceTypeinfo,
}

impl ResourceKind {
    /// Path under `root`, the `utils/` directory.
    #[must_use]
    pub fn dir(self, root: &Path) -> PathBuf {
        let source = root.join("source");
        match self {
            Self::SourceFid => source.join("fid"),
            Self::SourceFidbJava => source.join("fidb_java"),
            Self::SourceGdt => source.join("gdt"),
            Self::SourceOrdinals => source.join("ordinals"),
            Self::SourceTypeinfo => source.join("typeinfo"),
        }
    }

    /// `dir/filename`, if it exists.
    #[must_use]
    pub fn resolve_file(self, root: &Path, filename: &str) -> Option<PathBuf> {
        let path = self.dir(root).join(filename);
        path.exists().then_some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_source_kind_lands_under_source() {
        // The bundle excludes `source/` wholesale, so a kind that resolved
        // anywhere else would be shipped by accident.
        let root = Path::new("/utils");
        for kind in [
            ResourceKind::SourceFid,
            ResourceKind::SourceFidbJava,
            ResourceKind::SourceGdt,
            ResourceKind::SourceOrdinals,
            ResourceKind::SourceTypeinfo,
        ] {
            assert!(
                kind.dir(root).starts_with("/utils/source"),
                "{kind:?} escapes source/"
            );
        }
    }
}
