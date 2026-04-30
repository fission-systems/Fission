
fn define_bits_kind(line: &str) -> Option<&'static str> {
    let mut parts = line.split_whitespace();
    if parts.next()? != "define" {
        return None;
    }
    match parts.next()? {
        "token" => Some("token"),
        "context" => Some("context"),
        _ => None,
    }
}

fn native_matcher_minimum_length(matcher: &CompiledPatternMatcher) -> usize {
    match matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => bytes.len(),
        CompiledPatternMatcher::RowCc { prefix, .. } => prefix.len() + 1,
        CompiledPatternMatcher::RowPage { .. } => 1,
        CompiledPatternMatcher::BitConstraints(constraints) => constraints
            .iter()
            .filter_map(|constraint| match constraint {
                PatternConstraint::Instruction { offset, .. } => Some(*offset as usize + 1),
                PatternConstraint::Context { .. } => None,
            })
            .max()
            .unwrap_or(0),
    }
}

fn strip_comments(raw: &str) -> &str {
    let mut in_string = false;
    for (idx, ch) in raw.char_indices() {
        if ch == '"' {
            in_string = !in_string;
        } else if ch == '#' && !in_string {
            return &raw[..idx];
        }
    }
    raw
}

fn constructor_mnemonic(signature: &str) -> String {
    signature
        .trim_start_matches(':')
        .split_whitespace()
        .next()
        .unwrap_or("<unknown>")
        .trim_end_matches(',')
        .to_string()
}

fn macro_name(signature: &str) -> String {
    signature
        .strip_prefix("macro ")
        .unwrap_or(signature)
        .split('(')
        .next()
        .unwrap_or("<unknown>")
        .trim()
        .to_string()
}

fn definition_name(statement: &str) -> String {
    statement
        .split_whitespace()
        .nth(2)
        .unwrap_or("<unknown>")
        .trim_matches(|ch| ch == ';' || ch == ':' || ch == '(' || ch == ')')
        .to_string()
}

fn classify_control_flow(body: &str) -> ControlFlowClass {
    let lower = body.to_ascii_lowercase();
    if lower.contains("call ") {
        ControlFlowClass::Call
    } else if lower.contains("return") {
        ControlFlowClass::Return
    } else if lower.contains("cbranch") || lower.contains("if ") {
        ControlFlowClass::ConditionalBranch
    } else if lower.contains("goto ") || lower.contains("branch") {
        ControlFlowClass::Branch
    } else {
        ControlFlowClass::None
    }
}

fn constructor_semantic_ops(body: &str, defined_pcode_ops: &BTreeSet<String>) -> Vec<String> {
    defined_pcode_ops
        .iter()
        .filter(|op| body.contains(&format!("{op}(")))
        .cloned()
        .collect()
}

fn stable_hash(text: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn build_decision_tree(constructors: &[CompiledExecutableConstructor]) -> CompiledDecisionTree {
    let constructor_indexes = (0..constructors.len()).collect::<Vec<_>>();
    let mut nodes = Vec::new();
    let root_node_index = build_bucket_node(
        constructors,
        &constructor_indexes,
        &decision_probes_for_constructors(constructors),
        &mut nodes,
    );
    let decision_node_count = nodes.len();
    CompiledDecisionTree {
        root_node_index,
        root_buckets: Vec::new(),
        nodes,
        decision_node_count,
    }
}

fn decision_probes_for_constructors(
    constructors: &[CompiledExecutableConstructor],
) -> Vec<CompiledDecisionProbe> {
    let mut probes = Vec::new();
    for offset in 0..4 {
        for bit in 0..8 {
            probes.push(CompiledDecisionProbe::InstructionBitSlice {
                offset: offset as u8,
                mask: 1 << bit,
                shift: bit as u8,
            });
        }
    }
    for bit in 0..8 {
        probes.push(CompiledDecisionProbe::ContextBitSlice {
            offset: 0,
            mask: 1 << bit,
            shift: bit as u8,
        });
    }
    probes
}

fn pattern_matcher_probe_len(matcher: &CompiledPatternMatcher) -> usize {
    match matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => bytes.len(),
        CompiledPatternMatcher::BitConstraints(constraints) => constraints
            .iter()
            .filter_map(|c| {
                if let PatternConstraint::Instruction { offset, .. } = c {
                    Some(*offset as usize + 1)
                } else {
                    None
                }
            })
            .max()
            .unwrap_or(0),
        _ => 1,
    }
}

