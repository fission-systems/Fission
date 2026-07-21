use super::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

struct DecodePool {
    handles: Vec<Option<RuntimeHandle>>,
    operand_absolute_offsets: Vec<Option<usize>>,
    operand_relative_lengths: Vec<Option<usize>>,
    handle_reference_bitmap: Vec<bool>,
}

/// Shared root-instruction fallthrough for `inst_next` (RIP-relative).
///
/// Nested exports may evaluate `inst_next` before trailing root immediates are
/// bound and before the cursor advances past in-constructor-minimum fields.
/// We track:
/// - progress through bound content / local construct end
/// - remaining SLA minimum lengths of not-yet-started root operands
#[derive(Debug)]
struct InstNextShared {
    /// Root instruction VA.
    inst_address: u64,
    /// Buffer offset of instruction start (`ctx.cursor` at root entry).
    inst_buf_start: usize,
    /// Max absolute buffer end of finished root content / cursor progress.
    bound_end: Cell<usize>,
    /// Sum of `handles[i].minimum_length` for root operands not yet started.
    unbound_min: Cell<usize>,
    /// Growing floor from root `minimum_length` (updated as root learns size).
    root_min_length: Cell<usize>,
}

impl InstNextShared {
    fn estimated_rel_len(&self, cursor: usize, local_construct_end: usize) -> usize {
        let end = self
            .bound_end
            .get()
            .max(cursor)
            .max(local_construct_end);
        let progress = end.saturating_sub(self.inst_buf_start);
        let with_trailing = progress.saturating_add(self.unbound_min.get());
        with_trailing.max(self.root_min_length.get())
    }

    fn inst_next_addr(&self, cursor: usize, local_construct_end: usize) -> u64 {
        self.inst_address
            .saturating_add(self.estimated_rel_len(cursor, local_construct_end) as u64)
    }
}

thread_local! {
    static DECODE_POOL: RefCell<Vec<DecodePool>> = RefCell::new(Vec::new());
    static WALK_STACK: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

pub(super) struct WalkStackGuard;

impl WalkStackGuard {
    fn new(desc: String) -> Self {
        WALK_STACK.with(|stack| {
            stack.borrow_mut().push(desc);
        });
        Self
    }
}

impl Drop for WalkStackGuard {
    fn drop(&mut self) {
        WALK_STACK.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

pub(super) fn bind_instruction<'a>(
    compiled: &'a CompiledFrontend,
    strategy: RuntimeDecodeStrategy,
    ctx: &CompiledInstructionContext<'_>,
    selection: RuntimeSelection<'a>,
) -> Result<RuntimeConstructState> {
    bind_instruction_with_inst_next(compiled, strategy, ctx, selection, None)
}

fn bind_instruction_with_inst_next<'a>(
    compiled: &'a CompiledFrontend,
    strategy: RuntimeDecodeStrategy,
    ctx: &CompiledInstructionContext<'_>,
    selection: RuntimeSelection<'a>,
    parent_inst_next: Option<Rc<InstNextShared>>,
) -> Result<RuntimeConstructState> {
    let result = (|| {
        constructor_matches(ctx, selection.constructor)?;
        // Root instruction: two-pass so `inst_next` sees the full length (Ghidra
        // resolveHandles-after-resolve). Nested walkers inherit the parent's shared
        // fallthrough from the second pass.
        if parent_inst_next.is_none() && selection.trace.root_bucket == "instruction" {
            let probe = CompiledParserWalker::new(
                compiled,
                strategy,
                ctx,
                selection.clone(),
                None,
            )?
            .walk()?;
            // Prefer absolute end-relative-to-start: length may be absolute buffer end.
            let full_len = probe
                .relative_length
                .max(probe.length.saturating_sub(ctx.cursor));
            let shared = Rc::new(InstNextShared {
                inst_address: ctx.address,
                inst_buf_start: ctx.cursor,
                bound_end: Cell::new(ctx.cursor),
                unbound_min: Cell::new(0),
                root_min_length: Cell::new(full_len),
            });
            // owns=false: root_min_length already set to full probe length.
            return CompiledParserWalker::new(
                compiled,
                strategy,
                ctx,
                selection,
                Some(shared),
            )?
            .walk();
        }
        CompiledParserWalker::new(compiled, strategy, ctx, selection, parent_inst_next)?.walk()
    })();

    match result {
        Ok(state) => Ok(state),
        Err(err) => {
            let err_str = format!("{err:?}");
            if err_str.contains("sleigh parser path:") {
                Err(err)
            } else {
                let backtrace = WALK_STACK.with(|stack| stack.borrow().join(" -> "));
                if backtrace.is_empty() {
                    Err(err)
                } else {
                    Err(err.context(format!("sleigh parser path: {backtrace}")))
                }
            }
        }
    }
}

pub(super) struct PoolGuard {
    handles: Vec<Option<RuntimeHandle>>,
    operand_absolute_offsets: Vec<Option<usize>>,
    operand_relative_lengths: Vec<Option<usize>>,
    handle_reference_bitmap: Vec<bool>,
}

impl Drop for PoolGuard {
    fn drop(&mut self) {
        let handles = std::mem::take(&mut self.handles);
        let operand_absolute_offsets = std::mem::take(&mut self.operand_absolute_offsets);
        let operand_relative_lengths = std::mem::take(&mut self.operand_relative_lengths);
        let handle_reference_bitmap = std::mem::take(&mut self.handle_reference_bitmap);

        DECODE_POOL.with(|pool| {
            pool.borrow_mut().push(DecodePool {
                handles,
                operand_absolute_offsets,
                operand_relative_lengths,
                handle_reference_bitmap,
            });
        });
    }
}

pub(super) struct CompiledParserWalker<'a, 'b> {
    compiled: &'a CompiledFrontend,
    strategy: RuntimeDecodeStrategy,
    ctx: &'a CompiledInstructionContext<'b>,
    selection: RuntimeSelection<'a>,
    minimum_length: usize,
    context_register: u64,
    context_known_mask: u64,
    cursor: usize,
    pool_guard: PoolGuard,
    walker: spine::RuntimeParserWalker,
    /// Root fallthrough estimate for `inst_next` (shared with nested walkers).
    inst_next_shared: Rc<InstNextShared>,
    /// True when this walker owns/updates the shared fallthrough (root only).
    owns_inst_next: bool,
}

impl<'a, 'b> std::ops::Deref for CompiledParserWalker<'a, 'b> {
    type Target = PoolGuard;
    fn deref(&self) -> &Self::Target {
        &self.pool_guard
    }
}

impl<'a, 'b> std::ops::DerefMut for CompiledParserWalker<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pool_guard
    }
}

pub(super) struct OperandBinding {
    debug_value: Option<BoundOperand>,
    subtable_state: Option<RuntimeConstructState>,
    fixed: Option<RuntimeFixedHandle>,
    requires_fixed: bool,
}

impl OperandBinding {
    fn plain(value: BoundOperand) -> Self {
        Self {
            debug_value: Some(value),
            subtable_state: None,
            fixed: None,
            requires_fixed: true,
        }
    }

    fn with_fixed(value: BoundOperand, fixed: RuntimeFixedHandle) -> Self {
        Self {
            debug_value: Some(value),
            subtable_state: None,
            fixed: Some(fixed),
            requires_fixed: true,
        }
    }

    fn guard_only(subtable_state: RuntimeConstructState) -> Self {
        Self {
            debug_value: None,
            subtable_state: Some(subtable_state),
            fixed: None,
            requires_fixed: false,
        }
    }
}

fn instruction_terminal_pattern_len(selection: &RuntimeSelection<'_>) -> Result<usize> {
    let pattern = selection
        .trace
        .matched_leaf_pattern
        .as_ref()
        .ok_or_else(|| anyhow!("instruction selection missing terminal SLA pattern"))?;
    let len = disjoint_pattern_instruction_byte_len(pattern)?;
    if len == 0 {
        bail!("instruction terminal SLA pattern has zero instruction byte length");
    }
    Ok(len)
}

fn spec_derived_instruction_opcode_len(selection: &RuntimeSelection<'_>) -> Result<usize> {
    if let Some(len) = opcode_len_from_matcher(&selection.constructor.matcher)? {
        return Ok(len);
    }
    let len = usize::try_from(selection.constructor.minimum_length)
        .map_err(|_| anyhow!("constructor minimum length exceeds usize"))?;
    if len == 0 {
        bail!("instruction constructor has neither matcher opcode span nor minimum length");
    }
    Ok(len)
}

