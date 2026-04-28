use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use fission_pcode::PcodeOpcode;
use flate2::read::ZlibDecoder;

use super::ir::{
    CompiledConstTpl, CompiledConstructTpl, CompiledContextOp, CompiledDecisionLeafEntry,
    CompiledDisjointPattern, CompiledDisplayOperand, CompiledDisplayOperandKind,
    CompiledDisplayPiece, CompiledDisplayTemplate, CompiledHandleSelector, CompiledHandleTpl,
    CompiledLabelRef, CompiledOpTpl, CompiledOpTplOpcode, CompiledOperandSpec,
    CompiledPatternBlock, CompiledPatternExpression, CompiledResolvedVarnode, CompiledSpaceRef,
    CompiledSpaceTpl, CompiledTemplateSource, CompiledVarnodeTpl,
};

pub const GHIDRA_SLA_MAGIC: &[u8; 3] = b"sla";
pub const GHIDRA_SLA_FORMAT_VERSION: u8 = 4;

include!("symbols.rs");
include!("display.rs");
include!("templates.rs");
include!("packed.rs");

#[cfg(test)]
mod tests;
pub fn load_compiled_sla(path: impl AsRef<Path>) -> Result<CompiledSlaArtifact> {
    let path = path.as_ref();
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read compiled SLEIGH artifact {path:?}"))?;
    decode_compiled_sla(path.to_path_buf(), &bytes)
}

pub fn load_construct_templates_from_sla(
    path: impl AsRef<Path>,
) -> Result<CompiledSlaTemplateLibrary> {
    let artifact = load_compiled_sla(path)?;
    decode_construct_templates(&artifact)
}

fn decode_compiled_sla(path: PathBuf, bytes: &[u8]) -> Result<CompiledSlaArtifact> {
    if bytes.len() < 5 {
        return Err(anyhow!("compiled SLEIGH artifact is too short: {path:?}"));
    }
    if &bytes[..3] != GHIDRA_SLA_MAGIC {
        return Err(anyhow!(
            "compiled SLEIGH artifact missing sla magic: {path:?}"
        ));
    }
    let version = bytes[3];
    let mut decoder = ZlibDecoder::new(&bytes[4..]);
    let mut payload = Vec::new();
    decoder
        .read_to_end(&mut payload)
        .with_context(|| format!("failed to decompress compiled SLEIGH payload {path:?}"))?;
    if payload.is_empty() {
        return Err(anyhow!("compiled SLEIGH payload is empty: {path:?}"));
    }
    Ok(CompiledSlaArtifact {
        path,
        version,
        payload,
    })
}