fn build_bucket_node(
    constructors: &[CompiledExecutableConstructor],
    indexes: &[usize],
    probes: &[CompiledDecisionProbe],
    nodes: &mut Vec<CompiledDecisionNode>,
) -> usize {
    if indexes.len() <= 1 || probes.is_empty() {
        return push_leaf_node(constructors, indexes, nodes);
    }
    for (pos, probe) in probes.iter().enumerate() {
        let mut groups = BTreeMap::<u8, Vec<usize>>::new();
        let mut wildcard = Vec::new();
        for &idx in indexes {
            let values = decision_feature_values(&constructors[idx], *probe);
            if values.is_empty() {
                wildcard.push(idx);
            } else {
                for v in values {
                    groups.entry(v).or_default().push(idx);
                }
            }
        }
        if groups.len() <= 1 {
            continue;
        }
        let node_index = nodes.len();
        nodes.push(CompiledDecisionNode {
            probe: *probe,
            branches: Vec::new(),
            leaf_constructor_indexes: Vec::new(),
            leaf_entries: Vec::new(),
        });
        let mut branches = Vec::new();
        for (value, mut specific) in groups {
            let mut branch_indexes = wildcard.clone();
            branch_indexes.append(&mut specific);
            branch_indexes.sort_unstable();
            branch_indexes.dedup();
            branches.push(CompiledDecisionEdge {
                value,
                next_node_index: build_bucket_node(
                    constructors,
                    &branch_indexes,
                    &probes[pos + 1..],
                    nodes,
                ),
            });
        }
        nodes[node_index].branches = branches;
        return node_index;
    }
    push_leaf_node(constructors, indexes, nodes)
}

fn push_leaf_node(
    constructors: &[CompiledExecutableConstructor],
    indexes: &[usize],
    nodes: &mut Vec<CompiledDecisionNode>,
) -> usize {
    let mut sorted = indexes.to_vec();
    sorted.sort_by_key(|&idx| std::cmp::Reverse(decision_specificity(&constructors[idx])));
    let node_index = nodes.len();
    nodes.push(CompiledDecisionNode {
        probe: CompiledDecisionProbe::Terminal,
        branches: Vec::new(),
        leaf_constructor_indexes: sorted,
        leaf_entries: Vec::new(),
    });
    node_index
}

fn decision_feature_values(
    ctor: &CompiledExecutableConstructor,
    probe: CompiledDecisionProbe,
) -> Vec<u8> {
    match probe {
        CompiledDecisionProbe::InstructionBitSlice {
            offset,
            mask,
            shift,
        } => instruction_probe_values(&ctor.matcher, offset as usize)
            .into_iter()
            .map(|v| (v & mask) >> shift)
            .collect(),
        CompiledDecisionProbe::ContextBitSlice {
            offset,
            mask,
            shift,
        } => context_probe_values(&ctor.matcher, offset as usize)
            .into_iter()
            .map(|v| ((v & u64::from(mask)) >> shift) as u8)
            .collect(),
        CompiledDecisionProbe::SlaInstructionBits { .. }
        | CompiledDecisionProbe::SlaContextBits { .. } => Vec::new(),
        _ => Vec::new(),
    }
}