fn operand_spec_offsets(spec: &CompiledOperandSpec) -> Option<(i32, i32)> {
    match spec {
        CompiledOperandSpec::SlaTokenField {
            reloffset,
            offsetbase,
            ..
        }
        | CompiledOperandSpec::SlaVarnodeList {
            reloffset,
            offsetbase,
            ..
        }
        | CompiledOperandSpec::SlaVarnodeListExpression {
            reloffset,
            offsetbase,
            ..
        }
        | CompiledOperandSpec::SlaValueMap {
            reloffset,
            offsetbase,
            ..
        }
        | CompiledOperandSpec::SlaValueMapExpression {
            reloffset,
            offsetbase,
            ..
        }
        | CompiledOperandSpec::SlaPatternExpression {
            reloffset,
            offsetbase,
            ..
        }
        | CompiledOperandSpec::SubtableEvaluation {
            reloffset,
            offsetbase,
            ..
        } => Some((*reloffset, *offsetbase)),
        _ => None,
    }
}

fn required_const_tpl_u32(value: Option<u64>, role: &str) -> Result<u32> {
    let value = value.ok_or_else(|| anyhow!("{role} is missing"))?;
    u32::try_from(value).map_err(|_| anyhow!("{role} value {value} exceeds u32"))
}

#[cfg(test)]
mod construct_state_offset_tests {
    use super::{
        checked_pattern_add, checked_pattern_div, checked_pattern_left_shift, checked_pattern_mul,
        checked_pattern_negate, checked_pattern_right_shift, checked_pattern_sub,
        checked_relative_offset, checked_selector_display_index, checked_selector_index_i64,
        checked_selector_index_u64, checked_u32_to_usize, context_change_expr_word,
        context_change_mask_word, pattern_context_bits_i64, shifted_context_change_word,
    };
    use crate::compiler::{compile_x86_64_frontend, discovery};

    #[test]
    fn opcode_register_subtable_reads_from_sla_operand_offset() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        let decoded =
            crate::runtime::spine::compiled_table::decode_instruction(&compiled, &[0x57], 0x1000)
                .expect("decode push rdi");

        assert_eq!(decoded.length, 1);
        assert_eq!(decoded.mnemonic, "push");
        assert_eq!(decoded.operands_text, "RDI");
    }

    #[test]
    fn shared_token_operands_do_not_require_legacy_cursor_policy() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        for bytes in [&[0x57][..], &[0x48, 0x89, 0x5c, 0x24, 0x08][..]] {
            let (_ops, length, details) =
                crate::runtime::spine::compiled_table::decode_and_lift_with_details(
                    &compiled, bytes, 0x1000,
                )
                .expect("decode/lift shared-token sample");

            assert_eq!(length as usize, bytes.len());
            assert_eq!(
                details.template_source,
                Some(crate::compiler::CompiledTemplateSource::SpecDerived)
            );
        }
    }

    #[test]
    fn context_change_expr_word_fails_closed_on_out_of_range_values() {
        assert_eq!(context_change_expr_word(0xffff_ffff).unwrap(), u32::MAX);
        assert_eq!(context_change_expr_word(-1).unwrap(), u32::MAX);
        assert_eq!(context_change_expr_word(-2).unwrap(), 0xffff_fffe);
        assert!(context_change_expr_word(i64::from(i32::MIN) - 1).is_err());
        assert!(context_change_expr_word(i64::from(u32::MAX) + 1).is_err());
    }

    #[test]
    fn context_change_mask_word_fails_closed_above_u32() {
        assert_eq!(context_change_mask_word(0xffff_ffff).unwrap(), u32::MAX);
        assert!(context_change_mask_word(u64::from(u32::MAX) + 1).is_err());
    }

    #[test]
    fn context_change_shift_fails_closed_above_word_width() {
        assert_eq!(shifted_context_change_word(1, 31).unwrap(), 1u32 << 31);
        assert_eq!(shifted_context_change_word(0x8000_0000, -31).unwrap(), 1);
        assert!(shifted_context_change_word(1, 32).is_err());
        assert!(shifted_context_change_word(1, -32).is_err());
    }

    #[test]
    fn pattern_expression_arithmetic_fails_closed_on_overflow() {
        assert_eq!(checked_pattern_add(40, 2).unwrap(), 42);
        assert_eq!(checked_pattern_sub(40, 2).unwrap(), 38);
        assert_eq!(checked_pattern_mul(6, 7).unwrap(), 42);
        assert_eq!(checked_pattern_div(84, 2).unwrap(), 42);
        assert_eq!(checked_pattern_negate(42).unwrap(), -42);

        assert!(checked_pattern_add(i64::MAX, 1).is_err());
        assert!(checked_pattern_sub(i64::MIN, 1).is_err());
        assert!(checked_pattern_mul(i64::MAX, 2).is_err());
        assert!(checked_pattern_div(1, 0).is_err());
        assert!(checked_pattern_div(i64::MIN, -1).is_err());
        assert!(checked_pattern_negate(i64::MIN).is_err());
    }

    #[test]
    fn pattern_expression_shifts_fail_closed_on_negative_amounts() {
        assert_eq!(checked_pattern_left_shift(1, 63).unwrap(), i64::MIN);
        assert_eq!(checked_pattern_right_shift(-1, 63).unwrap(), 1);

        assert!(checked_pattern_left_shift(1, -1).is_err());
        assert!(checked_pattern_right_shift(1, -1).is_err());
    }

    /// Ghidra's own `LeftShiftExpression`/`RightShiftExpression::getValue`
    /// evaluate shifts as a raw C++ `<<`/`>>` on a 64-bit `intb` -- on the
    /// x86-64/ARM64 hosts Ghidra actually runs on, the CPU's shift
    /// instruction only consults the low 6 bits of the count, so amounts
    /// >= 64 behave as `amount & 63`, not an error. AArch64's `ubfx`/
    /// `bfxil`-family decode legitimately computes a shift amount of
    /// exactly 64 for a full-width 64-bit bitfield immediate (confirmed
    /// via a real `aarch64-linux-gnu-gcc`-compiled bitfield-struct fixture
    /// that failed to *decode* entirely before this was masked instead of
    /// rejected).
    #[test]
    fn pattern_expression_shifts_wrap_amounts_past_64_like_ghidras_host_cpu() {
        assert_eq!(checked_pattern_left_shift(1, 64).unwrap(), 1); // amount & 63 == 0
        assert_eq!(checked_pattern_right_shift(1, 64).unwrap(), 1);
        assert_eq!(checked_pattern_left_shift(1, 65).unwrap(), 2); // amount & 63 == 1
        assert_eq!(checked_pattern_left_shift(1, 127).unwrap(), i64::MIN); // amount & 63 == 63
    }

    #[test]
    fn relative_offsets_fail_closed_outside_usize_window() {
        assert_eq!(checked_relative_offset(10, -3, "test").unwrap(), 7);
        assert_eq!(checked_relative_offset(10, 3, "test").unwrap(), 13);
        assert!(checked_relative_offset(0, -1, "test").is_err());
        assert!(checked_relative_offset(usize::MAX, 1, "test").is_err());
    }

    #[test]
    fn selector_indices_fail_closed_on_lossy_conversion() {
        assert_eq!(checked_selector_index_u64(3, "test").unwrap(), 3);
        assert_eq!(checked_selector_index_i64(3, "test").unwrap(), 3);
        assert_eq!(checked_selector_display_index(3, "test").unwrap(), 3);

        if usize::try_from(u64::MAX).is_err() {
            assert!(checked_selector_index_u64(u64::MAX, "test").is_err());
        }
        assert!(checked_selector_index_i64(-1, "test").is_err());
        if usize::BITS > u32::BITS {
            assert!(checked_selector_display_index(u32::MAX as usize + 1, "test").is_err());
        }
    }

    #[test]
    fn u32_to_usize_conversion_fails_closed_on_narrow_targets() {
        assert_eq!(checked_u32_to_usize(3, "test").unwrap(), 3);
        if usize::BITS < u32::BITS {
            assert!(checked_u32_to_usize(u32::MAX, "test").is_err());
        }
    }

    #[test]
    fn pattern_context_bits_preserve_or_sign_extend_bit_patterns() {
        assert_eq!(pattern_context_bits_i64(0xff, 8, false).unwrap(), 0xff);
        assert_eq!(pattern_context_bits_i64(0xff, 8, true).unwrap(), -1);
        assert_eq!(pattern_context_bits_i64(0x80, 8, true).unwrap(), -128);
        assert_eq!(
            pattern_context_bits_i64(0x8000_0000_0000_0000, 64, false).unwrap(),
            i64::MIN
        );
    }
}

