use super::*;

/// How a [`RuntimeSleighFrontend::decode_run`] stretch ended.
pub(crate) enum DecodeRunEnd {
    /// The offset reached the stop offset (or the window end).
    Reached,
    /// A decode error after the first instruction: the sweep ends here.
    Stopped,
    /// An error that fails the whole window.
    Failed(anyhow::Error),
}

/// Instructions decoded by one [`RuntimeSleighFrontend::decode_run`].
pub(crate) struct DecodeRun {
    pub(crate) instructions: Vec<DecodedInstruction>,
    /// Window offset of each instruction, ascending.
    pub(crate) offsets: Vec<usize>,
    /// Offset of the first byte not consumed.
    pub(crate) end_offset: usize,
    pub(crate) end: DecodeRunEnd,
    /// Context commits queued by this run, for the next one.
    pub(crate) pending: std::collections::BTreeMap<u64, PackedContextOverride>,
    /// Whether any instruction in the run queued a context commit.
    pub(crate) saw_commits: bool,
    /// Speculative runs only: offsets where decoding failed and was resumed
    /// one byte on. `Some` is an error that fails the whole window if the real
    /// stream reaches it; `None` ends the sweep there.
    pub(crate) errors: Vec<(usize, Option<anyhow::Error>)>,
    /// The run stopped because it reached an offset in its `join` set.
    pub(crate) joined: bool,
}

/// State of one speculative piece while workers and the merge overlap.
enum DecodePiece {
    Pending,
    Skipped,
    Done(DecodeRun),
    Taken,
}

/// Why one decode step produced no instruction.
enum DecodeStepError {
    /// The decoder matched nothing (or could not lift): the end of a sweep.
    NoMatch(anyhow::Error),
    /// Anything that fails the whole window.
    Fail(anyhow::Error),
}

/// Queue the instruction's context commits and step past it.
fn advance_after(
    pending_overrides: &mut std::collections::BTreeMap<u64, PackedContextOverride>,
    saw_commits: &mut bool,
    instruction: &DecodedInstruction,
    current: u64,
    offset: usize,
) -> Result<(u64, usize)> {
    // Collect context commits from this instruction and queue them.
    for (target_addr, word_index, mask_u32, value_u32) in &instruction.pending_context_commits {
        *saw_commits = true;
        let entry = pending_overrides.entry(*target_addr).or_default();
        entry.merge_commit_word(*word_index, *mask_u32, *value_u32)?;
    }
    let step = instruction.length;
    let next_current = checked_instruction_fallthrough(current, step as u64)?;
    let next_offset = offset
        .checked_add(step)
        .ok_or_else(|| anyhow!("decode byte offset overflowed at 0x{next_current:x}"))?;
    Ok((next_current, next_offset))
}

impl DecodeRun {
    fn empty() -> Self {
        Self {
            instructions: Vec::new(),
            offsets: Vec::new(),
            end_offset: 0,
            end: DecodeRunEnd::Reached,
            pending: std::collections::BTreeMap::new(),
            saw_commits: false,
            errors: Vec::new(),
            joined: false,
        }
    }
}

const MAX_DECODE_THREADS: usize = 16;
/// Below this a piece costs more to hand off than to decode.
const MIN_DECODE_CHUNK_BYTES: usize = 16 * 1024;
/// A decode worker must not get less stack than the thread that used to do
/// the whole sweep; the pages are only committed when touched.
const DECODE_WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

impl RuntimeSleighFrontend {
    pub fn decode_and_lift(&self, bytes: &[u8], address: u64) -> Result<Vec<PcodeOp>> {
        let (ops, _) = self.decode_and_lift_with_len(bytes, address)?;
        Ok(ops)
    }

    pub fn decode_and_lift_with_details(
        &self,
        bytes: &[u8],
        address: u64,
    ) -> Result<(Vec<PcodeOp>, u64, RuntimeExecutionDetails)> {
        self.decode_and_lift_with_context_override(bytes, address, None)
    }

