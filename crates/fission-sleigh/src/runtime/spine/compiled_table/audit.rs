/// SLA template feature audit (cross-build, delay INDIRECT, flow consts).

use crate::compiler::{
    CompiledConstTpl, CompiledFrontend, CompiledOpTpl, CompiledOpTplOpcode, CompiledSpaceTpl,
    CompiledVarnodeTpl,
};

/// Counts cross-build / delay-slot / flow-const usage across all `.sla`-lowered constructor templates.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct SlaTemplateFeatureAudit {
    pub opcode_cross_build: u64,
    pub opcode_delay_slot_indirect: u64,
    pub const_flow_ref: u64,
    pub const_flow_ref_size: u64,
    pub const_flow_dest: u64,
    pub const_flow_dest_size: u64,
}

/// Scan `compiled` subtable constructors for template features that need Ghidra `PcodeEmit` parity.
pub fn audit_sla_template_features(compiled: &CompiledFrontend) -> SlaTemplateFeatureAudit {
    let mut audit = SlaTemplateFeatureAudit::default();
    for sub in compiled.subtables.values() {
        for ctor in &sub.constructors {
            audit_construct_tpl_ops(&ctor.constructor_template.ops, &mut audit);
            for named in &ctor.named_templates {
                if let Some(tpl) = named {
                    audit_construct_tpl_ops(&tpl.ops, &mut audit);
                }
            }
        }
    }
    audit
}

fn audit_construct_tpl_ops(ops: &[CompiledOpTpl], audit: &mut SlaTemplateFeatureAudit) {
    for op in ops {
        match op.opcode {
            CompiledOpTplOpcode::CrossBuild => audit.opcode_cross_build += 1,
            CompiledOpTplOpcode::DelaySlotIndirect => audit.opcode_delay_slot_indirect += 1,
            _ => {}
        }
        if let Some(out) = &op.output {
            audit_varnode_tpl_flow_consts(out, audit);
        }
        for inp in &op.inputs {
            audit_varnode_tpl_flow_consts(inp, audit);
        }
    }
}

fn audit_varnode_tpl_flow_consts(vn: &CompiledVarnodeTpl, audit: &mut SlaTemplateFeatureAudit) {
    match vn {
        CompiledVarnodeTpl::Const(c) => audit_const_tpl_flow(c, audit),
        CompiledVarnodeTpl::Varnode { space, offset, size } => {
            if let CompiledSpaceTpl::Const(c) = space {
                audit_const_tpl_flow(c, audit);
            }
            audit_const_tpl_flow(offset, audit);
            audit_const_tpl_flow(size, audit);
        }
        CompiledVarnodeTpl::HandleTpl(ht) => {
            if let Some(s) = &ht.size {
                audit_const_tpl_flow(s, audit);
            }
            if let Some(o) = &ht.ptr_offset {
                audit_const_tpl_flow(o, audit);
            }
            if let Some(o) = &ht.temp_offset {
                audit_const_tpl_flow(o, audit);
            }
        }
        _ => {}
    }
}

fn audit_const_tpl_flow(ct: &CompiledConstTpl, audit: &mut SlaTemplateFeatureAudit) {
    match ct {
        CompiledConstTpl::FlowRef => audit.const_flow_ref += 1,
        CompiledConstTpl::FlowRefSize => audit.const_flow_ref_size += 1,
        CompiledConstTpl::FlowDest => audit.const_flow_dest += 1,
        CompiledConstTpl::FlowDestSize => audit.const_flow_dest_size += 1,
        _ => {}
    }
}