impl<'a, 'b> CompiledParserWalker<'a, 'b> {
    fn new(
        compiled: &'a CompiledFrontend,
        strategy: RuntimeDecodeStrategy,
        ctx: &'a CompiledInstructionContext<'b>,
        selection: RuntimeSelection<'a>,
        parent_inst_next: Option<Rc<InstNextShared>>,
    ) -> Result<Self> {
        let replace_current_wrapper = constructor_replaces_current(selection.constructor);
        let opcode_len = if replace_current_wrapper {
            0
        } else if selection.constructor.constructor_template.template_source
            == CompiledTemplateSource::SpecDerived
        {
            if selection.trace.root_bucket == "instruction"
                && constructor_consumes_sequential_operand_bytes(compiled, selection.constructor)?
            {
                instruction_terminal_pattern_len(&selection)?
            } else if selection.trace.root_bucket == "instruction" {
                // Some instruction-level constructors encode address bytes directly in the
                // terminal matcher instead of through a descendant operand subtable. If the
                // matcher is context-only, the SLA-derived constructor minimum length is the
                // typed cursor advance before binding displacement/address operands.
                spec_derived_instruction_opcode_len(&selection)?
            } else {
                0
            }
        } else {
            opcode_len_from_matcher(&selection.constructor.matcher)?.ok_or_else(|| {
                anyhow!("native matcher has no instruction byte span for opcode length")
            })?
        };
        let minimum_length = checked_u32_to_usize(
            selection.constructor.minimum_length,
            "constructor minimum length",
        )?;
        let cursor = ctx
            .cursor
            .checked_add(opcode_len)
            .ok_or_else(|| anyhow!("constructor cursor overflowed"))?;

        let mut pool_item = DECODE_POOL
            .with(|pool| pool.borrow_mut().pop())
            .unwrap_or_else(|| DecodePool {
                handles: Vec::new(),
                operand_absolute_offsets: Vec::new(),
                operand_relative_lengths: Vec::new(),
                handle_reference_bitmap: Vec::new(),
            });

        let target_len = selection.constructor.constructor_template.handles.len();

        pool_item.handles.clear();
        pool_item.handles.resize(target_len, None);

        pool_item.operand_absolute_offsets.clear();
        pool_item.operand_absolute_offsets.resize(target_len, None);

        pool_item.operand_relative_lengths.clear();
        pool_item.operand_relative_lengths.resize(target_len, None);

        constructor_template_handle_reference_bitmap(
            &selection.constructor.constructor_template,
            &mut pool_item.handle_reference_bitmap,
        )?;

        let (inst_next_shared, owns_inst_next) = if let Some(shared) = parent_inst_next {
            (shared, false)
        } else {
            let unbound_min = selection
                .constructor
                .constructor_template
                .handles
                .iter()
                .map(|h| h.minimum_length as usize)
                .sum();
            (
                Rc::new(InstNextShared {
                    inst_address: ctx.address,
                    inst_buf_start: ctx.cursor,
                    bound_end: Cell::new(ctx.cursor),
                    unbound_min: Cell::new(unbound_min),
                    root_min_length: Cell::new(minimum_length),
                }),
                true,
            )
        };

        Ok(Self {
            compiled,
            strategy,
            ctx,
            selection,
            minimum_length,
            context_register: ctx.context_register,
            context_known_mask: ctx.context_known_mask,
            cursor,
            pool_guard: PoolGuard {
                handles: pool_item.handles,
                operand_absolute_offsets: pool_item.operand_absolute_offsets,
                operand_relative_lengths: pool_item.operand_relative_lengths,
                handle_reference_bitmap: pool_item.handle_reference_bitmap,
            },
            walker: spine::RuntimeParserWalker::new(ctx.cursor, opcode_len),
            inst_next_shared,
            owns_inst_next,
        })
    }

    fn resolve_inst_next_addr(&self) -> Result<u64> {
        // Nested export often leaves `cursor` at the field base (fields inside
        // constructor minimum do not advance the cursor). Use construct end and
        // operand ends so InstNext is at least past the current field.
        let local_construct_end = self.constructor_minimum_end()?;
        let local_end = local_construct_end
            .max(self.cursor)
            .max(self.max_operand_end().unwrap_or(self.cursor));
        Ok(self
            .inst_next_shared
            .inst_next_addr(local_end, local_construct_end))
    }

    fn walk(mut self) -> Result<RuntimeConstructState> {
        let _guard = WalkStackGuard::new(format!(
            "constructor({}::{})",
            self.selection.constructor.source, self.selection.constructor.mnemonic
        ));

        for change in self.selection.constructor.context_changes.clone() {
            self.apply_context_change(&change)?;
        }

        let decode_steps = self
            .selection
            .constructor
            .constructor_template
            .decode_steps
            .clone();
        for step in decode_steps {
            match step {
                CompiledOperandDecodeStep::DecodeOperand { operand_index } => {
                    self.decode_operand(operand_index)?;
                }
                CompiledOperandDecodeStep::DescendSubtable {
                    table_name,
                    replace_current,
                } => {
                    // Mirror Ghidra's operand positioning from the handle template:
                    // ParserWalker uses getOffset(offsetbase) + reloffset before
                    // descending into a subtable.
                    let mut subtable_offset = (None, None, None);
                    for h in &self.selection.constructor.constructor_template.handles {
                        if let CompiledOperandSpec::SubtableEvaluation {
                            table_name: ref tn,
                            reloffset,
                            offsetbase,
                        } = h.spec
                        {
                            if tn.as_str() == table_name.as_str() {
                                subtable_offset = (
                                    Some(reloffset),
                                    Some(offsetbase),
                                    Some(self.operand_absolute_offset(&h.spec)?),
                                );
                                break;
                            }
                        }
                    }
                    let (reloffset, offsetbase, operand_absolute_offset) = subtable_offset;
                    // Capture this wrapper's own instruction-relative pattern before
                    // `decode_subtable` potentially replaces it entirely -- e.g. an x86
                    // legacy prefix byte's constructor matches only that byte, then
                    // (replace_current) hands off completely to the constructor for the
                    // rest of the instruction. Ghidra's own InstructionPrototype still
                    // folds the wrapper's pattern into the final instruction mask (see
                    // `instruction_pattern_mask`'s doc comment), so it can't just be
                    // dropped here even though the wrapper's identity otherwise is.
                    let wrapper_absolute_offset = self.ctx.cursor;
                    let wrapper_pattern = self.selection.trace.matched_leaf_pattern.clone();
                    let mut sub_state = self.decode_subtable(
                        &table_name,
                        reloffset,
                        offsetbase,
                        operand_absolute_offset,
                    )?;
                    if replace_current {
                        if let Some(pattern) = wrapper_pattern {
                            sub_state
                                .replaced_wrapper_patterns
                                .push((wrapper_absolute_offset, pattern));
                        }
                        return Ok(sub_state);
                    }
                }
            }
        }

        let mut handles = Vec::with_capacity(self.handles.len());
        for opt in &mut self.handles {
            let handle = opt
                .take()
                .ok_or_else(|| anyhow!("incomplete handle decode"))?;
            handles.push(handle);
        }
        handles.sort_by_key(|handle| handle.operand_index);
        let exported_handle = self.materialize_export_handle(&handles)?;
        let operands = handles
            .iter()
            .filter_map(|handle| handle.debug_value.clone())
            .collect::<Vec<_>>();

        let length = self
            .cursor
            .max(self.constructor_minimum_end()?)
            .max(self.max_operand_end()?);
        let absolute_offset = self.ctx.cursor;
        let relative_length = length
            .checked_sub(absolute_offset)
            .ok_or_else(|| anyhow!("constructor length resolved before instruction start"))?;

        Ok(RuntimeConstructState {
            subtable_id: self.selection.subtable_id,
            constructor_id: self.selection.constructor_id,
            constructor_slot: self.selection.constructor_slot,
            mnemonic: self.selection.constructor.mnemonic.clone(),
            construct_tpl_kind: self.selection.constructor.construct_tpl_kind,
            constructor_template: self.selection.constructor.constructor_template.clone(),
            named_templates: self.selection.constructor.named_templates.clone(),
            context_commits: self.selection.constructor.context_commits.clone(),
            display_template: self.selection.constructor.display_template.clone(),
            display_operands: self.selection.constructor.display_operands.clone(),
            construct_nodes: self.walker.into_nodes(),
            handles,
            exported_handle,
            operands,
            context_register: self.context_register,
            context_known_mask: self.context_known_mask,
            absolute_offset,
            relative_length,
            length,
            match_trace: self.selection.trace,
            replaced_wrapper_patterns: Vec::new(),
        })
    }

    fn materialize_export_handle(
        &mut self,
        handles: &[RuntimeHandle],
    ) -> Result<Option<RuntimeHandle>> {
        let Some(export_tpl) = self
            .selection
            .constructor
            .constructor_template
            .result
            .clone()
        else {
            return Ok(None);
        };
        let fixed = self.fixed_handle_from_handle_tpl(&export_tpl, handles)?;
        let value = self
            .display_operand_from_exported_display_template(handles)
            .transpose()?;
        Ok(Some(RuntimeHandle {
            operand_index: usize::MAX,
            spec: CompiledOperandSpec::SubtableEvaluation {
                table_name: self.selection.constructor.source.clone(),
                reloffset: 0,
                offsetbase: -1,
            },
            fixed,
            debug_value: value,
            subtable_state: None,
        }))
    }