fn instruction_probe_values(matcher: &CompiledPatternMatcher, offset: usize) -> Vec<u8> {
    match matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => {
            bytes.get(offset).copied().into_iter().collect()
        }
        CompiledPatternMatcher::BitConstraints(constraints) => {
            let mut val = 0u8;
            let mut found = false;
            for c in constraints {
                if let PatternConstraint::Instruction {
                    offset: c_off,
                    mask,
                    value,
                } = c
                {
                    if offset >= *c_off as usize && offset < *c_off as usize + 8 {
                        let shift = (offset - *c_off as usize) * 8;
                        if (mask >> shift) & 0xff != 0 {
                            val |= ((value >> shift) & 0xff) as u8;
                            found = true;
                        }
                    }
                }
            }
            if found { vec![val] } else { Vec::new() }
        }
        _ => Vec::new(),
    }
}

fn context_probe_values(matcher: &CompiledPatternMatcher, offset: usize) -> Vec<u64> {
    if let CompiledPatternMatcher::BitConstraints(constraints) = matcher {
        constraints
            .iter()
            .filter_map(|c| {
                if let PatternConstraint::Context {
                    offset: c_off,
                    value,
                    ..
                } = c
                {
                    if offset == *c_off as usize {
                        Some(*value)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    }
}

fn decision_specificity(constructor: &CompiledExecutableConstructor) -> usize {
    let mut score = 0usize;
    if constructor.mnemonic.starts_with('^') {
        score = score.saturating_sub(500);
    }
    if let CompiledPatternMatcher::BitConstraints(ref constraints) = constructor.matcher {
        if !constraints.is_empty() {
            score += 1000;
        }
    }
    score += constructor.opsize_variants.len().min(1) * 2;
    score += constructor.operand_reg_values.len().min(1) * 3;
    score += usize::from(constructor.mod_constraint.is_some()) * 2;
    match &constructor.matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => score += bytes.len() * 80,
        CompiledPatternMatcher::RowCc { prefix, .. } => score += prefix.len() * 80 + 40,
        CompiledPatternMatcher::RowPage { .. } => score += 50,
        CompiledPatternMatcher::BitConstraints(constraints) => {
            for constraint in constraints {
                match constraint {
                    PatternConstraint::Instruction { mask, .. } => {
                        score += (mask.count_ones() as usize) * 10;
                    }
                    PatternConstraint::Context { mask, .. } => {
                        score += (mask.count_ones() as usize) * 10;
                    }
                }
            }
        }
    }
    score += constructor
        .operand_specs
        .iter()
        .filter(|spec| {
            matches!(
                spec,
                CompiledOperandSpec::TokenFieldExtraction { .. }
                    | CompiledOperandSpec::SlaTokenField { .. }
                    | CompiledOperandSpec::ContextFieldExtraction { .. }
                    | CompiledOperandSpec::SubtableEvaluation { .. }
            )
        })
        .count()
        * 20;
    score
}

fn normalize_executable_mnemonic(mnemonic: &str) -> String {
    let trimmed = mnemonic.trim();
    if trimmed.eq_ignore_ascii_case("J^cc") {
        return "J^CC".to_string();
    }
    if trimmed.eq_ignore_ascii_case("SET^cc") {
        return "SET^CC".to_string();
    }
    trimmed
        .split('^')
        .next()
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}

fn runtime_signature_is_supported(_signature: &str) -> bool {
    true
}

fn classify_display_construct_kind(mnemonic: &str) -> CompiledConstructTplKind {
    match mnemonic.to_ascii_uppercase().as_str() {
        "FINIT" | "FNINIT" => CompiledConstructTplKind::Unsupported,
        "NOP" | "PAUSE" => CompiledConstructTplKind::Nop,
        "RET" => CompiledConstructTplKind::Ret,
        "CALL" => CompiledConstructTplKind::Call,
        "JMP" => CompiledConstructTplKind::Jmp,
        "J^CC" => CompiledConstructTplKind::Jcc,
        "MOV" => CompiledConstructTplKind::Mov,
        "LEA" => CompiledConstructTplKind::AddressOf,
        "PUSH" => CompiledConstructTplKind::StackStore,
        "POP" => CompiledConstructTplKind::StackLoad,
        "LEAVE" => CompiledConstructTplKind::FrameTeardown,
        "ADD" => CompiledConstructTplKind::Add,
        "SUB" => CompiledConstructTplKind::Sub,
        "AND" => CompiledConstructTplKind::And,
        "OR" => CompiledConstructTplKind::Or,
        "XOR" => CompiledConstructTplKind::Xor,
        "IMUL" => CompiledConstructTplKind::Imul,
        "MUL" => CompiledConstructTplKind::Mul,
        "SHL" | "SAL" => CompiledConstructTplKind::Shl,
        "SHR" => CompiledConstructTplKind::Shr,
        "SAR" => CompiledConstructTplKind::Sar,
        "INC" => CompiledConstructTplKind::Inc,
        "DEC" => CompiledConstructTplKind::Dec,
        "CMP" => CompiledConstructTplKind::Cmp,
        "TEST" => CompiledConstructTplKind::Test,
        "MOVZX" => CompiledConstructTplKind::Movzx,
        "MOVSX" => CompiledConstructTplKind::Movsx,
        "MOVSXD" => CompiledConstructTplKind::Movsxd,
        "SET^CC" => CompiledConstructTplKind::Setcc,
        "CBW" => CompiledConstructTplKind::Cbw,
        "CWDE" => CompiledConstructTplKind::Cwde,
        "CDQE" => CompiledConstructTplKind::Cdqe,
        _ => CompiledConstructTplKind::Generic,
    }
}

fn parse_operand_specs(
    signature: &str,
    _matcher: &CompiledPatternMatcher,
    construct_tpl_kind: CompiledConstructTplKind,
) -> Result<Vec<CompiledOperandSpec>> {
    let first_line = signature.lines().next().unwrap_or(signature);
    let head = if let Some(pos) = first_line.find(" is ") {
        &first_line[..pos]
    } else if let Some(pos) = first_line.find("is ") {
        &first_line[..pos]
    } else {
        first_line
    };
    let head = head.trim().trim_start_matches(':');
    let operand_part = head
        .split_whitespace()
        .skip(1)
        .collect::<Vec<_>>()
        .join(" ");
    if operand_part.is_empty() {
        return Ok(Vec::new());
    }
    let mut specs = Vec::new();
    for raw_token in operand_part.split(',') {
        let token = raw_token.trim().trim_matches(|ch| ch == '(' || ch == ')');
        if token.is_empty() {
            continue;
        }
        if let Some(size) = relative_size(token) {
            specs.push(CompiledOperandSpec::Relative { size });
            continue;
        }
        if let Some((size, signed)) = immediate_size(token) {
            specs.push(CompiledOperandSpec::Immediate { size, signed });
            continue;
        }
        if let Some(size) = fixed_accumulator_size(token) {
            specs.push(CompiledOperandSpec::FixedRegister {
                reg: CompiledFixedRegister::Accumulator,
                size,
            });
            continue;
        }
        if let Some(size) = register_size_token(token) {
            specs.push(CompiledOperandSpec::TokenFieldExtraction {
                bit_offset: 0,
                bit_width: size * 8,
                sign_extend: false,
            });
            continue;
        }
        let token = token.trim();
        if !token.is_empty()
            && token.len() <= 64
            && token.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            specs.push(CompiledOperandSpec::SubtableEvaluation {
                table_name: token.to_string(),
                reloffset: 0,
                offsetbase: -1,
            });
        } else {
            specs.push(CompiledOperandSpec::Immediate {
                size: 0,
                signed: false,
            });
        }
    }
    if specs.is_empty() && !operand_part.is_empty() {
        return Ok(vec![CompiledOperandSpec::SubtableEvaluation {
            table_name: "unknown".to_string(),
            reloffset: 0,
            offsetbase: -1,
        }]);
    }
    if specs.is_empty() && operand_part.is_empty() {
        return Ok(Vec::new());
    }
    if matches!(construct_tpl_kind, CompiledConstructTplKind::Setcc) && specs.len() != 1 {
        return Err(anyhow!("setcc expects one operand"));
    }
    Ok(specs)
}

fn parse_hidden_subtables(
    signature: &str,
    field_info: &BTreeMap<String, FieldBitRange>,
) -> Vec<String> {
    let Some(is_pos) = signature.find(" is ") else {
        return Vec::new();
    };
    let rest = &signature[is_pos + 4..];
    let pattern_part = rest.split(['[', '{']).next().unwrap_or(rest);
    let mut subtables = Vec::new();
    for raw_token in pattern_part.split('&') {
        let token = raw_token
            .trim()
            .trim_matches(|ch| ch == '(' || ch == ')' || ch == '^');
        if token.is_empty()
            || token.contains('=')
            || token.chars().any(|ch| ch.is_ascii_whitespace())
            || !token.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
            || field_info.contains_key(token)
        {
            continue;
        }
        if !subtables.iter().any(|existing| existing == token) {
            subtables.push(token.to_string());
        }
    }
    subtables
}

fn parse_context_changes(
    signature: &str,
    field_info: &BTreeMap<String, FieldBitRange>,
) -> Vec<CompiledContextOp> {
    let mut ops = Vec::new();
    let Some(start) = signature.find('[') else {
        return ops;
    };
    let Some(end_rel) = signature[start + 1..].find(']') else {
        return ops;
    };
    let block = &signature[start + 1..start + 1 + end_rel];
    for stmt in block.split(';') {
        let stmt = stmt.trim();
        let Some((lhs, rhs)) = stmt.split_once('=') else {
            continue;
        };
        let name = lhs.trim();
        let rhs = rhs.trim();
        let Some(info) = field_info.get(name) else {
            continue;
        };
        if !matches!(info.kind, FieldKind::Context) {
            continue;
        }
        let Some(value) = parse_context_literal(rhs) else {
            continue;
        };
        ops.push(CompiledContextOp {
            bit_offset: info.bit_offset,
            bit_width: info.bit_width,
            value,
            word_index: 0,
            mask: if info.bit_width >= 64 {
                u64::MAX
            } else {
                ((1u64 << info.bit_width) - 1)
                    .checked_shl(info.bit_offset)
                    .unwrap_or(0)
            },
            shift: info.bit_offset as i32,
            expr: None,
        });
    }
    ops
}

fn parse_context_literal(text: &str) -> Option<u64> {
    let trimmed = text.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()
    } else if trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        trimmed.parse::<u64>().ok()
    } else {
        None
    }
}