    pub fn decode_and_lift_with_context_override(
        &self,
        bytes: &[u8],
        address: u64,
        context_override: Option<PackedContextOverride>,
    ) -> Result<(Vec<PcodeOp>, u64, RuntimeExecutionDetails)> {
        if bytes.is_empty() {
            return Err(RuntimeSleighError::DecodeNoMatch {
                language: self.entry.entry_id.clone(),
                address,
            }
            .into());
        }
        match self.status {
            RuntimeFrontendStatus::RegisteredCompileOnly => {
                Err(RuntimeSleighError::UnsupportedGeneratedSemantic {
                    language: self.entry.entry_id.clone(),
                    status: self.status,
                }
                .into())
            }
            RuntimeFrontendStatus::ExecutableCandidate => engine::decode_and_lift_with_details(
                &self.entry,
                self.compiled.as_ref().ok_or_else(|| {
                    anyhow!("missing compiled frontend for {}", self.entry.entry_id)
                })?,
                bytes,
                address,
                context_override,
            ),
        }
    }

    pub fn decode_instruction_and_lift_with_context_override(
        &self,
        bytes: &[u8],
        address: u64,
        context_override: Option<PackedContextOverride>,
    ) -> Result<(
        DecodedInstruction,
        Vec<PcodeOp>,
        u64,
        RuntimeExecutionDetails,
    )> {
        if bytes.is_empty() {
            return Err(RuntimeSleighError::DecodeNoMatch {
                language: self.entry.entry_id.clone(),
                address,
            }
            .into());
        }
        match self.status {
            RuntimeFrontendStatus::RegisteredCompileOnly => {
                Err(RuntimeSleighError::UnsupportedGeneratedSemantic {
                    language: self.entry.entry_id.clone(),
                    status: self.status,
                }
                .into())
            }
            RuntimeFrontendStatus::ExecutableCandidate => {
                engine::decode_instruction_and_lift_with_details(
                    &self.entry,
                    self.compiled.as_ref().ok_or_else(|| {
                        anyhow!("missing compiled frontend for {}", self.entry.entry_id)
                    })?,
                    bytes,
                    address,
                    context_override,
                )
            }
        }
    }

    pub fn decode_and_lift_with_len(
        &self,
        bytes: &[u8],
        address: u64,
    ) -> Result<(Vec<PcodeOp>, u64)> {
        let (ops, len, _) = self.decode_and_lift_with_details(bytes, address)?;
        Ok((ops, len))
    }

    /// Ghidra-compatible FID (Function ID) full and specific hashes for a
    /// function's instruction extent (e.g. `DecodedPcodeFunction.instructions`).
    ///
    /// See `compiled_table::fid_hash` for the algorithm and its current
    /// limitations (no relocation-awareness yet).
    /// `resolve_register_offset` resolves a Ghidra-style register name (as
    /// carried by a decoded `BoundOperand::NamedVarnode`) to its SLEIGH
    /// register-space offset -- typically
    /// `fission_pcode_ir::midend::cspec::RegisterModel::lookup_name`, supplied
    /// by the caller rather than depended on directly (`fission-pcode`
    /// depends on `fission-sleigh`, not the other way around).
    ///
    /// Returns `None` if the extent is too short to hash reliably (fewer
    /// than `compiled_table::fid_hash::FID_SHORT_CODE_UNIT_LIMIT` code
    /// units after skipping alignment padding) or if this frontend has no
    /// compiled-table engine available. On success, returns
    /// `(full_count, full_hash, specific_count, specific_hash)` mirroring
    /// Ghidra's `FidHashQuad` field order.
    pub fn fid_hashes(
        &self,
        extent: &[DecodedInstruction],
        resolve_register_offset: &dyn Fn(&str) -> Option<i64>,
    ) -> Option<(u16, u64, u8, u64)> {
        let compiled = self.compiled.as_ref()?;
        let _binding_scope = spine::compiled_table::FidBindingScope::enter(compiled);
        let hashes = spine::compiled_table::fid_hash::compute_fid_hashes(
            compiled,
            extent,
            resolve_register_offset,
        )?;
        Some((
            hashes.full_count,
            hashes.full_hash,
            hashes.specific_count,
            hashes.specific_hash,
        ))
    }

