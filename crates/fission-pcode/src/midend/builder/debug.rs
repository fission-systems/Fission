use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn debug_lowering_error(
        &self,
        stage: &str,
        block_addr: u64,
        seq: u64,
        opcode: PcodeOpcode,
        err: &MlilPreviewError,
    ) {
        if preview_debug_enabled() {
            let message = format!(
                "[mlil-preview] stage={} block=0x{:x} seq=0x{:x} opcode={:?} err={}",
                stage, block_addr, seq, opcode, err
            );
            eprintln!("{message}");
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(format!(
                    "/tmp/fission_preview_{:x}.log",
                    self.function_address()
                ))
                .and_then(|mut f| {
                    std::io::Write::write_all(&mut f, format!("{message}\n").as_bytes())
                });
        }

        if matches!(err, MlilPreviewError::UnsupportedPattern("opcode")) {
            self.record_unsupported_inventory_event(
                stage,
                None,
                None,
                Some(opcode),
                Some(block_addr),
                Some(seq),
                true,
                "builder_root",
            );
        }
    }

    fn function_address(&self) -> u64 {
        self.pcode
            .blocks
            .first()
            .map(|block| block.start_address)
            .unwrap_or_default()
    }

    pub(super) fn preview_log_path(&self) -> String {
        format!("/tmp/fission_preview_{:x}.log", self.function_address())
    }

    fn unsupported_inventory_path(&self) -> String {
        format!(
            "/tmp/fission_preview_{:x}_unsupported.json",
            self.function_address()
        )
    }

    pub(super) fn next_trace_id(&mut self) -> u64 {
        let trace_id = self.next_trace_id;
        self.next_trace_id += 1;
        trace_id
    }

    fn inventory_trace_id(&self) -> Option<u64> {
        self.active_trace_id.or(self.last_trace_id)
    }

    fn format_varnode(&self, vn: &Varnode) -> String {
        format!(
            "space={} off=0x{:x} size={} const={} val={}",
            vn.space_id, vn.offset, vn.size, vn.is_constant, vn.constant_val
        )
    }

    fn format_op_snippet(&self, op: &PcodeOp) -> String {
        let output = op
            .output
            .as_ref()
            .map(|vn| self.format_varnode(vn))
            .unwrap_or_else(|| "<none>".to_string());
        let inputs = op
            .inputs
            .iter()
            .map(|vn| self.format_varnode(vn))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "addr=0x{:x} seq=0x{:x} opcode={:?} out={} inputs=[{}] asm={}",
            op.address,
            op.seq_num,
            op.opcode,
            output,
            inputs,
            op.asm_mnemonic.as_deref().unwrap_or("<none>")
        )
    }

    pub(super) fn debug_branch_target_resolution_failure(
        &self,
        stage: &str,
        block_idx: usize,
        block_addr: u64,
        op: &PcodeOp,
        target_vn: &Varnode,
        succ_addrs: &[u64],
    ) {
        let guessed_target = branch_target_address(target_vn)
            .map(|v| format!("0x{v:x}"))
            .unwrap_or_else(|| "<none>".to_string());
        let target_fmt = self.format_varnode(target_vn);
        let succ_fmt = if succ_addrs.is_empty() {
            "<none>".to_string()
        } else {
            succ_addrs
                .iter()
                .map(|addr| format!("0x{addr:x}"))
                .collect::<Vec<_>>()
                .join(",")
        };

        if preview_builder_diag_enabled() {
            eprintln!(
                "[DIAG] stage={} block_idx={} block=0x{:x} seq=0x{:x} opcode={:?} target={} guessed_target={} succs=[{}]",
                stage,
                block_idx,
                block_addr,
                op.seq_num,
                op.opcode,
                target_fmt,
                guessed_target,
                succ_fmt
            );
        }

        if preview_debug_enabled() {
            let message = format!(
                "[mlil-preview] stage={} block_idx={} block=0x{:x} seq=0x{:x} opcode={:?} target={} guessed_target={} succs=[{}]",
                stage,
                block_idx,
                block_addr,
                op.seq_num,
                op.opcode,
                target_fmt,
                guessed_target,
                succ_fmt
            );
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.preview_log_path())
                .and_then(|mut f| {
                    std::io::Write::write_all(&mut f, format!("{message}\n").as_bytes())
                });

            self.record_unsupported_inventory_event(
                stage,
                Some(target_vn),
                Some(op),
                Some(op.opcode),
                Some(block_addr),
                Some(u64::from(op.seq_num)),
                true,
                "branch_target_resolve",
            );
        }
    }

    pub(crate) fn record_unsupported_inventory_event(
        &self,
        stage: &str,
        vn: Option<&Varnode>,
        op: Option<&PcodeOp>,
        opcode: Option<PcodeOpcode>,
        block_addr: Option<u64>,
        seq: Option<u64>,
        fatal: bool,
        context: &str,
    ) {
        if !preview_debug_enabled() {
            return;
        }
        let trace_id = self.inventory_trace_id().unwrap_or(0);

        let def_op = vn
            .and_then(|vn| self.lookup_def_site(vn))
            .map(|(_, def)| format!("{:?}", def.opcode));
        let snippet = op
            .map(|op| self.format_op_snippet(op))
            .or_else(|| {
                vn.and_then(|vn| self.lookup_def_site(vn))
                    .map(|(_, def)| self.format_op_snippet(def))
            })
            .unwrap_or_else(|| "<none>".to_string());
        let event = serde_json::json!({
            "trace_id": trace_id,
            "stage": stage,
            "opcode": opcode.map(|op| format!("{op:?}")),
            "address": op.map(|op| op.address).or(block_addr),
            "block_start": block_addr
                .or_else(|| self.current_lowering_site.map(|site| self.pcode.blocks[site.block_idx].start_address)),
            "varnode": vn.map(|vn| self.format_varnode(vn)),
            "def_op": def_op,
            "def_chain_depth": self.lowering_site_depth,
            "snippet": snippet,
            "fatal": fatal,
            "context": context,
            "seq": op.map(|op| u64::from(op.seq_num)).or(seq),
        });

        let path = self.unsupported_inventory_path();
        let mut events = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Vec<serde_json::Value>>(&raw).ok())
            .unwrap_or_default();
        events.push(event);
        let _ = std::fs::write(
            path,
            serde_json::to_vec_pretty(&events).unwrap_or_else(|_| b"[]".to_vec()),
        );
    }
}

// These flags are checked from many call sites across the builder,
// several on hot per-block/per-op paths (e.g. `control/terminator.rs`'s
// ~17 call sites, `expr/lower_expr.rs`, `memory/aggregate_recovery.rs`).
// Cached with `OnceLock` so each is one syscall per process instead of
// one per call site visited.

pub(super) fn preview_builder_diag_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("FISSION_PREVIEW_DIAG").is_some())
}

pub(super) fn preview_debug_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("FISSION_PREVIEW_DEBUG").is_some())
}

pub(super) fn preview_debug_regdump_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("FISSION_PREVIEW_DEBUG_REGDUMP").is_some())
}