fn parse_byte_sequence(signature: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut start = 0usize;
    while let Some(pos) = signature[start..].find("byte=0x") {
        let begin = start + pos + "byte=0x".len();
        let hex = signature[begin..]
            .chars()
            .take_while(|ch| ch.is_ascii_hexdigit())
            .collect::<String>();
        if let Ok(byte) = u8::from_str_radix(&hex, 16) {
            bytes.push(byte);
        }
        start = begin + hex.len();
    }
    bytes
}

fn parse_single_value(signature: &str, key: &str) -> Option<u8> {
    let mut search_start = 0usize;
    while let Some(pos) = signature[search_start..].find(key) {
        let absolute = search_start + pos;
        let has_token_boundary = absolute == 0
            || signature[..absolute]
                .chars()
                .next_back()
                .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
        let value_start = absolute + key.len();
        if has_token_boundary {
            let digits = signature[value_start..]
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>();
            if let Ok(value) = digits.parse() {
                return Some(value);
            }
        }
        search_start = value_start;
    }
    None
}

fn parse_value_list(signature: &str, key: &str) -> Vec<u8> {
    if let Some(single) = parse_single_value(signature, key) {
        return vec![single];
    }
    let Some(start) = signature.find(key) else {
        return Vec::new();
    };
    let rest = &signature[start + key.len()..];
    if !rest.starts_with('(') {
        return Vec::new();
    }
    let Some(end) = rest.find(')') else {
        return Vec::new();
    };
    rest[1..end]
        .split('|')
        .filter_map(|value| value.trim().parse().ok())
        .collect()
}