    pub fn decode_window(
        &self,
        bytes: &[u8],
        address: u64,
        limit: usize,
    ) -> Result<Vec<DecodedInstruction>> {
        self.decode_window_with_context_override(bytes, address, limit, None)
    }

    pub fn decode_window_with_context_override(
        &self,
        bytes: &[u8],
        address: u64,
        limit: usize,
        initial_context_override: Option<PackedContextOverride>,
    ) -> Result<Vec<DecodedInstruction>> {
        if limit == 0 || bytes.is_empty() {
            return Ok(Vec::new());
        }
        let run = self.decode_run(
            bytes,
            address,
            0,
            bytes.len(),
            limit,
            initial_context_override,
            std::collections::BTreeMap::new(),
            true,
            false,
            None,
        );
        match run.end {
            DecodeRunEnd::Failed(err) => Err(err),
            DecodeRunEnd::Reached | DecodeRunEnd::Stopped => Ok(run.instructions),
        }
    }

    /// One stretch of the linear decode loop: decode from `start_offset` while
    /// the offset is below `stop_offset`, carrying `pending` context commits.
    ///
    /// This is the single body of the sweep. [`Self::decode_window`] runs it
    /// over the whole window; [`Self::decode_window_parallel`] runs it over
    /// pieces and stitches them, so both walk the identical loop.
    ///
    /// * `window_start`: the run begins the window, so a decode failure on its
    ///   first instruction is the window's error rather than the end of sweep.
    /// * `speculative`: the run starts at an offset nobody has shown to be an
    ///   instruction boundary. A failure there is not the end of the sweep, it
    ///   is a bad guess: the failure is recorded and decoding resumes one byte
    ///   on, looking for the boundary the real stream will land on.
    /// * `join`: ascending offsets at which to stop (once past the start),
    ///   because another run already holds the stream from there.
    #[allow(clippy::too_many_arguments)]
    fn decode_run(
        &self,
        bytes: &[u8],
        address: u64,
        start_offset: usize,
        stop_offset: usize,
        limit: usize,
        initial_context_override: Option<PackedContextOverride>,
        mut pending_overrides: std::collections::BTreeMap<u64, PackedContextOverride>,
        window_start: bool,
        speculative: bool,
        join: Option<&[usize]>,
    ) -> DecodeRun {
        let mut run = DecodeRun {
            instructions: Vec::with_capacity(limit.min(64)),
            offsets: Vec::new(),
            end_offset: start_offset,
            end: DecodeRunEnd::Reached,
            pending: std::collections::BTreeMap::new(),
            saw_commits: false,
            errors: Vec::new(),
            joined: false,
        };
        let mut offset = start_offset;
        let mut current = match checked_instruction_fallthrough(address, start_offset as u64) {
            Ok(current) => current,
            Err(err) => {
                run.end = DecodeRunEnd::Failed(err);
                return run;
            }
        };
        while offset < bytes.len() && offset < stop_offset && run.instructions.len() < limit {
            if let Some(join) = join {
                if offset > start_offset && join.binary_search(&offset).is_ok() {
                    run.joined = true;
                    break;
                }
            }
            let remaining = &bytes[offset..];

            // Apply any pending ContextCommit overrides for this address.
            let ctx_override = match (initial_context_override, pending_overrides.get(&current)) {
                (Some(base), Some(pending)) => Some(base.merge_override(*pending)),
                (Some(base), None) => Some(base),
                (None, Some(pending)) => Some(*pending),
                (None, None) => None,
            };

            let decoded = match self.decode_instruction_with_context_override(
                remaining,
                current,
                ctx_override,
            ) {
                Ok(instruction) if instruction.length == 0 => Err(DecodeStepError::Fail(anyhow!(
                    "decoder returned zero length at 0x{:x}",
                    current
                ))),
                Ok(instruction) if instruction.length > remaining.len() => {
                    Err(DecodeStepError::Fail(anyhow!(
                        "decoded length {} exceeds available bytes {} at 0x{:x}",
                        instruction.length,
                        remaining.len(),
                        current
                    )))
                }
                Ok(instruction) => Ok(instruction),
                Err(err) => Err(DecodeStepError::NoMatch(err)),
            };
            let instruction = match decoded {
                Ok(instruction) => instruction,
                Err(DecodeStepError::NoMatch(err))
                    if window_start && run.instructions.is_empty() =>
                {
                    run.end = DecodeRunEnd::Failed(err);
                    break;
                }
                Err(DecodeStepError::NoMatch(err)) => {
                    if speculative {
                        run.errors.push((offset, None));
                        offset += 1;
                        current = current.wrapping_add(1);
                        continue;
                    }
                    drop(err);
                    run.end = DecodeRunEnd::Stopped;
                    break;
                }
                Err(DecodeStepError::Fail(err)) => {
                    if speculative {
                        run.errors.push((offset, Some(err)));
                        offset += 1;
                        current = current.wrapping_add(1);
                        continue;
                    }
                    run.end = DecodeRunEnd::Failed(err);
                    break;
                }
            };

            let step = instruction.length;
            match advance_after(
                &mut pending_overrides,
                &mut run.saw_commits,
                &instruction,
                current,
                offset,
            ) {
                Ok((next_current, next_offset)) => {
                    run.offsets.push(offset);
                    run.instructions.push(instruction);
                    current = next_current;
                    offset = next_offset;
                    run.end_offset = offset;
                }
                Err(err) => {
                    if speculative {
                        run.errors.push((offset, Some(err)));
                        offset += 1;
                        current = current.wrapping_add(1);
                        continue;
                    }
                    let _ = step;
                    run.end = DecodeRunEnd::Failed(err);
                    break;
                }
            }
        }
        run.pending = pending_overrides;
        run
    }

