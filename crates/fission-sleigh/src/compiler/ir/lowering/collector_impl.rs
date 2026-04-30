impl Collector {
    fn language_layout(&self) -> CompiledLanguageLayout {
        let mut address_spaces = Vec::new();
        let mut registers = Vec::new();
        let mut token_fields = Vec::new();
        let mut context_fields = Vec::new();
        let mut subtables = Vec::new();
        for definition in &self.definitions {
            match definition.kind.as_str() {
                "space" => address_spaces.push(CompiledAddressSpace {
                    name: definition_name(&definition.statement),
                    source: definition.source.clone(),
                }),
                "register" => registers.push(CompiledRegister {
                    name: definition_name(&definition.statement),
                    source: definition.source.clone(),
                }),
                "token" => {
                    let name = definition_name(&definition.statement);
                    let info = self.field_info.get(&name);
                    token_fields.push(CompiledTokenField {
                        name,
                        bit_offset: info.map(|i| i.bit_offset).unwrap_or(0),
                        bit_width: info.map(|i| i.bit_width).unwrap_or(0),
                        source: definition.source.clone(),
                    })
                }
                "context" => {
                    let name = definition_name(&definition.statement);
                    let info = self.field_info.get(&name);
                    context_fields.push(CompiledContextField {
                        name,
                        bit_offset: info.map(|i| i.bit_offset).unwrap_or(0),
                        bit_width: info.map(|i| i.bit_width).unwrap_or(0),
                        source: definition.source.clone(),
                    })
                }
                "table" => subtables.push(CompiledSubtable {
                    name: definition_name(&definition.statement),
                    source: definition.source.clone(),
                }),
                _ => {}
            }
        }
        let display_templates = self
            .constructors
            .iter()
            .map(|constructor| CompiledDisplayTemplate {
                constructor_hash: constructor.signature_hash,
                pieces: Vec::new(),
                first_whitespace: None,
                flowthru_operand_index: None,
                display: constructor.display.clone(),
            })
            .collect();
        CompiledLanguageLayout {
            address_spaces,
            registers,
            token_fields,
            context_fields,
            subtables,
            display_templates,
        }
    }

    fn construct_templates(&self) -> Vec<CompiledConstructTpl> {
        self.subtable_executables
            .values()
            .flatten()
            .map(|constructor| CompiledConstructTpl {
                constructor_hash: constructor.signature_hash,
                num_labels: constructor.constructor_template.num_labels,
                result: constructor.constructor_template.result.clone(),
                ops: constructor.constructor_template.ops.clone(),
            })
            .collect()
    }

    fn collect_items(&mut self, items: &[AstItem], with_stack: &mut Vec<WithContextFrame>) {
        for item in items {
            match item {
                AstItem::Define(definition) => {
                    let kind = definition
                        .statement
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("unknown")
                        .trim_end_matches(';')
                        .to_string();
                    let source = format!(
                        "{}:{}",
                        definition
                            .file
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("<unknown>"),
                        definition.line_number
                    );
                    if kind == "pcodeop" {
                        if let Some(name) = definition
                            .statement
                            .split_whitespace()
                            .nth(2)
                            .map(|value| value.trim_end_matches(';').to_string())
                        {
                            self.pcode_ops.insert(name.clone());
                            self.pcode_op_sources.insert(name, source.clone());
                        }
                    }
                    if kind == "token" || kind == "context" {
                        self.parse_define_bits(&definition.statement, &kind);
                    }
                    self.definitions.push(CompiledSpecDefinition {
                        kind,
                        source,
                        statement: definition.statement.clone(),
                    });
                }
                AstItem::Macro(m) => {
                    self.macros.push(CompiledMacro {
                        name: macro_name(&m.signature),
                        source: format!(
                            "{}:{}",
                            m.file
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("<unknown>"),
                            m.line_number
                        ),
                        body_line_count: m.body.lines().count(),
                    });
                }
                AstItem::Constructor(constructor) => {
                    self.record_constructor(constructor, with_stack);
                }
                AstItem::WithBlock(with) => {
                    with_stack.push(WithContextFrame {
                        header: with.header.clone(),
                    });
                    self.collect_items(&with.items, with_stack);
                    with_stack.pop();
                }
                AstItem::Raw(_) => {}
            }
        }
    }

    fn collect_define_bits_from_expanded(&mut self, lines: &[PreprocessedLine]) {
        let mut pending: Option<(String, String)> = None;

        for line in lines {
            let trimmed = strip_comments(&line.text).trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some((kind, statement)) = pending.as_mut() {
                statement.push('\n');
                statement.push_str(trimmed);
                if trimmed.contains(';') {
                    let (kind, statement) = pending.take().expect("pending define bits");
                    self.parse_define_bits(&statement, &kind);
                }
                continue;
            }

            let Some(kind) = define_bits_kind(trimmed) else {
                continue;
            };
            if trimmed.contains(';') {
                self.parse_define_bits(trimmed, kind);
            } else {
                pending = Some((kind.to_string(), trimmed.to_string()));
            }
        }

        if let Some((kind, statement)) = pending {
            self.parse_define_bits(&statement, &kind);
        }
    }

    fn record_constructor(
        &mut self,
        constructor: &AstConstructor,
        with_stack: &[WithContextFrame],
    ) {
        // Hierarchical subtable name extraction
        let mut table_name = "instruction".to_string();
        for frame in with_stack {
            let header = frame.header.trim();
            if let Some(pos) = header.find(':') {
                let name = header[..pos].trim();
                if !name.is_empty()
                    && name.len() <= 64
                    && !name.contains(' ')
                    && !name.contains('=')
                {
                    table_name = name.to_string();
                }
            }
        }
        if let Some(pos) = constructor.signature.find(':') {
            let name = constructor.signature[..pos].trim();
            if !name.is_empty() && name.len() <= 64 && !name.contains(' ') && !name.contains('=') {
                table_name = name.to_string();
            }
        }

        let mnemonic = constructor_mnemonic(&constructor.signature);
        let source = format!(
            "{}:{}",
            constructor
                .file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("<unknown>"),
            constructor.line_number
        );
        let control_flow = classify_control_flow(&constructor.body);
        let semantic_ops = constructor_semantic_ops(&constructor.body, &self.pcode_ops);
        let signature_hash = stable_hash(&constructor.signature);
        let semantic_template = CompiledSemanticTemplate {
            status: if constructor.body.trim().is_empty() {
                "empty".to_string()
            } else {
                "unsupported_template".to_string()
            },
            action_hash: stable_hash(&constructor.body),
            op_count: semantic_ops.len(),
        };

        self.pattern_nodes.push(CompiledPatternNode {
            node_id: format!("{source}#{:016x}", signature_hash),
            source: source.clone(),
            mnemonic: mnemonic.clone(),
            with_depth: with_stack.len(),
            control_flow,
        });

        let mut full_signature = String::new();
        for frame in with_stack {
            full_signature.push_str(&frame.header);
            full_signature.push_str(" & ");
        }
        full_signature.push_str(&constructor.signature);

        let context_changes = parse_context_changes(&full_signature, &self.field_info);

        self.constructors.push(CompiledConstructor {
            mnemonic: mnemonic.clone(),
            display: constructor.signature.clone(),
            source: source.clone(),
            control_flow,
            pattern_signature: constructor.signature.clone(),
            semantic_template,
            with_stack: with_stack
                .iter()
                .map(|frame| frame.header.clone())
                .collect(),
            semantic_ops,
            signature_hash,
            context_changes: context_changes.clone(),
        });

        if let Some(executable) = self.compile_executable_constructor(
            &full_signature,
            &mnemonic,
            &source,
            signature_hash,
            context_changes,
        ) {
            let mut executable = executable;
            executable.constructor_id = u32::MAX; // To be set by apply_sla
            self.subtable_executables
                .entry(table_name)
                .or_default()
                .push(executable);
        }
    }

    fn compile_executable_constructor(
        &self,
        signature: &str,
        mnemonic: &str,
        source: &str,
        signature_hash: u64,
        context_changes: Vec<CompiledContextOp>,
    ) -> Option<CompiledExecutableConstructor> {
        if !runtime_signature_is_supported(signature) {
            return None;
        }
        let normalized_mnemonic = normalize_executable_mnemonic(mnemonic);
        let construct_tpl_kind = classify_display_construct_kind(&normalized_mnemonic);
        let matcher = self
            .parse_opcode_matcher(signature)
            .unwrap_or_else(|| CompiledPatternMatcher::BitConstraints(vec![]));
        let mod_constraint = parse_single_value(signature, "mod=");
        let operand_reg_values = parse_value_list(signature, "reg=");
        let opsize_variants = parse_opsize_variants(signature);
        let operand_specs = parse_operand_specs(signature, &matcher, construct_tpl_kind).ok()?;
        let hidden_subtables = parse_hidden_subtables(signature, &self.field_info);
        let mut decode_steps = Vec::new();
        if !hidden_subtables.is_empty() && operand_specs.is_empty() {
            decode_steps.extend(hidden_subtables.into_iter().map(|table_name| {
                CompiledOperandDecodeStep::DescendSubtable {
                    table_name,
                    replace_current: true,
                }
            }));
        }
        decode_steps.extend(
            (0..operand_specs.len())
                .map(|operand_index| CompiledOperandDecodeStep::DecodeOperand { operand_index }),
        );

        let constructor_template = CompiledConstructorTemplate {
            handles: operand_specs
                .iter()
                .cloned()
                .enumerate()
                .map(|(operand_index, spec)| CompiledHandleTemplate {
                    operand_index,
                    spec,
                    sla_operand_symbol_meta: SlaOperandSymbolMeta::default(),
                })
                .collect(),
            decode_steps,
            num_labels: 0,
            result: None,
            ops: Vec::new(),
            template_source: CompiledTemplateSource::SpecDerived,
        };

        Some(CompiledExecutableConstructor {
            constructor_id: u32::MAX,
            sla_identity: None,
            sla_decode_status: CompiledSlaDecodeStatus::Unsupported,
            mnemonic: mnemonic.to_string(),
            source: source.to_string(),
            display: signature.to_string(),
            display_template: CompiledDisplayTemplate::fallback(signature.to_string()),
            signature_hash,
            minimum_length: native_matcher_minimum_length(&matcher) as u32,
            context_changes,
            matcher,
            mod_constraint,
            operand_reg_values,
            opsize_variants,
            operand_specs: operand_specs.clone(),
            display_operands: operand_specs
                .iter()
                .enumerate()
                .map(|(operand_index, _)| CompiledDisplayOperand {
                    operand_index,
                    kind: CompiledDisplayOperandKind::Generic,
                })
                .collect(),
            construct_tpl_kind,
            constructor_template,
            named_templates: Vec::new(),
            context_commits: Vec::new(),
            runtime_ready: false,
            unsupported_template_kind: Some(
                unsupported_template_reason(signature, construct_tpl_kind, &operand_specs)
                    .unwrap_or_else(|| "missing_sla_construct_tpl".to_string()),
            ),
        })
    }

    fn parse_opcode_matcher(&self, signature: &str) -> Option<CompiledPatternMatcher> {
        let (pattern_part, _context_block) = if let Some(is_pos) = signature.find(" is ") {
            let rest = &signature[is_pos + 4..];
            if let Some(bracket_pos) = rest.find('[') {
                (&rest[..bracket_pos], Some(&rest[bracket_pos..]))
            } else {
                (rest, None)
            }
        } else {
            (signature, None)
        };

        let mut constraints = Vec::new();
        for part in pattern_part.split(['&', ';', '\n']) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((name, value_str)) = part.split_once('=') {
                let name = name.trim();
                let value_str = value_str.trim();
                let value = if value_str.starts_with("0x") {
                    u64::from_str_radix(&value_str[2..], 16).unwrap_or(0)
                } else if value_str.starts_with("0b") {
                    u64::from_str_radix(&value_str[2..], 2).unwrap_or(0)
                } else {
                    value_str.parse::<u64>().unwrap_or(0)
                };

                if let Some(info) = self.field_info.get(name) {
                    let field_mask = if info.bit_width >= 64 {
                        u64::MAX
                    } else {
                        (1u64 << info.bit_width) - 1
                    };
                    let mask = field_mask.checked_shl(info.bit_offset).unwrap_or(0);
                    let shifted_value = value.checked_shl(info.bit_offset).unwrap_or(0) & mask;
                    match info.kind {
                        FieldKind::Instruction => {
                            constraints.push(PatternConstraint::Instruction {
                                offset: 0,
                                mask,
                                value: shifted_value,
                            })
                        }
                        FieldKind::Context => constraints.push(PatternConstraint::Context {
                            offset: 0,
                            mask,
                            value: shifted_value,
                        }),
                    }
                } else if name.starts_with("b_") {
                    if let Ok(bits) = name[2..].parse::<u32>() {
                        let (s, e) = if name.len() <= 4 {
                            (bits / 100, bits % 100)
                        } else {
                            (bits, bits)
                        };
                        let (start_bit, end_bit) = if s > e { (s, e) } else { (e, s) };
                        let width = start_bit - end_bit + 1;
                        let field_mask = if width >= 64 {
                            u64::MAX
                        } else {
                            (1u64 << width) - 1
                        };
                        let mask = field_mask.checked_shl(end_bit).unwrap_or(0);
                        let shifted_value = value.checked_shl(end_bit).unwrap_or(0) & mask;
                        constraints.push(PatternConstraint::Instruction {
                            offset: 0,
                            mask,
                            value: shifted_value,
                        });
                    }
                }
            } else {
                let name = part.trim_start_matches('~').trim();
                let is_negated = part.trim().starts_with('~');
                if let Some(info) = self.field_info.get(name) {
                    if info.bit_width == 1 {
                        let mask = 1u64 << info.bit_offset;
                        let value = if is_negated { 0 } else { mask };
                        match info.kind {
                            FieldKind::Instruction => {
                                constraints.push(PatternConstraint::Instruction {
                                    offset: 0,
                                    mask,
                                    value,
                                })
                            }
                            FieldKind::Context => constraints.push(PatternConstraint::Context {
                                offset: 0,
                                mask,
                                value,
                            }),
                        }
                    }
                }
            }
        }
        if !constraints.is_empty() {
            return Some(CompiledPatternMatcher::BitConstraints(constraints));
        }
        let bytes = parse_byte_sequence(signature);
        if !bytes.is_empty() {
            return Some(CompiledPatternMatcher::ExactBytes(bytes));
        }
        if signature.contains(" is ") {
            return Some(CompiledPatternMatcher::BitConstraints(vec![]));
        }
        None
    }

    fn parse_define_bits(&mut self, statement: &str, kind_str: &str) {
        let trimmed = strip_comments(statement).trim();
        let kind = match kind_str {
            "token" => FieldKind::Instruction,
            "context" => FieldKind::Context,
            _ => return,
        };
        let first_line_end = trimmed.find('\n').unwrap_or(trimmed.len());
        let start_pos = if trimmed[..first_line_end].contains('(') {
            if let Some(pos) = trimmed.find(')') {
                pos + 1
            } else {
                return;
            }
        } else {
            first_line_end
        };
        let fields_str = trimmed[start_pos..].trim_end_matches(';');
        for (pos, _) in fields_str.match_indices('(') {
            let left = fields_str[..pos].trim();
            let name = if let Some(last) = left.split_whitespace().last() {
                let n = last.trim_end_matches('=').trim();
                if n.is_empty() {
                    left.split_whitespace().rev().nth(1).unwrap_or("")
                } else {
                    n
                }
            } else {
                ""
            };
            if name.is_empty() || name == "endian" {
                continue;
            }
            let right = &fields_str[pos + 1..];
            if let Some(end_pos) = right.find(')') {
                let range_part = &right[..end_pos];
                if let Some((start_str, end_str)) = range_part.split_once(',') {
                    let start = start_str.trim().parse::<u32>().unwrap_or(0);
                    let end = end_str.trim().parse::<u32>().unwrap_or(0);
                    let (bit_offset, bit_width) = if start <= end {
                        (start, end - start + 1)
                    } else {
                        (end, start - end + 1)
                    };
                    self.field_info.insert(
                        name.to_string(),
                        FieldBitRange {
                            bit_offset,
                            bit_width,
                            kind,
                        },
                    );
                }
            }
        }
    }
}