    fn display_operand_from_exported_display_template(
        &self,
        handles: &[RuntimeHandle],
    ) -> Option<Result<BoundOperand>> {
        let template = &self.selection.constructor.display_template;
        let mut operand_indices = if let Some(flowthru_index) = template.flowthru_operand_index {
            vec![flowthru_index]
        } else {
            template
                .pieces
                .iter()
                .filter_map(|piece| match piece {
                    crate::compiler::CompiledDisplayPiece::OperandRef(index) => Some(*index),
                    crate::compiler::CompiledDisplayPiece::Literal(_) => None,
                })
                .collect::<Vec<_>>()
        };
        operand_indices.sort_unstable();
        operand_indices.dedup();
        let mut referenced_values = operand_indices
            .into_iter()
            .filter_map(|index| handles.get(index))
            .filter_map(|handle| handle.debug_value.clone())
            .collect::<Vec<_>>();
        if referenced_values.len() == 1 {
            Some(Ok(referenced_values.remove(0)))
        } else {
            None
        }
    }

    fn fixed_handle_from_handle_tpl(
        &mut self,
        handle_tpl: &CompiledHandleTpl,
        handles: &[RuntimeHandle],
    ) -> Result<RuntimeFixedHandle> {
        let space = handle_tpl
            .space
            .as_ref()
            .map(|space| self.resolve_export_space_tpl(space, handles))
            .transpose()?;
        let size = handle_tpl
            .size
            .as_ref()
            .map(|value| self.resolve_export_const_tpl(value, handles))
            .transpose()
            .and_then(|value| required_const_tpl_u32(value, "export HandleTpl size"))?;
        let offset_space = handle_tpl
            .ptr_space
            .as_ref()
            .map(|space| self.resolve_export_space_tpl(space, handles))
            .transpose()?;
        let offset_offset = handle_tpl
            .ptr_offset
            .as_ref()
            .map(|value| self.resolve_export_const_tpl(value, handles))
            .transpose()?
            .ok_or_else(|| anyhow!("export HandleTpl ptr_offset is missing"))?;
        let offset_size = handle_tpl
            .ptr_size
            .as_ref()
            .map(|value| self.resolve_export_const_tpl(value, handles))
            .transpose()
            .and_then(|value| required_const_tpl_u32(value, "export HandleTpl ptr_size"))?;
        let temp_space = handle_tpl
            .temp_space
            .as_ref()
            .map(|space| self.resolve_export_space_tpl(space, handles))
            .transpose()?;
        let temp_offset = handle_tpl
            .temp_offset
            .as_ref()
            .map(|value| self.resolve_export_const_tpl(value, handles))
            .transpose()?
            .ok_or_else(|| anyhow!("export HandleTpl temp_offset is missing"))?;
        // Ghidra: HandleTpl.fix() sets offset_space = null when the pointer space is
        // the constant address space (TYPE_CONSTANT). This distinguishes static handles
        // (register/RAM at a constant offset) from truly dynamic handles (pointer in unique).
        // Without this, register operands with const ptr_space incorrectly appear dynamic.
        let is_const_space = offset_space
            .as_ref()
            .is_some_and(|s| s.index == 0 || s.name == "const");
        let (offset_space, offset_offset, offset_size, temp_space, temp_offset) = if is_const_space
        {
            // Convert constant offset_space to static handle: multiply offset by addressable unit
            // size. For RAM (word_size=1) this is a no-op; for other spaces it scales correctly.
            let space_ref = space
                .as_ref()
                .ok_or_else(|| anyhow!("static HandleTpl missing target space"))?;
            let target_space = self
                .compiled
                .sla_spaces
                .get(&space_ref.index)
                .ok_or_else(|| anyhow!("static HandleTpl target space missing SLA metadata"))?;
            let addr_unit = if target_space.index == 0
                || target_space.name == "const"
                || target_space.name == "unique"
            {
                1
            } else if target_space.word_size == 0 {
                bail!(
                    "static HandleTpl target space {} has word_size=0",
                    target_space.name
                );
            } else {
                u64::from(target_space.word_size)
            };
            (
                None,
                offset_offset
                    .checked_mul(addr_unit)
                    .ok_or_else(|| anyhow!("static HandleTpl address-unit scaling overflowed"))?,
                0u32,
                None,
                0u64,
            )
        } else {
            (
                offset_space,
                offset_offset,
                offset_size,
                temp_space,
                temp_offset,
            )
        };
        let fixable = space.is_some()
            && (offset_space.is_none() || (offset_size != 0 && temp_space.is_some()));
        Ok(RuntimeFixedHandle {
            space,
            size,
            offset_space,
            offset_offset,
            offset_size,
            temp_space,
            temp_offset,
            fixable,
        })
    }

    fn resolve_export_space_tpl(
        &mut self,
        space: &CompiledSpaceTpl,
        handles: &[RuntimeHandle],
    ) -> Result<CompiledSpaceRef> {
        match space {
            CompiledSpaceTpl::SpaceRef(space) => Ok(space.clone()),
            CompiledSpaceTpl::Const(value) => {
                let index = self.resolve_export_const_tpl(value, handles)?;
                if let Some(space_ref) = self.compiled.sla_spaces.get(&index) {
                    return Ok(space_ref.clone());
                }
                bail!("export SpaceTpl references unknown SLA space id {index}")
            }
        }
    }

    fn resolve_export_const_tpl(
        &mut self,
        value: &CompiledConstTpl,
        handles: &[RuntimeHandle],
    ) -> Result<u64> {
        match value {
            CompiledConstTpl::Real { value } => Ok(*value),
            CompiledConstTpl::Integer { value, .. } if *value >= 0 => u64::try_from(*value)
                .map_err(|_| anyhow!("positive export integer ConstTpl exceeds u64")),
            CompiledConstTpl::Integer { value, .. } => Ok(i64_to_u64_bits(*value)),
            CompiledConstTpl::SpaceId(space) => Ok(space.index),
            CompiledConstTpl::Handle {
                handle_index,
                selector,
                plus,
            } => {
                let handle_index = checked_handle_index(*handle_index, "export")?;
                let handle = handles
                    .get(handle_index)
                    .ok_or_else(|| anyhow!("export handle {} is missing", handle_index))?;
                if matches!(selector, CompiledHandleSelector::OffsetPlus) {
                    let plus =
                        plus.ok_or_else(|| anyhow!("export offset_plus handle is missing plus"))?;
                    return resolve_offset_plus_pub(handle, plus);
                }
                let value = match selector {
                    CompiledHandleSelector::Space => handle
                        .fixed
                        .space
                        .as_ref()
                        .map(|space| space.index)
                        .ok_or_else(|| anyhow!("export fixed handle missing space"))?,
                    CompiledHandleSelector::Offset => handle.fixed.offset_offset,
                    CompiledHandleSelector::Size => u64::from(handle.fixed.size),
                    CompiledHandleSelector::OffsetPlus => unreachable!(),
                };
                reject_non_offset_handle_plus(*plus, "export")?;
                Ok(value)
            }
            CompiledConstTpl::InstStart => Ok(self.ctx.address),
            CompiledConstTpl::InstNext => self.resolve_inst_next_addr(),
            other => bail!("export ConstTpl {:?} is unsupported", other),
        }
    }

    fn decode_operand(&mut self, operand_index: usize) -> Result<()> {
        if self
            .handles
            .get(operand_index)
            .is_some_and(|handle| handle.is_some())
        {
            return Ok(());
        }
        let template = self
            .selection
            .constructor
            .constructor_template
            .handles
            .get(operand_index)
            .ok_or_else(|| anyhow!("missing handle template {operand_index}"))?
            .clone();
        let _guard = WalkStackGuard::new(format!("operand({})", operand_index));
        if self.owns_inst_next {
            // Active operand is counted via local construct/cursor progress; keep
            // only *later* operands in unbound_min.
            let op_min = template.minimum_length as usize;
            let u = self.inst_next_shared.unbound_min.get();
            self.inst_next_shared
                .unbound_min
                .set(u.saturating_sub(op_min));
            // Keep floor in sync if root minimum_length grew.
            let r = self.inst_next_shared.root_min_length.get();
            self.inst_next_shared
                .root_min_length
                .set(r.max(self.minimum_length));
        }
        let operand_absolute_offset = self.operand_absolute_offset(&template.spec)?;
        let binding = self.bind_operand(&template, operand_absolute_offset)?;
        let handle_index = operand_index;
        let operand_relative_length = match binding.subtable_state.as_ref() {
            Some(state) => state.relative_length,
            None => usize::try_from(template.minimum_length)
                .map_err(|_| anyhow!("operand minimum length exceeds usize"))?,
        };
        if self.owns_inst_next {
            let end = operand_absolute_offset.saturating_add(operand_relative_length);
            let b = self.inst_next_shared.bound_end.get();
            self.inst_next_shared.bound_end.set(b.max(end).max(self.cursor));
        }
        self.walker.record_operand_node(
            operand_index,
            0,
            operand_absolute_offset,
            operand_relative_length,
            handle_index,
        );
        self.operand_absolute_offsets[operand_index] = Some(operand_absolute_offset);
        self.operand_relative_lengths[operand_index] = Some(operand_relative_length);
        let fixed = match binding.fixed {
            Some(fixed) => fixed,
            None if !binding.requires_fixed => RuntimeFixedHandle::default(),
            None => bail!(
                "missing_sla_exported_fixed_handle: operand {operand_index} did not produce a fixed handle"
            ),
        };
        self.handles[operand_index] = Some(RuntimeHandle {
            operand_index,
            spec: template.spec,
            fixed,
            debug_value: binding.debug_value,
            subtable_state: binding.subtable_state.map(Box::new),
        });
        Ok(())
    }

