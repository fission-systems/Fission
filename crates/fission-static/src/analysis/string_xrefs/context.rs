//! Read-only navigation over the canonical reference index. No extraction or
//! second disassembly pass: every relationship retains its original evidence.

use crate::analysis::xref_index::{XrefId, XrefIndex, XrefKind, XrefRecord, XrefSourceCategory};
use crate::analysis::{CallEdge, CallGraph};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use std::collections::BTreeMap;

pub struct FunctionReferenceContext<'a> {
    pub function: &'a FunctionInfo,
    /// References that attributed this string to this function in the index.
    pub reference_ids: Vec<XrefId>,
    pub callers: &'a [CallEdge],
    pub callees: &'a [CallEdge],
}

pub struct StringReferenceContext<'a> {
    pub references: Vec<&'a XrefRecord>,
    pub functions: Vec<FunctionReferenceContext<'a>>,
}

/// Join references into an explicitly supplied string byte span with one-hop
/// call relationships from the same index. Loader string anchors are not uses.
/// Unknown enclosing functions and pointer-data uses remain references, never
/// invented functions. The caller supplies actual bytes, including a NUL only
/// when present, and carries `index.analysis` alongside the result.
pub fn string_reference_context<'a>(
    binary: &'a LoadedBinary,
    index: &'a XrefIndex,
    graph: &'a CallGraph,
    address: u64,
    byte_len: u64,
) -> StringReferenceContext<'a> {
    let references: Vec<_> = index
        .refs_to_span(address, byte_len)
        .into_iter()
        .filter(|record| {
            matches!(
                record.kind,
                XrefKind::DataRead | XrefKind::DataWrite | XrefKind::Relocation
            ) && !matches!(
                record.source.category,
                XrefSourceCategory::LoaderMetadata { .. }
            )
        })
        .collect();
    let mut functions: BTreeMap<u64, Vec<XrefId>> = BTreeMap::new();
    for record in &references {
        if let XrefSourceCategory::Instruction {
            enclosing_function: Some(entry),
        } = record.source.category
        {
            functions.entry(entry).or_default().push(record.id);
        }
    }
    let functions = functions
        .into_iter()
        .filter_map(|(entry, reference_ids)| {
            let function = binary.function_at_exact(entry)?;
            Some(FunctionReferenceContext {
                function,
                reference_ids,
                callers: graph.callers_of(entry),
                callees: graph.callees_of(entry),
            })
        })
        .collect();
    StringReferenceContext {
        references,
        functions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::xref_index::{
        XrefEvidence, XrefIndexBuilder, XrefSource, XrefSourceLayer, XrefTarget,
    };
    use fission_loader::{
        Confidence,
        loader::{DataBuffer, LoadedBinaryBuilder},
    };

    fn record(
        builder: &mut XrefIndexBuilder,
        source: u64,
        target: u64,
        kind: XrefKind,
        category: XrefSourceCategory,
    ) {
        builder.push_record(
            XrefSource {
                address: source,
                category,
            },
            XrefTarget {
                address: Some(target),
                symbol: None,
            },
            kind,
            Confidence::High,
            XrefEvidence {
                layer: XrefSourceLayer::Disassembly,
                instruction_mnemonic: None,
                pcode_op: None,
                relocation_kind: None,
                relocation_type: None,
                relocation_size: None,
                relocation_addend: None,
                symbol_name: None,
                note: Some("retained evidence".into()),
            },
        );
    }
    fn instruction(entry: Option<u64>) -> XrefSourceCategory {
        XrefSourceCategory::Instruction {
            enclosing_function: entry,
        }
    }
    fn binary() -> LoadedBinary {
        let mut builder =
            LoadedBinaryBuilder::new("test.bin".into(), DataBuffer::Heap(vec![0; 16]));
        for (address, name) in [(0x1000, "caller"), (0x2000, "reader"), (0x3000, "callee")] {
            builder = builder.add_function(FunctionInfo {
                address,
                name: name.into(),
                size: 0x100,
                ..Default::default()
            });
        }
        builder.build().unwrap()
    }

    #[test]
    fn context_keeps_suffix_evidence_and_joins_one_hop_calls() {
        let binary = binary();
        let mut builder = XrefIndexBuilder::new();
        record(
            &mut builder,
            0x1008,
            0x2000,
            XrefKind::Call,
            instruction(Some(0x1000)),
        );
        record(
            &mut builder,
            0x2008,
            0x3000,
            XrefKind::Call,
            instruction(Some(0x2000)),
        );
        record(
            &mut builder,
            0x2010,
            0x4002,
            XrefKind::DataRead,
            instruction(Some(0x2000)),
        );
        record(
            &mut builder,
            0x100a,
            0x4000,
            XrefKind::Jump,
            instruction(Some(0x1000)),
        );
        record(
            &mut builder,
            0x4000,
            0x4000,
            XrefKind::StringRef,
            XrefSourceCategory::LoaderMetadata {
                label: "string anchor".into(),
            },
        );
        record(
            &mut builder,
            0x5000,
            0x4004,
            XrefKind::DataWrite,
            XrefSourceCategory::Data {
                section: ".data".into(),
            },
        );
        let index = builder.finish();
        let graph = CallGraph::build_from_xref_index(&binary.functions, &index, 0x40);
        let context = string_reference_context(&binary, &index, &graph, 0x4000, 4);
        assert_eq!(context.references.len(), 1);
        assert_eq!(context.references[0].source.address, 0x2010);
        assert_eq!(
            context.references[0].evidence.note.as_deref(),
            Some("retained evidence")
        );
        assert_eq!(context.functions.len(), 1);
        let function = &context.functions[0];
        assert_eq!(function.function.address, 0x2000);
        assert_eq!(function.reference_ids, vec![context.references[0].id]);
        assert_eq!(function.callers[0].addr, 0x1000);
        assert_eq!(function.callees[0].addr, 0x3000);
        // The caller explicitly includes a real terminator byte; pointer data
        // remains evidence rather than being promoted into a function.
        let extended = string_reference_context(&binary, &index, &graph, 0x4000, 5);
        assert_eq!(extended.references.len(), 2);
        assert_eq!(extended.functions.len(), 1);
    }

    #[test]
    fn unknown_owners_and_loader_metadata_never_become_functions() {
        let binary = binary();
        let mut builder = XrefIndexBuilder::new();
        record(
            &mut builder,
            0x2010,
            0x4000,
            XrefKind::DataRead,
            instruction(None),
        );
        record(
            &mut builder,
            0x2018,
            0x4000,
            XrefKind::Relocation,
            XrefSourceCategory::LoaderMetadata {
                label: "anchor".into(),
            },
        );
        let index = builder.finish();
        let graph = CallGraph::build_from_xref_index(&binary.functions, &index, 0x40);
        let context = string_reference_context(&binary, &index, &graph, 0x4000, 4);
        assert_eq!(context.references.len(), 1);
        assert!(context.functions.is_empty());
    }

    #[test]
    fn span_query_preserves_record_order_and_never_wraps() {
        let mut builder = XrefIndexBuilder::new();
        record(
            &mut builder,
            8,
            u64::MAX,
            XrefKind::DataRead,
            instruction(None),
        );
        record(
            &mut builder,
            4,
            u64::MAX - 1,
            XrefKind::DataRead,
            instruction(None),
        );
        record(&mut builder, 1, 0, XrefKind::DataRead, instruction(None));
        let index = builder.finish();
        assert!(index.refs_to_span(u64::MAX, 0).is_empty());
        let refs = index.refs_to_span(u64::MAX - 1, 8);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].source.address, 4);
        assert_eq!(refs[1].source.address, 8);
        assert_eq!(index.refs_to_address(0).len(), 1);
    }
}