fn parse_opsize_variants(signature: &str) -> Vec<u8> {
    if signature.contains("(opsize=1 | opsize=2)") {
        return vec![1, 2];
    }
    if let Some(opsize) = parse_single_value(signature, "opsize=") {
        return vec![opsize];
    }
    Vec::new()
}

fn unsupported_template_reason(
    signature: &str,
    construct_tpl_kind: CompiledConstructTplKind,
    operand_specs: &[CompiledOperandSpec],
) -> Option<String> {
    if let Some(reason) = unsupported_check_constraint_reason(signature) {
        return Some(reason);
    }
    if signature.contains("currentCS")
        || signature.contains("rexRprefix=")
        || signature.contains("creg")
        || signature.contains("debugreg")
        || signature.contains("xmmmod=")
        || signature.contains("ymmmod=")
        || signature.contains("zmm")
        || signature.contains("bnd")
        || signature.contains("moffs")
    {
        return Some("unsupported_runtime_constraint".to_string());
    }
    match construct_tpl_kind {
        CompiledConstructTplKind::Unsupported => Some("unsupported_template_kind".to_string()),
        _ => {
            if operand_specs.len() > 2
                && !matches!(
                    construct_tpl_kind,
                    CompiledConstructTplKind::StackStore | CompiledConstructTplKind::StackLoad
                )
            {
                Some("unsupported_operand_arity".to_string())
            } else {
                None
            }
        }
    }
}