    fn operand_absolute_offset(&self, spec: &CompiledOperandSpec) -> Result<usize> {
        let Some((reloffset, offsetbase)) = operand_spec_offsets(spec) else {
            return self.offset_irrelevant_operand_start(spec);
        };
        let base = self.offset_for_operand_base(offsetbase, "operand offset")?;
        checked_relative_offset(base, reloffset, "operand offset")
    }

    fn offset_irrelevant_operand_start(&self, spec: &CompiledOperandSpec) -> Result<usize> {
        match spec {
            CompiledOperandSpec::SlaFixedVarnode { .. }
            | CompiledOperandSpec::ContextFieldExtraction { .. } => Ok(self.ctx.cursor),
            other => bail!("SLA operand spec is missing offset metadata: {other:?}"),
        }
    }

    fn offset_for_operand_base(&self, offsetbase: i32, role: &str) -> Result<usize> {
        if offsetbase < 0 {
            return Ok(self.ctx.cursor);
        }
        let index = usize::try_from(offsetbase)
            .map_err(|_| anyhow!("{role} base {offsetbase} does not fit usize"))?;
        let offset = (*self
            .operand_absolute_offsets
            .get(index)
            .ok_or_else(|| anyhow!("{role} base {offsetbase} is out of range"))?)
        .ok_or_else(|| anyhow!("{role} base {offsetbase} has unresolved offset"))?;
        let length = (*self
            .operand_relative_lengths
            .get(index)
            .ok_or_else(|| anyhow!("{role} base {offsetbase} is out of range"))?)
        .ok_or_else(|| anyhow!("{role} base {offsetbase} has unresolved length"))?;
        offset
            .checked_add(length)
            .ok_or_else(|| anyhow!("{role} base {offsetbase} end offset overflowed"))
    }

    fn max_operand_end(&self) -> Result<usize> {
        let mut max_end = self.ctx.cursor;
        for (offset, length) in self
            .operand_absolute_offsets
            .iter()
            .zip(self.operand_relative_lengths.iter())
        {
            let (Some(offset), Some(length)) = (*offset, *length) else {
                continue;
            };
            let end = offset
                .checked_add(length)
                .ok_or_else(|| anyhow!("operand end offset overflowed"))?;
            max_end = max_end.max(end);
        }
        Ok(max_end)
    }

    fn constructor_minimum_end(&self) -> Result<usize> {
        self.ctx
            .cursor
            .checked_add(self.minimum_length)
            .ok_or_else(|| anyhow!("constructor minimum length overflowed"))
    }

    fn subtable_offset_from_sla_operands(
        &self,
        reloffset: Option<i32>,
        offsetbase: Option<i32>,
    ) -> Result<Option<usize>> {
        let Some(rel) = reloffset else {
            return Ok(None);
        };
        let base_index = offsetbase
            .ok_or_else(|| anyhow!("subtable offset missing base for reloffset {rel}"))?;
        let base = self.offset_for_operand_base(base_index, "subtable offset")?;
        Ok(Some(checked_relative_offset(base, rel, "subtable offset")?))
    }

    fn bind_operand(
        &mut self,
        template: &CompiledHandleTemplate,
        operand_absolute_offset: usize,
    ) -> Result<OperandBinding> {
        match &template.spec {
            CompiledOperandSpec::SlaTokenField {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                reloffset: _,
                offsetbase: _,
            } => {
                let token_base = self.token_base_for_sla_field(operand_absolute_offset);
                let value = read_sla_token_field_at(
                    self.ctx,
                    token_base,
                    *big_endian,
                    *sign_bit,
                    *bit_start,
                    *bit_end,
                    *byte_start,
                    *byte_end,
                    *shift,
                )?;
                let encoded_size =
                    checked_sla_field_encoded_size(*byte_start, *byte_end, "token field")?;
                self.advance_cursor_past_sla_field(token_base, encoded_size)?;
                Ok(OperandBinding::with_fixed(
                    BoundOperand::Immediate {
                        value,
                        encoded_size,
                        signed: *sign_bit,
                    },
                    fixed_handle_for_const_value(value, encoded_size),
                ))
            }
            CompiledOperandSpec::SlaVarnodeList {
                big_endian,
                sign_bit: _,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                entries,
                reloffset: _,
                offsetbase: _,
            } => {
                let token_base = self.token_base_for_sla_field(operand_absolute_offset);
                let selector = read_sla_token_field_at(
                    self.ctx,
                    token_base,
                    *big_endian,
                    false,
                    *bit_start,
                    *bit_end,
                    *byte_start,
                    *byte_end,
                    *shift,
                )?;
                let encoded_size =
                    checked_sla_field_encoded_size(*byte_start, *byte_end, "varnode list")?;
                self.advance_cursor_past_sla_field(token_base, encoded_size)?;
                let selector_index = checked_selector_index_u64(selector, "varnode list")?;
                let display_index = checked_selector_display_index(selector_index, "varnode list")?;
                let entry = entries.get(selector_index).ok_or_else(|| {
                    anyhow!(
                        "varnode list selector {} out of range for {} entries",
                        selector,
                        entries.len()
                    )
                })?;
                Ok(OperandBinding::with_fixed(
                    BoundOperand::NamedVarnode {
                        name: entry.name.clone(),
                        display_index: Some(display_index),
                        size: entry.size,
                    },
                    fixed_handle_from_resolved_varnode(entry),
                ))
            }
            CompiledOperandSpec::SlaVarnodeListExpression {
                expr,
                entries,
                reloffset: _,
                offsetbase: _,
            } => {
                let selector = self.eval_pattern_expression(expr)?;
                let selector_index = checked_selector_index_i64(selector, "varnode list")?;
                let display_index = checked_selector_display_index(selector_index, "varnode list")?;
                let entry = entries.get(selector_index).ok_or_else(|| {
                    anyhow!(
                        "varnode list selector {} out of range for {} entries",
                        selector,
                        entries.len()
                    )
                })?;
                Ok(OperandBinding::with_fixed(
                    BoundOperand::NamedVarnode {
                        name: entry.name.clone(),
                        display_index: Some(display_index),
                        size: entry.size,
                    },
                    fixed_handle_from_resolved_varnode(entry),
                ))
            }
            CompiledOperandSpec::SlaValueMap {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                values,
                reloffset: _,
                offsetbase: _,
            } => {
                let token_base = self.token_base_for_sla_field(operand_absolute_offset);
                let selector = read_sla_token_field_at(
                    self.ctx,
                    token_base,
                    *big_endian,
                    false,
                    *bit_start,
                    *bit_end,
                    *byte_start,
                    *byte_end,
                    *shift,
                )?;
                let selector_index = checked_selector_index_u64(selector, "value map")?;
                let value = values.get(selector_index).copied().ok_or_else(|| {
                    anyhow!(
                        "value map selector {} out of range for {} entries",
                        selector,
                        values.len()
                    )
                })?;
                let encoded_size =
                    checked_sla_field_encoded_size(*byte_start, *byte_end, "value map")?;
                self.advance_cursor_past_sla_field(token_base, encoded_size)?;
                let value_bits = i64_to_u64_bits(value);
                Ok(OperandBinding::with_fixed(
                    BoundOperand::Immediate {
                        value: value_bits,
                        encoded_size,
                        signed: *sign_bit || value < 0,
                    },
                    fixed_handle_for_const_value(value_bits, encoded_size),
                ))
            }
            CompiledOperandSpec::SlaValueMapExpression {
                expr,
                values,
                reloffset: _,
                offsetbase: _,
            } => {
                let selector = self.eval_pattern_expression(expr)?;
                let selector_index = checked_selector_index_i64(selector, "value map")?;
                let value = values.get(selector_index).copied().ok_or_else(|| {
                    anyhow!(
                        "value map selector {} out of range for {} entries",
                        selector,
                        values.len()
                    )
                })?;
                let value_bits = i64_to_u64_bits(value);
                Ok(OperandBinding::with_fixed(
                    BoundOperand::Immediate {
                        value: value_bits,
                        encoded_size: 0,
                        signed: value < 0,
                    },
                    fixed_handle_for_const_value(value_bits, 0),
                ))
            }
            CompiledOperandSpec::SlaFixedVarnode { varnode } => Ok(OperandBinding::with_fixed(
                BoundOperand::NamedVarnode {
                    name: varnode.name.clone(),
                    display_index: None,
                    size: varnode.size,
                },
                fixed_handle_from_resolved_varnode(varnode),
            )),
            CompiledOperandSpec::SlaPatternExpression {
                expr,
                reloffset: _,
                offsetbase: _,
            } => {
                let mut encoded_size = 0;
                let value = if let CompiledPatternExpression::TokenField {
                    big_endian,
                    sign_bit,
                    bit_start,
                    bit_end,
                    byte_start,
                    byte_end,
                    shift,
                } = expr
                {
                    let token_base = self.token_base_for_sla_field(operand_absolute_offset);
                    let value = u64_to_i64_bits(read_sla_token_field_at(
                        self.ctx,
                        token_base,
                        *big_endian,
                        *sign_bit,
                        *bit_start,
                        *bit_end,
                        *byte_start,
                        *byte_end,
                        *shift,
                    )?);
                    encoded_size =
                        checked_sla_field_encoded_size(*byte_start, *byte_end, "pattern token")?;
                    self.advance_cursor_past_sla_field(token_base, encoded_size)?;
                    value
                } else {
                    self.eval_pattern_expression(expr)?
                };
                let value_bits = i64_to_u64_bits(value);
                Ok(OperandBinding::with_fixed(
                    BoundOperand::Immediate {
                        value: value_bits,
                        encoded_size,
                        signed: value < 0,
                    },
                    fixed_handle_for_const_value(value_bits, encoded_size),
                ))
            }
            CompiledOperandSpec::ContextFieldExtraction {
                bit_offset,
                bit_width,
                sign_extend,
            } => {
                let val = u64::from(packed_context_bits(
                    self.context_register,
                    *bit_offset,
                    *bit_width,
                )?);
                let value = if *sign_extend {
                    sign_extend_bits(val, *bit_width)
                } else {
                    val
                };
                let encoded_size = (*bit_width / 8).max(1);
                Ok(OperandBinding::with_fixed(
                    BoundOperand::Immediate {
                        value,
                        encoded_size,
                        signed: *sign_extend,
                    },
                    fixed_handle_for_const_value(value, encoded_size),
                ))
            }
            CompiledOperandSpec::SubtableEvaluation {
                table_name,
                reloffset,
                offsetbase,
            } => {
                let cursor_start = self.cursor;
                let sub_state = self.decode_subtable(
                    table_name,
                    Some(*reloffset),
                    Some(*offsetbase),
                    Some(operand_absolute_offset),
                )?;
                let spec_derived_sla_operand = self
                    .selection
                    .constructor
                    .constructor_template
                    .template_source
                    == CompiledTemplateSource::SpecDerived
                    && operand_spec_offsets(&template.spec).is_some();
                let sub_relative_length = sub_state
                    .length
                    .checked_sub(self.ctx.cursor)
                    .ok_or_else(|| anyhow!("subtable length resolved before instruction start"))?;
                if spec_derived_sla_operand {
                    self.minimum_length = self.minimum_length.max(sub_relative_length);
                    self.cursor = cursor_start;
                } else if !subtable_consumes_sequential_bytes(self.compiled, table_name, 0)? {
                    self.minimum_length = self.minimum_length.max(sub_relative_length);
                    self.cursor = cursor_start;
                } else {
                    self.cursor = self.cursor.max(sub_state.length);
                }
                if self.owns_inst_next {
                    let r = self.inst_next_shared.root_min_length.get();
                    self.inst_next_shared
                        .root_min_length
                        .set(r.max(self.minimum_length));
                }
                // Return the exported handle from the sub-constructor. If no
                // handle is exported, only pure guard subtables may continue:
                // the parent ConstructTpl must not reference this operand
                // handle. This keeps no-export subtables out of raw P-code
                // handle resolution instead of inventing dummy handles.
                let exported = match sub_state.exported_handle.as_ref() {
                    Some(exported) => exported,
                    None => {
                        if constructor_template_references_handle(
                            &self.handle_reference_bitmap,
                            template.operand_index,
                        )? {
                            bail!(
                                "missing_sla_exported_fixed_handle: subtable {table_name} did not export handle for referenced operand {}",
                                template.operand_index
                            );
                        }
                        return Ok(OperandBinding::guard_only(sub_state));
                    }
                };
                let value = if exported.debug_value.is_some() {
                    Some(display_value_for_exported_handle(exported, &sub_state)?)
                } else {
                    None
                };
                Ok(OperandBinding {
                    debug_value: value,
                    fixed: Some(exported.fixed.clone()),
                    subtable_state: Some(sub_state),
                    requires_fixed: true,
                })
            }
        }
    }