    /// [`Self::decode_window`] over the whole window with `limit` unbounded,
    /// split across threads. The result is the same `Vec`, instruction for
    /// instruction, as the sequential sweep.
    ///
    /// A linear sweep cannot be cut anywhere: where an instruction starts
    /// depends on the one before it. So each piece after the first is decoded
    /// speculatively from its nominal start, and the pieces are stitched in
    /// order against the real boundary the previous piece ended on. A piece is
    /// used only from an offset the real stream actually lands on (from there
    /// the two streams are the same bytes at the same addresses and so decode
    /// the same) and only if no context commit can have carried state across
    /// the seam. Where the real stream does not land on the piece, it is
    /// decoded sequentially until it does. Wrong speculation costs time, never
    /// a different result.
    pub fn decode_window_parallel(
        &self,
        bytes: &[u8],
        address: u64,
    ) -> Result<Vec<DecodedInstruction>> {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(MAX_DECODE_THREADS);
        let chunk_len = (bytes.len() / (threads * 4).max(1)).max(MIN_DECODE_CHUNK_BYTES);
        self.decode_window_chunked(bytes, address, chunk_len, threads)
    }

    pub(crate) fn decode_window_chunked(
        &self,
        bytes: &[u8],
        address: u64,
        chunk_len: usize,
        threads: usize,
    ) -> Result<Vec<DecodedInstruction>> {
        let chunks = if chunk_len == 0 {
            1
        } else {
            bytes.len().div_ceil(chunk_len)
        };
        let addresses_are_plain = address.checked_add(bytes.len() as u64).is_some();
        if threads < 2 || chunks < 2 || !addresses_are_plain {
            return self.decode_window(bytes, address, usize::MAX);
        }

        let stop_of = |k: usize| ((k + 1) * chunk_len).min(bytes.len());

        // The first piece is the real stream, so it is decoded on its own
        // before anything speculative starts. If the sweep already ended
        // there (a binary whose code does not decode from its first byte
        // stops after a handful of instructions), or if the architecture
        // queues context commits (speculation is unusable), there is nothing
        // to gain from other threads and no reason to start them.
        let first = self.decode_run(
            bytes,
            address,
            0,
            stop_of(0),
            usize::MAX,
            None,
            std::collections::BTreeMap::new(),
            true,
            false,
            None,
        );
        if let DecodeRunEnd::Failed(err) = first.end {
            return Err(err);
        }
        let stopped = matches!(first.end, DecodeRunEnd::Stopped);
        if stopped || first.end_offset >= bytes.len() || first.saw_commits {
            let mut out = first.instructions;
            if !stopped && first.end_offset < bytes.len() {
                let rest = self.decode_run(
                    bytes,
                    address,
                    first.end_offset,
                    bytes.len(),
                    usize::MAX,
                    None,
                    first.pending,
                    false,
                    false,
                    None,
                );
                if let DecodeRunEnd::Failed(err) = rest.end {
                    return Err(err);
                }
                out.extend(rest.instructions);
            }
            return Ok(out);
        }

        let next = std::sync::atomic::AtomicUsize::new(1);
        // Set when the merge below no longer needs pieces (the sweep ended or
        // finished) or once a piece queues a context commit: pieces not yet
        // started are then skipped rather than decoded for nothing.
        let stop_work = std::sync::atomic::AtomicBool::new(false);
        let slots = std::sync::Mutex::new(
            (0..chunks)
                .map(|_| DecodePiece::Pending)
                .collect::<Vec<_>>(),
        );
        let ready = std::sync::Condvar::new();
        // A worker that panics would leave its piece `Pending` for ever and the
        // merge waiting on it: say so instead, so the scope reports the panic.
        let worker_panicked = std::sync::atomic::AtomicBool::new(false);
        struct PanicGuard<'a> {
            panicked: &'a std::sync::atomic::AtomicBool,
            slots: &'a std::sync::Mutex<Vec<DecodePiece>>,
            ready: &'a std::sync::Condvar,
        }
        impl Drop for PanicGuard<'_> {
            fn drop(&mut self) {
                if std::thread::panicking() {
                    self.panicked
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    // Taking the lock orders this store before the merge's
                    // next check-then-wait.
                    drop(self.slots.lock());
                    self.ready.notify_all();
                }
            }
        }
        let work_loop = || loop {
            let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if k >= chunks {
                break;
            }
            let piece = if stop_work.load(std::sync::atomic::Ordering::Relaxed) {
                DecodePiece::Skipped
            } else {
                let run = self.decode_run(
                    bytes,
                    address,
                    k * chunk_len,
                    stop_of(k),
                    usize::MAX,
                    None,
                    std::collections::BTreeMap::new(),
                    false,
                    true,
                    None,
                );
                if run.saw_commits {
                    stop_work.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                DecodePiece::Done(run)
            };
            slots.lock().expect("decode slots poisoned")[k] = piece;
            ready.notify_all();
        };
        let work = || {
            let _guard = PanicGuard {
                panicked: &worker_panicked,
                slots: &slots,
                ready: &ready,
            };
            work_loop()
        };
        // The piece for `k`, waiting for a worker to finish it.
        let take_piece = |k: usize| -> Option<DecodeRun> {
            let mut guard = slots.lock().expect("decode slots poisoned");
            loop {
                match std::mem::replace(&mut guard[k], DecodePiece::Taken) {
                    DecodePiece::Pending => {
                        guard[k] = DecodePiece::Pending;
                        assert!(
                            !worker_panicked.load(std::sync::atomic::Ordering::SeqCst),
                            "a decode worker panicked"
                        );
                        guard = ready.wait(guard).expect("decode slots poisoned");
                    }
                    DecodePiece::Done(run) => return Some(run),
                    DecodePiece::Skipped | DecodePiece::Taken => return None,
                }
            }
        };

        std::thread::scope(|scope| {
            let mut spawned = 0;
            for _ in 0..threads.min(chunks - 1) {
                // Out of threads just means fewer workers.
                if std::thread::Builder::new()
                    .stack_size(DECODE_WORKER_STACK_BYTES)
                    .spawn_scoped(scope, work)
                    .is_ok()
                {
                    spawned += 1;
                }
            }
            if spawned == 0 {
                work();
            }

            let result = (|| -> Result<Vec<DecodedInstruction>> {
                let mut stopped = false;
                let mut pos = first.end_offset;
                let mut pending = first.pending;
                let mut out = first.instructions;

                // Plain sums throughout: `address + bytes.len()` was checked not
                // to overflow. No commit queued for an address at or after `pos`
                // means no earlier instruction can change how a later one decodes.
                let future_commits =
                    |pending: &std::collections::BTreeMap<u64, PackedContextOverride>,
                     pos: usize| {
                        pending.range(address + pos as u64..).next().is_some()
                    };

                for k in 1..chunks {
                    if stopped || pos >= bytes.len() {
                        break;
                    }
                    let stop = stop_of(k);
                    if pos >= stop {
                        continue;
                    }
                    // A piece that was skipped (speculation judged unusable) or
                    // that queued commits contributes nothing; the real stream
                    // decodes it.
                    let mut spec_run = take_piece(k).filter(|spec| !spec.saw_commits);

                    // Put the real stream on an instruction start of this piece.
                    if !(!future_commits(&pending, pos)
                        && spec_run
                            .as_ref()
                            .is_some_and(|spec| spec.offsets.binary_search(&pos).is_ok()))
                    {
                        let join = spec_run.as_ref().map(|spec| spec.offsets.as_slice());
                        let redo = self.decode_run(
                            bytes,
                            address,
                            pos,
                            stop,
                            usize::MAX,
                            None,
                            std::mem::take(&mut pending),
                            false,
                            false,
                            join,
                        );
                        if let DecodeRunEnd::Failed(err) = redo.end {
                            return Err(err);
                        }
                        stopped = matches!(redo.end, DecodeRunEnd::Stopped);
                        pos = redo.end_offset;
                        pending = redo.pending;
                        out.extend(redo.instructions);
                        if stopped || !redo.joined {
                            continue;
                        }
                        if future_commits(&pending, pos) {
                            // A commit reaches past the join: the piece's own
                            // decode did not see it, so finish this stretch
                            // sequentially.
                            let rest = self.decode_run(
                                bytes,
                                address,
                                pos,
                                stop,
                                usize::MAX,
                                None,
                                std::mem::take(&mut pending),
                                false,
                                false,
                                None,
                            );
                            if let DecodeRunEnd::Failed(err) = rest.end {
                                return Err(err);
                            }
                            stopped = matches!(rest.end, DecodeRunEnd::Stopped);
                            pos = rest.end_offset;
                            pending = rest.pending;
                            out.extend(rest.instructions);
                            continue;
                        }
                    }

                    // `pos` is now an instruction start of the piece, with no
                    // state in flight the piece did not see: take its stream
                    // from there, up to the first place that stream fails.
                    let spec = spec_run
                        .as_mut()
                        .expect("a piece is only joined when it was decoded");
                    let j = spec
                        .offsets
                        .binary_search(&pos)
                        .expect("pos is an instruction start of the piece");
                    let first_error = spec.errors.partition_point(|(at, _)| *at < pos);
                    if let Some((at, kind)) = spec.errors.get_mut(first_error) {
                        let at = *at;
                        if let Some(err) = kind.take() {
                            return Err(err);
                        }
                        let take = spec.offsets.partition_point(|offset| *offset < at);
                        out.extend(spec.instructions.drain(j..take));
                        pos = at;
                        stopped = true;
                    } else {
                        out.extend(spec.instructions.drain(j..));
                        pos = spec.end_offset;
                        pending = std::mem::take(&mut spec.pending);
                    }
                }
                Ok(out)
            })();
            // The merge is done with pieces: let workers drop what they have
            // not started so the scope can end.
            stop_work.store(true, std::sync::atomic::Ordering::Relaxed);
            result
        })
    }

    pub fn discover_direct_call_targets(
        &self,
        bytes: &[u8],
        base_address: u64,
    ) -> Result<Vec<u64>> {
        let mut targets = std::collections::BTreeSet::new();
        let mut offset = 0usize;
        let mut current = base_address;
        while offset < bytes.len() {
            let remaining = &bytes[offset..];
            let instruction = match self.decode_instruction_with_len(remaining, current) {
                Ok(instruction) => instruction,
                Err(err) if offset == 0 => return Err(err),
                Err(_) => break,
            };
            if instruction.flow_kind == DecodedFlowKind::Call {
                if let Some(target) = instruction.direct_target {
                    targets.insert(target);
                }
            }
            if instruction.length == 0 || instruction.length > remaining.len() {
                break;
            }
            current = checked_instruction_fallthrough(current, instruction.length as u64)?;
            offset = offset.checked_add(instruction.length).ok_or_else(|| {
                anyhow!("direct-call discovery byte offset overflowed at 0x{current:x}")
            })?;
        }
        Ok(targets.into_iter().collect())
    }

    pub fn decode_instruction_with_context_override(
        &self,
        bytes: &[u8],
        address: u64,
        context_override: Option<PackedContextOverride>,
    ) -> Result<DecodedInstruction> {
        if bytes.is_empty() {
            return Err(RuntimeSleighError::DecodeNoMatch {
                language: self.entry.entry_id.clone(),
                address,
            }
            .into());
        }
        match self.status {
            RuntimeFrontendStatus::RegisteredCompileOnly => {
                Err(RuntimeSleighError::UnsupportedGeneratedSemantic {
                    language: self.entry.entry_id.clone(),
                    status: self.status,
                }
                .into())
            }
            RuntimeFrontendStatus::ExecutableCandidate => engine::decode_instruction_with_context(
                &self.entry,
                self.compiled.as_ref().ok_or_else(|| {
                    anyhow!("missing compiled frontend for {}", self.entry.entry_id)
                })?,
                bytes,
                address,
                context_override,
            ),
        }
    }

    /// Decode a single instruction without emitting p-code. For callers that
    /// only need the disassembly-level view (mnemonic, length, flow kind,
    /// references) -- e.g. function-discovery candidate validation, which
    /// can call this thousands of times per candidate across hundreds of
    /// thousands of candidates -- this skips a full, and then immediately
    /// discarded, p-code lift per instruction. See
    /// `compiled_table::decode_instruction_no_pcode`'s doc comment for the
    /// full rationale.
    pub fn decode_single_no_pcode(
        &self,
        bytes: &[u8],
        address: u64,
        context_override: Option<PackedContextOverride>,
    ) -> Result<DecodedInstruction> {
        if bytes.is_empty() {
            return Err(RuntimeSleighError::DecodeNoMatch {
                language: self.entry.entry_id.clone(),
                address,
            }
            .into());
        }
        match self.status {
            RuntimeFrontendStatus::RegisteredCompileOnly => {
                Err(RuntimeSleighError::UnsupportedGeneratedSemantic {
                    language: self.entry.entry_id.clone(),
                    status: self.status,
                }
                .into())
            }
            RuntimeFrontendStatus::ExecutableCandidate => engine::decode_instruction_no_pcode(
                &self.entry,
                self.compiled.as_ref().ok_or_else(|| {
                    anyhow!("missing compiled frontend for {}", self.entry.entry_id)
                })?,
                bytes,
                address,
                context_override,
            ),
        }
    }

    /// [`decode_window`](Self::decode_window), but without emitting p-code
    /// per instruction -- see [`decode_single_no_pcode`](Self::decode_single_no_pcode)'s
    /// doc comment. Same multi-instruction/context-commit-propagation loop
    /// as `decode_window_with_context_override`, just calling the no-pcode
    /// primitive per step.
    pub fn decode_window_no_pcode(
        &self,
        bytes: &[u8],
        address: u64,
        limit: usize,
    ) -> Result<Vec<DecodedInstruction>> {
        self.decode_window_no_pcode_in_context(bytes, address, limit, None)
    }

    /// [`decode_window_no_pcode`], starting from a known processor context.
    ///
    /// A caller sweeping an image whose mode is not recoverable from the
    /// addresses themselves needs to say what the mode is. The clearest case
    /// is a stripped Cortex-M image: every address in it is even, because the
    /// ABI's bit-0 Thumb marker lives on function symbols and there are none
    /// left, yet every instruction in it is Thumb.
    ///
    /// `base_context` seeds the decode and applies to each instruction that a
    /// preceding one has not already committed a context for -- those still
    /// win, so a mode switch encoded in the instruction stream is honoured
    /// rather than overridden.
    ///
    /// [`decode_window_no_pcode`]: Self::decode_window_no_pcode
    pub fn decode_window_no_pcode_in_context(
        &self,
        bytes: &[u8],
        address: u64,
        limit: usize,
        base_context: Option<PackedContextOverride>,
    ) -> Result<Vec<DecodedInstruction>> {
        if limit == 0 || bytes.is_empty() {
            return Ok(Vec::new());
        }

        let mut pending_overrides: std::collections::BTreeMap<u64, PackedContextOverride> =
            std::collections::BTreeMap::new();
        let mut decoded = Vec::with_capacity(limit.min(64));
        let mut offset = 0usize;
        let mut current = address;
        while offset < bytes.len() && decoded.len() < limit {
            let remaining = &bytes[offset..];

            let ctx_override = pending_overrides.get(&current).copied().or(base_context);

            let instruction = match self.decode_single_no_pcode(remaining, current, ctx_override) {
                Ok(instruction) => instruction,
                Err(err) if decoded.is_empty() => return Err(err),
                Err(_) => break,
            };
            if instruction.length == 0 {
                bail!("decoder returned zero length at 0x{:x}", current);
            }
            let step = instruction.length;
            if step > remaining.len() {
                bail!(
                    "decoded length {} exceeds available bytes {} at 0x{:x}",
                    step,
                    remaining.len(),
                    current
                );
            }

            for (target_addr, word_index, mask_u32, value_u32) in
                &instruction.pending_context_commits
            {
                let entry = pending_overrides.entry(*target_addr).or_default();
                entry.merge_commit_word(*word_index, *mask_u32, *value_u32)?;
            }

            current = checked_instruction_fallthrough(current, step as u64)?;
            offset = offset
                .checked_add(step)
                .ok_or_else(|| anyhow!("decode byte offset overflowed at 0x{current:x}"))?;
            decoded.push(instruction);
        }
        Ok(decoded)
    }

    pub(super) fn decode_instruction_with_len(
        &self,
        bytes: &[u8],
        address: u64,
    ) -> Result<DecodedInstruction> {
        if bytes.is_empty() {
            return Err(RuntimeSleighError::DecodeNoMatch {
                language: self.entry.entry_id.clone(),
                address,
            }
            .into());
        }
        match self.status {
            RuntimeFrontendStatus::RegisteredCompileOnly => {
                Err(RuntimeSleighError::UnsupportedGeneratedSemantic {
                    language: self.entry.entry_id.clone(),
                    status: self.status,
                }
                .into())
            }
            RuntimeFrontendStatus::ExecutableCandidate => engine::decode_instruction(
                &self.entry,
                self.compiled.as_ref().ok_or_else(|| {
                    anyhow!("missing compiled frontend for {}", self.entry.entry_id)
                })?,
                bytes,
                address,
            ),
        }
    }
}