fn unsupported_check_constraint_reason(signature: &str) -> Option<String> {
    for token in signature.split(|ch: char| ch.is_whitespace() || ch == '&' || ch == ';') {
        let trimmed = token.trim_matches(|ch| ch == '(' || ch == ')' || ch == ',');
        if !trimmed.starts_with("check_") {
            continue;
        }
        if matches!(
            trimmed,
            "check_Reg32_dest" | "check_Rmr32_dest" | "check_rm32_dest" | "check_EAX_dest"
        ) {
            continue;
        }
        return Some("unsupported_runtime_constraint".to_string());
    }
    None
}

fn relative_size(token: &str) -> Option<u32> {
    if !token.starts_with("rel") {
        return None;
    }
    register_size_token(token)
}
fn immediate_size(token: &str) -> Option<(u32, bool)> {
    if !(token.starts_with("imm") || token.starts_with("simm")) {
        return None;
    }
    let signed = token.starts_with("simm");
    let digits = token
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    let bits = digits.parse::<u32>().ok()?;
    Some(((bits / 8).max(1), signed))
}
fn fixed_accumulator_size(token: &str) -> Option<u32> {
    match token {
        "AL" => Some(1),
        "AX" => Some(2),
        "EAX" => Some(4),
        "RAX" => Some(8),
        _ => None,
    }
}
fn register_size_token(token: &str) -> Option<u32> {
    let digits = token
        .chars()
        .rev()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    if digits.is_empty() {
        match token {
            "AL" => Some(1),
            "AX" => Some(2),
            "EAX" => Some(4),
            "RAX" => Some(8),
            "FS" | "GS" | "CS" | "SS" | "DS" | "ES" => Some(2),
            _ => None,
        }
    } else {
        digits.parse::<u32>().ok().map(|bits| (bits / 8).max(1))
    }
}

#[cfg(test)]
mod collector_define_bits_tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::Collector;

    #[test]
    fn test_parse_aarch64_token_definition() {
        let mut collector = Collector {
            definitions: Vec::new(),
            macros: Vec::new(),
            constructors: Vec::new(),
            subtable_executables: BTreeMap::new(),
            pcode_ops: BTreeSet::new(),
            pcode_op_sources: BTreeMap::new(),
            default_context: 0,
            pattern_nodes: Vec::new(),
            field_info: BTreeMap::new(),
        };
        collector.parse_define_bits(
            "define token instrAARCH64 (32) endian = little Rm = (16,20) Rn = (5,9) sf = (31,31);",
            "token",
        );
        assert_eq!(collector.field_info.get("Rm").unwrap().bit_offset, 16);
        assert_eq!(collector.field_info.get("sf").unwrap().bit_offset, 31);
    }
}