    fn apply_context_change(&mut self, change: &crate::compiler::CompiledContextOp) -> Result<()> {
        if let Some(expr) = &change.expr {
            let saved_cursor = self.cursor;
            let raw = context_change_expr_word(self.eval_pattern_expression(expr)?)?;
            self.cursor = saved_cursor;
            let value = shifted_context_change_word(raw, change.shift)?;
            set_packed_context_word(
                &mut self.context_register,
                change.word_index,
                value,
                context_change_mask_word(change.mask)?,
            )?;
            set_packed_context_word(
                &mut self.context_known_mask,
                change.word_index,
                context_change_mask_word(change.mask)?,
                context_change_mask_word(change.mask)?,
            )?;
            if crate::runtime::diagnostics::terminal_reselect_trace_enabled() {
                eprintln!(
                    "[context-change expr] word={} mask=0x{:08x} value=0x{:08x} ctx=0x{:016x} known=0x{:016x}",
                    change.word_index,
                    context_change_mask_word(change.mask)?,
                    value,
                    self.context_register,
                    self.context_known_mask,
                );
            }
            Ok(())
        } else {
            if change.bit_offset >= 64 {
                let value = u32::try_from(change.value)
                    .map_err(|_| anyhow!("context word value exceeds u32"))?;
                let mask = context_change_mask_word(change.mask)?;
                set_packed_context_word(
                    &mut self.context_register,
                    change.word_index,
                    value,
                    mask,
                )?;
                set_packed_context_word(
                    &mut self.context_known_mask,
                    change.word_index,
                    mask,
                    mask,
                )?;
                return Ok(());
            }
            let field_mask = if change.bit_width >= 64 {
                u64::MAX
            } else {
                (1u64 << change.bit_width) - 1
            };
            let masked_value = change.value & field_mask;
            set_packed_context_bits(
                &mut self.context_register,
                change.bit_offset,
                change.bit_width,
                masked_value,
            )?;
            set_packed_context_bits(
                &mut self.context_known_mask,
                change.bit_offset,
                change.bit_width,
                field_mask,
            )?;
            if crate::runtime::diagnostics::terminal_reselect_trace_enabled() {
                eprintln!(
                    "[context-change bits] start={} width={} value=0x{:x} ctx=0x{:016x} known=0x{:016x}",
                    change.bit_offset,
                    change.bit_width,
                    masked_value,
                    self.context_register,
                    self.context_known_mask,
                );
            }
            Ok(())
        }
    }

    fn token_base_for_sla_field(&mut self, operand_absolute_offset: usize) -> usize {
        operand_absolute_offset
    }

    fn advance_cursor_past_sla_field(
        &mut self,
        token_base: usize,
        encoded_size: u32,
    ) -> Result<()> {
        if self.sla_field_is_within_constructor_minimum(token_base, encoded_size)? {
            return Ok(());
        }
        let field_end = token_base
            .checked_add(checked_u32_to_usize(
                encoded_size,
                "SLA token field encoded size",
            )?)
            .ok_or_else(|| anyhow!("SLA token field end offset overflowed"))?;
        self.cursor = self.cursor.max(field_end);
        Ok(())
    }

    fn sla_field_is_within_constructor_minimum(
        &self,
        token_base: usize,
        encoded_size: u32,
    ) -> Result<bool> {
        let constructor_end = self
            .ctx
            .cursor
            .checked_add(checked_u32_to_usize(
                self.selection.constructor.minimum_length,
                "constructor minimum length",
            )?)
            .ok_or_else(|| anyhow!("constructor minimum token range overflowed"))?;
        let token_end = token_base
            .checked_add(checked_u32_to_usize(
                encoded_size,
                "SLA token field encoded size",
            )?)
            .ok_or_else(|| anyhow!("SLA token field end offset overflowed"))?;
        Ok(token_base == self.ctx.cursor && token_end <= constructor_end)
    }

    fn eval_pattern_expression(&mut self, expr: &CompiledPatternExpression) -> Result<i64> {
        match expr {
            CompiledPatternExpression::Constant(value) => Ok(*value),
            CompiledPatternExpression::InstStart => Ok(u64_to_i64_bits(self.ctx.address)),
            CompiledPatternExpression::InstNext => {
                // Prefer shared root fallthrough so trailing immediates are included.
                Ok(u64_to_i64_bits(self.resolve_inst_next_addr()?))
            }
            CompiledPatternExpression::InstNext2 => {
                bail!("pattern expression inst_next2 requires delayed instruction context")
            }
            CompiledPatternExpression::TokenField {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
            } => Ok(u64_to_i64_bits(read_sla_token_field(
                self.ctx,
                *big_endian,
                *sign_bit,
                *bit_start,
                *bit_end,
                *byte_start,
                *byte_end,
                *shift,
            )?)),
            CompiledPatternExpression::ContextField {
                sign_bit,
                bit_start,
                bit_end,
                byte_start: _,
                byte_end: _,
                shift: _,
            } => {
                let bit_width = bit_end
                    .checked_sub(*bit_start)
                    .and_then(|value| value.checked_add(1))
                    .ok_or_else(|| anyhow!("invalid context field {}..{}", bit_start, bit_end))?;
                let raw = u64::from(packed_context_bits(
                    self.context_register,
                    *bit_start,
                    bit_width,
                )?);
                pattern_context_bits_i64(raw, bit_width, *sign_bit)
            }
            CompiledPatternExpression::OperandValue { index } => {
                self.eval_operand_value_expression(*index)
            }
            CompiledPatternExpression::Add(lhs, rhs) => checked_pattern_add(
                self.eval_pattern_expression(lhs)?,
                self.eval_pattern_expression(rhs)?,
            ),
            CompiledPatternExpression::Sub(lhs, rhs) => checked_pattern_sub(
                self.eval_pattern_expression(lhs)?,
                self.eval_pattern_expression(rhs)?,
            ),
            CompiledPatternExpression::Mul(lhs, rhs) => checked_pattern_mul(
                self.eval_pattern_expression(lhs)?,
                self.eval_pattern_expression(rhs)?,
            ),
            CompiledPatternExpression::Div(lhs, rhs) => {
                let rhs = self.eval_pattern_expression(rhs)?;
                checked_pattern_div(self.eval_pattern_expression(lhs)?, rhs)
            }
            CompiledPatternExpression::LeftShift(lhs, rhs) => checked_pattern_left_shift(
                self.eval_pattern_expression(lhs)?,
                self.eval_pattern_expression(rhs)?,
            ),
            CompiledPatternExpression::RightShift(lhs, rhs) => checked_pattern_right_shift(
                self.eval_pattern_expression(lhs)?,
                self.eval_pattern_expression(rhs)?,
            ),
            CompiledPatternExpression::And(lhs, rhs) => {
                Ok(self.eval_pattern_expression(lhs)? & self.eval_pattern_expression(rhs)?)
            }
            CompiledPatternExpression::Or(lhs, rhs) => {
                Ok(self.eval_pattern_expression(lhs)? | self.eval_pattern_expression(rhs)?)
            }
            CompiledPatternExpression::Xor(lhs, rhs) => {
                Ok(self.eval_pattern_expression(lhs)? ^ self.eval_pattern_expression(rhs)?)
            }
            CompiledPatternExpression::Negate(inner) => {
                checked_pattern_negate(self.eval_pattern_expression(inner)?)
            }
            CompiledPatternExpression::Not(inner) => Ok(!self.eval_pattern_expression(inner)?),
        }
    }

    fn eval_operand_value_expression(&mut self, operand_index: usize) -> Result<i64> {
        let spec = self
            .selection
            .constructor
            .constructor_template
            .handles
            .get(operand_index)
            .ok_or_else(|| anyhow!("missing operand {operand_index} for pattern expression"))?
            .spec
            .clone();
        let operand_absolute_offset = self.operand_absolute_offset(&spec)?;
        match &spec {
            CompiledOperandSpec::SlaTokenField {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                ..
            }
            | CompiledOperandSpec::SlaVarnodeList {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                ..
            }
            | CompiledOperandSpec::SlaValueMap {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                ..
            } => Ok(u64_to_i64_bits(read_sla_token_field_at(
                self.ctx,
                operand_absolute_offset,
                *big_endian,
                *sign_bit,
                *bit_start,
                *bit_end,
                *byte_start,
                *byte_end,
                *shift,
            )?)),
            CompiledOperandSpec::SlaVarnodeListExpression { expr, .. }
            | CompiledOperandSpec::SlaValueMapExpression { expr, .. }
            | CompiledOperandSpec::SlaPatternExpression { expr, .. } => {
                if let CompiledPatternExpression::TokenField {
                    big_endian,
                    sign_bit,
                    bit_start,
                    bit_end,
                    byte_start,
                    byte_end,
                    shift,
                } = expr
                {
                    Ok(u64_to_i64_bits(read_sla_token_field_at(
                        self.ctx,
                        operand_absolute_offset,
                        *big_endian,
                        *sign_bit,
                        *bit_start,
                        *bit_end,
                        *byte_start,
                        *byte_end,
                        *shift,
                    )?))
                } else {
                    self.eval_pattern_expression(expr)
                }
            }
            CompiledOperandSpec::ContextFieldExtraction {
                bit_offset,
                bit_width,
                sign_extend,
            } => {
                let raw = u64::from(packed_context_bits(
                    self.context_register,
                    *bit_offset,
                    *bit_width,
                )?);
                pattern_context_bits_i64(raw, *bit_width, *sign_extend)
            }
            CompiledOperandSpec::SlaFixedVarnode { .. }
            | CompiledOperandSpec::SubtableEvaluation { .. } => {
                self.decode_operand(operand_index)?;
                let handle = self
                    .handles
                    .get(operand_index)
                    .and_then(|value| value.as_ref())
                    .ok_or_else(|| {
                        anyhow!("operand {operand_index} was not decoded for pattern expression")
                    })?;
                let fixed = &handle.fixed;
                if fixed.offset_space.is_none()
                    && fixed
                        .space
                        .as_ref()
                        .is_some_and(|space| space.name == "const" || space.index == 0)
                {
                    return Ok(u64_to_i64_bits(fixed.offset_offset));
                }
                bail!(
                    "operand {operand_index} has no evaluable defining expression for pattern expression"
                )
            }
        }
    }

    fn decode_subtable(
        &mut self,
        table_name: &str,
        reloffset: Option<i32>,
        offsetbase: Option<i32>,
        operand_absolute_offset: Option<usize>,
    ) -> Result<RuntimeConstructState> {
        let _guard = WalkStackGuard::new(format!("subtable({})", table_name));
        let mut sub_ctx = (*self.ctx).clone();
        sub_ctx.cursor = if let Some(offset) = operand_absolute_offset {
            offset
        } else if let Some(offset) =
            self.subtable_offset_from_sla_operands(reloffset, offsetbase)?
        {
            offset
        } else if self.selection.constructor.context_changes.is_empty() {
            self.cursor
        } else {
            let pattern = self
                .selection
                .trace
                .matched_leaf_pattern
                .as_ref()
                .ok_or_else(|| {
                    anyhow!("context-dependent subtable {table_name} missing terminal SLA pattern")
                })?;
            let consumed_instruction_bytes = disjoint_pattern_instruction_byte_len(pattern)?;
            if consumed_instruction_bytes == 0 {
                self.cursor
            } else {
                let cursor_delta = self
                    .cursor
                    .checked_sub(self.ctx.cursor)
                    .ok_or_else(|| anyhow!("subtable cursor resolved before instruction start"))?;
                let advance = consumed_instruction_bytes.max(cursor_delta);
                self.ctx
                    .cursor
                    .checked_add(advance)
                    .ok_or_else(|| anyhow!("subtable cursor overflowed"))?
            }
        };
        sub_ctx.context_register = self.context_register;
        sub_ctx.context_known_mask = self.context_known_mask;
        if crate::runtime::diagnostics::terminal_reselect_trace_enabled() {
            eprintln!(
                "[decode-subtable] table={} cursor=0x{:x} ctx=0x{:016x} known=0x{:016x}",
                table_name, sub_ctx.cursor, sub_ctx.context_register, sub_ctx.context_known_mask,
            );
        }

        let decode_no_match_address = subtable_decode_address(&sub_ctx)?;
        let selection =
            select_constructor(self.compiled, table_name, &sub_ctx)?.ok_or_else(|| {
                anyhow!("DecodeNoMatch in subtable {table_name} at 0x{decode_no_match_address:x}")
            })?;
        if crate::runtime::diagnostics::terminal_reselect_trace_enabled() {
            eprintln!(
                "[decode-subtable selection] table={} ctor={} mnemonic={} source={}",
                table_name,
                selection.constructor_index,
                selection.constructor.mnemonic,
                selection.constructor.source,
            );
        }

        bind_instruction_with_inst_next(
            self.compiled,
            self.strategy,
            &sub_ctx,
            selection,
            Some(Rc::clone(&self.inst_next_shared)),
        )
    }
}

fn subtable_decode_address(ctx: &CompiledInstructionContext<'_>) -> Result<u64> {
    let cursor = checked_usize_to_u64(ctx.cursor, "subtable cursor")?;
    ctx.address
        .checked_add(cursor)
        .ok_or_else(|| anyhow!("subtable decode address overflowed"))
}

fn checked_usize_to_u64(value: usize, role: &str) -> Result<u64> {
    u64::try_from(value).map_err(|_| anyhow!("{role} {value} exceeds u64"))
}

fn checked_u32_to_usize(value: u32, role: &str) -> Result<usize> {
    usize::try_from(value).map_err(|_| anyhow!("{role} {value} exceeds usize"))
}

fn checked_selector_index_u64(value: u64, role: &str) -> Result<usize> {
    usize::try_from(value).map_err(|_| anyhow!("{role} selector {value} exceeds usize"))
}

fn checked_selector_index_i64(value: i64, role: &str) -> Result<usize> {
    if value < 0 {
        bail!("{role} selector {value} is negative");
    }
    usize::try_from(value).map_err(|_| anyhow!("{role} selector {value} exceeds usize"))
}

fn checked_selector_display_index(value: usize, role: &str) -> Result<u32> {
    u32::try_from(value).map_err(|_| anyhow!("{role} selector {value} exceeds u32 display index"))
}

fn checked_sla_field_encoded_size(byte_start: u32, byte_end: u32, role: &str) -> Result<u32> {
    byte_end
        .checked_sub(byte_start)
        .and_then(|width| width.checked_add(1))
        .ok_or_else(|| {
            anyhow!("{role} byte range {byte_start}..={byte_end} is invalid or overflows")
        })
}

fn checked_relative_offset(base: usize, rel: i32, role: &str) -> Result<usize> {
    let rel = isize::try_from(rel)
        .map_err(|_| anyhow!("{role} relative offset {rel} does not fit isize"))?;
    base.checked_add_signed(rel)
        .ok_or_else(|| anyhow!("{role} resolved outside addressable decode window"))
}

fn context_change_expr_word(value: i64) -> Result<u32> {
    if value < i64::from(i32::MIN) || value > i64::from(u32::MAX) {
        return Err(anyhow!("context expression value {value} exceeds u32 word"));
    }
    if value < 0 {
        let signed = i32::try_from(value)
            .map_err(|_| anyhow!("context expression value {value} exceeds i32 word"))?;
        Ok(u32::from_ne_bytes(signed.to_ne_bytes()))
    } else {
        u32::try_from(value)
            .map_err(|_| anyhow!("context expression value {value} exceeds u32 word"))
    }
}

fn context_change_mask_word(mask: u64) -> Result<u32> {
    u32::try_from(mask).map_err(|_| anyhow!("context change mask 0x{mask:x} exceeds u32"))
}

fn shifted_context_change_word(value: u32, shift: i32) -> Result<u32> {
    if shift >= 0 {
        let shift =
            u32::try_from(shift).map_err(|_| anyhow!("context expression shift exceeds u32"))?;
        value
            .checked_shl(shift)
            .ok_or_else(|| anyhow!("context expression left shift {shift} exceeds u32 width"))
    } else {
        let amount = shift
            .checked_neg()
            .ok_or_else(|| anyhow!("context expression shift underflow"))?;
        let amount =
            u32::try_from(amount).map_err(|_| anyhow!("context expression shift exceeds u32"))?;
        value
            .checked_shr(amount)
            .ok_or_else(|| anyhow!("context expression right shift {amount} exceeds u32 width"))
    }
}

fn checked_pattern_add(lhs: i64, rhs: i64) -> Result<i64> {
    lhs.checked_add(rhs)
        .ok_or_else(|| anyhow!("pattern expression add overflowed: {lhs} + {rhs}"))
}

fn checked_pattern_sub(lhs: i64, rhs: i64) -> Result<i64> {
    lhs.checked_sub(rhs)
        .ok_or_else(|| anyhow!("pattern expression subtract overflowed: {lhs} - {rhs}"))
}

fn checked_pattern_mul(lhs: i64, rhs: i64) -> Result<i64> {
    lhs.checked_mul(rhs)
        .ok_or_else(|| anyhow!("pattern expression multiply overflowed: {lhs} * {rhs}"))
}

fn checked_pattern_div(lhs: i64, rhs: i64) -> Result<i64> {
    lhs.checked_div(rhs)
        .ok_or_else(|| anyhow!("pattern expression divide overflowed: {lhs} / {rhs}"))
}

fn checked_pattern_negate(value: i64) -> Result<i64> {
    value
        .checked_neg()
        .ok_or_else(|| anyhow!("pattern expression negate overflowed: -{value}"))
}

fn checked_pattern_left_shift(lhs: i64, rhs: i64) -> Result<i64> {
    let amount = pattern_shift_amount(rhs)?;
    lhs.checked_shl(amount)
        .ok_or_else(|| anyhow!("pattern expression left shift {amount} exceeds i64 width"))
}

fn checked_pattern_right_shift(lhs: i64, rhs: i64) -> Result<i64> {
    let amount = pattern_shift_amount(rhs)?;
    let shifted = i64_to_u64_bits(lhs)
        .checked_shr(amount)
        .ok_or_else(|| anyhow!("pattern expression right shift {amount} exceeds i64 width"))?;
    Ok(u64_to_i64_bits(shifted))
}

fn pattern_context_bits_i64(raw: u64, bit_width: u32, sign_extend: bool) -> Result<i64> {
    if bit_width > 64 {
        bail!("pattern context bit width {bit_width} exceeds i64 width");
    }
    if sign_extend {
        let shift = 64u32
            .checked_sub(bit_width)
            .ok_or_else(|| anyhow!("pattern context sign-extension shift underflow"))?;
        let shifted = raw
            .checked_shl(shift)
            .ok_or_else(|| anyhow!("pattern context left shift {shift} exceeds i64 width"))?;
        Ok(u64_to_i64_bits(shifted) >> shift)
    } else {
        Ok(u64_to_i64_bits(raw))
    }
}

/// Ghidra's own `LeftShiftExpression`/`RightShiftExpression::getValue` (in
/// `slghpatexpress.cc`) evaluate pattern-expression shifts as a raw C++
/// `leftval << rightval` / `leftval >> rightval` on `intb` (a plain 64-bit
/// `int8`) -- a shift amount >= 64 is undefined behavior per the C++
/// standard, but on the x86-64/ARM64 hosts Ghidra actually runs on, the
/// CPU's SHL/SAR instruction only consults the low 6 bits of the shift
/// count, so in practice this behaves as `amount & 63`, not an error.
/// AArch64's own `ubfx`/`bfxil`-family decode (`ImmR_bitfield64_imm` ->
/// `DecodeWMask64` in `AARCH64instructions.sinc`) legitimately computes a
/// shift amount of exactly 64 for a full-width 64-bit bitfield immediate
/// -- confirmed via a real `aarch64-linux-gnu-gcc`-compiled bitfield-struct
/// fixture, where this previously hard-failed *decoding* the instruction
/// entirely (not just lowering it) with "shift amount 64 exceeds i64
/// width". Masking to match Ghidra's de facto behavior fixes it.
fn pattern_shift_amount(value: i64) -> Result<u32> {
    let amount = u32::try_from(value)
        .map_err(|_| anyhow!("pattern expression shift amount {value} is negative or too large"))?;
    Ok(amount & 63)
}

#[cfg(test)]
mod sleigh_parity_gaps_tests {
    use super::*;
    use crate::compiler::{compile_x86_64_frontend, discovery};

    #[test]
    fn test_decode_pool_reuses_allocations() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");

        let initial_pool_len = DECODE_POOL.with(|p| p.borrow().len());

        let _decoded =
            crate::runtime::spine::compiled_table::decode_instruction(&compiled, &[0x57], 0x1000)
                .expect("decode push rdi");

        let post_pool_len = DECODE_POOL.with(|p| p.borrow().len());
        assert!(
            post_pool_len > initial_pool_len,
            "Pool should have received the dropped walker vectors"
        );

        let mut pool_item = DECODE_POOL.with(|p| p.borrow_mut().pop()).unwrap();
        assert!(
            pool_item.handles.capacity() > 0,
            "Capacity of handles vector should be preserved"
        );
    }

    #[test]
    fn test_walk_stack_backtrace_on_failure() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");

        let result = crate::runtime::spine::compiled_table::decode_instruction(
            &compiled,
            &[0x48, 0x89],
            0x1000,
        );

        if let Err(err) = result {
            let err_str = format!("{err:?}");
            assert!(
                err_str.contains("sleigh parser path:") || err_str.contains("constructor("),
                "Error should contain sleigh parser path backtrace: {err_str}"
            );
        }
    }
}
