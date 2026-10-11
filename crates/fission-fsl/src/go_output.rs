//! Go syntax projection of borrowed canonical FIR; admission is shared.
use crate::{
    control, semantics::width_mask, CompiledInstruction, FirEdge, FirOp, FirTerminator, FslError,
    IntBinaryOp, IntConversion, IntPredicate, StackContract, ValueDomain, ValueId, WaveContract,
};
use std::fmt::Write;

pub(crate) const PRELUDE: &str = r#"// Generated from canonical FIR. Bounded state model; no guest fault/concurrency model.
package fsl

func fsl_runtime_bool(value bool) uint64 { if value { return 1 }; return 0 }
func fsl_runtime_carry(left, right, carry, mask uint64) uint64 {
    return fsl_runtime_bool(right > mask-left || carry > mask-((left+right)&mask))
}
// Safe Go permits overlapping slices. Refuse overlapping output banks explicitly.
func fsl_runtime_disjoint(a, b []uint64) bool {
    if len(a)==0 || len(b)==0 { return true }
    for i := range a { if &a[i]==&b[0] { return false } }
    for i := range b { if &b[i]==&a[0] { return false } }
    return true
}
"#;
fn error(message: &str) -> FslError {
    FslError::at(1, 1, message)
}
fn literal(value: u64) -> String {
    format!("uint64(0x{value:x})")
}
fn value(id: ValueId) -> String {
    format!("v[{}]", id.0)
}
fn mask(body: &CompiledInstruction, id: ValueId) -> String {
    literal(width_mask(body.values[usize::from(id.0)].ty.bits))
}
fn field_guards(out: &mut String, body: &CompiledInstruction, fail: &str) {
    writeln!(
        out,
        "if len(fields)!={} {{ {fail} }}",
        body.encoding.fields.len()
    )
    .unwrap();
    for (i, f) in body.encoding.fields.iter().enumerate() {
        let m = width_mask(f.bits);
        let fixed = ((body.encoding.mask >> f.offset) & u128::from(m)) as u64;
        let expected = ((body.encoding.value >> f.offset) & u128::from(m)) as u64;
        if f.bits < 64 {
            writeln!(out, "if fields[{i}]>{} {{ {fail} }}", literal(m)).unwrap();
        }
        if fixed != 0 {
            writeln!(
                out,
                "if fields[{i}]&{}!={} {{ {fail} }}",
                literal(fixed),
                literal(expected)
            )
            .unwrap();
        }
        for excluded in &f.excluded {
            writeln!(out, "if fields[{i}]=={} {{ {fail} }}", literal(*excluded)).unwrap();
        }
    }
}
fn bank_guards(out: &mut String, body: &CompiledInstruction, fail: &str) {
    for op in &body.ops {
        match *op {
            FirOp::RegisterRead { field, .. } | FirOp::RegisterWrite { field, .. } => {
                writeln!(
                    out,
                    "if fields[{field}]>=uint64(len(registers)) {{ {fail} }}"
                )
                .unwrap();
            }
            FirOp::FlagRead { slot, .. } | FirOp::FlagWrite { slot, .. } => {
                writeln!(out, "if len(flags)<={} {{ {fail} }}", slot).unwrap();
            }
            _ => {}
        }
    }
    writeln!(
        out,
        "for _, flag := range flags {{ if flag>1 {{ {fail} }} }}"
    )
    .unwrap();
}
fn expression(body: &CompiledInstruction, op: FirOp) -> Option<(ValueId, String)> {
    Some(match op {
        FirOp::IntConstant { output, value: v } => (output, literal(v)),
        FirOp::IntBinary {
            output,
            left,
            right,
            op: kind,
        } => {
            let operator = match kind {
                IntBinaryOp::Sub => "-",
                IntBinaryOp::And => "&",
                IntBinaryOp::Or => "|",
                IntBinaryOp::Xor => "^",
            };
            (
                output,
                format!(
                    "({} {operator} {}) & {}",
                    value(left),
                    value(right),
                    mask(body, output)
                ),
            )
        }
        FirOp::IntAddWrap {
            output,
            left,
            right,
        } => (
            output,
            format!(
                "({}+{}) & {}",
                value(left),
                value(right),
                mask(body, output)
            ),
        ),
        FirOp::IntAddWrapCarry {
            output,
            left,
            right,
            carry,
        } => (
            output,
            format!(
                "({}+{}+{}) & {}",
                value(left),
                value(right),
                value(carry),
                mask(body, output)
            ),
        ),
        FirOp::IntAddCarry {
            output,
            left,
            right,
        } => (
            output,
            format!(
                "fsl_runtime_carry({},{},0,{})",
                value(left),
                value(right),
                mask(body, left)
            ),
        ),
        FirOp::IntAddCarryIn {
            output,
            left,
            right,
            carry,
        } => (
            output,
            format!(
                "fsl_runtime_carry({},{},{},{})",
                value(left),
                value(right),
                value(carry),
                mask(body, left)
            ),
        ),
        FirOp::IntConvert {
            output,
            input,
            kind,
        } => {
            let e = if kind == IntConversion::SignExtend {
                let bits = body.values[usize::from(input.0)].ty.bits;
                format!(
                    "({} | ((uint64(0)-({} >> {})) & ^{}))",
                    value(input),
                    value(input),
                    bits - 1,
                    mask(body, input)
                )
            } else {
                value(input)
            };
            (output, format!("{e} & {}", mask(body, output)))
        }
        FirOp::IntCompare {
            output,
            left,
            right,
            predicate,
        } => {
            let condition = match predicate {
                IntPredicate::Equal => format!("{}=={}", value(left), value(right)),
                IntPredicate::UnsignedLess => format!("{}<{}", value(left), value(right)),
                IntPredicate::SignedLess => {
                    let sign = literal(1u64 << (body.values[usize::from(left.0)].ty.bits - 1));
                    format!("({}^{sign})<({}^{sign})", value(left), value(right))
                }
            };
            (output, format!("fsl_runtime_bool({condition})"))
        }
        _ => return None,
    })
}
fn edge(out: &mut String, e: &FirEdge, blocks: &[crate::FirBlock]) {
    let params = &blocks[usize::from(e.target)].parameters;
    if !params.is_empty() {
        // Go evaluates all RHS values before assigning block parameters.
        writeln!(
            out,
            "{} = {}",
            params
                .iter()
                .map(|v| value(*v))
                .collect::<Vec<_>>()
                .join(","),
            e.arguments
                .iter()
                .map(|v| value(*v))
                .collect::<Vec<_>>()
                .join(",")
        )
        .unwrap();
    }
    writeln!(out, "block={}; continue", e.target).unwrap();
}
fn terminator(out: &mut String, t: &FirTerminator, blocks: &[crate::FirBlock], success: &str) {
    match t {
        FirTerminator::Return => {
            writeln!(out, "{success}").unwrap();
        }
        FirTerminator::Branch(e) => edge(out, e, blocks),
        FirTerminator::CondBranch {
            condition,
            on_true,
            on_false,
        } => {
            writeln!(out, "if {}!=0 {{", value(*condition)).unwrap();
            edge(out, on_true, blocks);
            out.push_str("} else {\n");
            edge(out, on_false, blocks);
            out.push_str("}\n");
        }
    }
}
pub(crate) fn instruction(body: &CompiledInstruction, symbol: &str) -> Result<String, FslError> {
    let mut out = PRELUDE.to_string();
    if body.ops.iter().any(FirOp::requires_lane_version) {
        out += &wave(body, symbol)?;
    } else if body.ops.iter().any(FirOp::requires_state_version) {
        crate::state::admit(body)?;
        out += &state_body(body, &format!("{symbol}_body"))?;
        writeln!(out,"func {symbol}(registers, flags, fields []uint64) uint32 {{\nif !fsl_runtime_disjoint(registers,flags) {{ return 3 }}\nr:=append([]uint64(nil),registers...); f:=append([]uint64(nil),flags...); fields=append([]uint64(nil),fields...)\nstatus,_,_ := {symbol}_body(r,f,fields,0,nil,0)\nif status==0 {{ copy(registers,r); copy(flags,f) }}\nreturn status\n}}").unwrap();
    } else {
        out += &stack(body, symbol)?;
    }
    Ok(out)
}
fn stack(body: &CompiledInstruction, symbol: &str) -> Result<String, FslError> {
    let contract = StackContract::for_instruction(body)?;
    let blocks = control::blocks(body);
    let mut out=format!("func {symbol}(stack []uint64, depth *int) uint32 {{\nif depth==nil || *depth<0 || *depth>len(stack) {{ return 3 }}\nif *depth<{} {{ return 1 }}\nif {}>len(stack)-*depth {{ return 2 }}\ns:=append([]uint64(nil),stack...); sp:=*depth; _=sp\nvar v [{}]uint64; _=v\nblock:=0\nfor {{ switch block {{\n",contract.required_input,contract.extra_capacity,body.values.len());
    for (i, b) in blocks.iter().enumerate() {
        writeln!(out, "case {i}:").unwrap();
        for op in &body.ops[usize::from(b.start)..usize::from(b.end)] {
            if let Some((id, e)) = expression(body, *op) {
                writeln!(out, "{}={e}", value(id)).unwrap();
                continue;
            }
            match *op {
                FirOp::VmStackPop { output } => {
                    writeln!(out, "sp--; {}=s[sp]&{}", value(output), mask(body, output)).unwrap();
                }
                FirOp::VmStackPush { value: v } => {
                    writeln!(out, "s[sp]={}; sp++", value(v)).unwrap();
                }
                _ => return Err(error("unsupported Go stack operation")),
            }
        }
        terminator(
            &mut out,
            &b.terminator,
            &blocks,
            "copy(stack,s); *depth=sp; return 0",
        );
    }
    out.push_str("default: return 3\n} }\n}\n");
    Ok(out)
}
/// Private body works on a caller-owned transaction snapshot, not guest storage.
pub(crate) fn state_body(body: &CompiledInstruction, symbol: &str) -> Result<String, FslError> {
    crate::state::admit_context(body, true, true)?;
    let fail = "return 3,0,false";
    let mut out=format!("func {symbol}(registers, flags, fields []uint64, guest_pc uint64, memory []byte, memory_base uint64) (uint32,uint64,bool) {{\n");
    field_guards(&mut out, body, fail);
    bank_guards(&mut out, body, fail);
    writeln!(
        out,
        "var v [{}]uint64; _=v\nvar next uint64; changed:=false\nblock:=0\nfor {{ switch block {{",
        body.values.len()
    )
    .unwrap();
    let blocks = control::blocks(body);
    for (i, b) in blocks.iter().enumerate() {
        writeln!(out, "case {i}:").unwrap();
        for op in &body.ops[usize::from(b.start)..usize::from(b.end)] {
            if let Some((id, e)) = expression(body, *op) {
                writeln!(out, "{}={e}", value(id)).unwrap();
                continue;
            }
            match *op {
                FirOp::RegisterRead { output, field } => {
                    writeln!(
                        out,
                        "{}=registers[fields[{field}]]&{}",
                        value(output),
                        mask(body, output)
                    )
                    .unwrap();
                }
                FirOp::RegisterWrite { field, value: v } => {
                    writeln!(out, "registers[fields[{field}]]={}", value(v)).unwrap();
                }
                FirOp::FlagRead { output, slot } => {
                    writeln!(out, "{}=flags[{slot}]", value(output)).unwrap();
                }
                FirOp::FlagWrite { slot, value: v } => {
                    writeln!(out, "flags[{slot}]={}", value(v)).unwrap();
                }
                FirOp::FieldRead { output, field } => {
                    writeln!(
                        out,
                        "{}=fields[{field}]&{}",
                        value(output),
                        mask(body, output)
                    )
                    .unwrap();
                }
                FirOp::GuestPcRead { output } => {
                    writeln!(out, "{}=guest_pc", value(output)).unwrap();
                }
                FirOp::GuestNextPcWrite { value: v } => {
                    writeln!(
                        out,
                        "if changed {{ {fail} }}; next={}; changed=true",
                        value(v)
                    )
                    .unwrap();
                }
                FirOp::MemoryLoadLittle { output, address } => {
                    let n = body.values[usize::from(output.0)].ty.bits / 8;
                    let a = value(address);
                    writeln!(out,"if {a}<memory_base || {a}-memory_base>uint64(len(memory)) || uint64(len(memory))-({a}-memory_base)<{n} {{ {fail} }}").unwrap();
                    let e = (0..n)
                        .map(|j| format!("(uint64(memory[{}-memory_base+{j}])<<{})", a, j * 8))
                        .collect::<Vec<_>>()
                        .join("|");
                    writeln!(out, "{}={e}", value(output)).unwrap();
                }
                FirOp::MemoryStoreLittle { address, value: v } => {
                    let n = body.values[usize::from(v.0)].ty.bits / 8;
                    let a = value(address);
                    writeln!(out,"if {a}<memory_base || {a}-memory_base>uint64(len(memory)) || uint64(len(memory))-({a}-memory_base)<{n} {{ {fail} }}").unwrap();
                    for j in 0..n {
                        writeln!(
                            out,
                            "memory[{a}-memory_base+{j}]=byte({}>>{})",
                            value(v),
                            j * 8
                        )
                        .unwrap();
                    }
                }
                _ => return Err(error("unsupported Go state operation")),
            }
        }
        terminator(&mut out, &b.terminator, &blocks, "return 0,next,changed");
    }
    out.push_str("default: return 3,0,false\n} }\n}\n");
    Ok(out)
}
fn wave(body: &CompiledInstruction, symbol: &str) -> Result<String, FslError> {
    let contract = WaveContract::for_instruction(body)?;
    let n = contract.lanes;
    let mut out=format!("func {symbol}(registers, flags, lane_registers []uint64, lanes uint16, exec uint64, fields []uint64) uint32 {{\nif lanes!={n} || exec & ^{}!=0 || len(lane_registers)%{n}!=0 {{ return 3 }}\nif !fsl_runtime_disjoint(registers,flags) || !fsl_runtime_disjoint(registers,lane_registers) || !fsl_runtime_disjoint(flags,lane_registers) {{ return 3 }}\n",literal(width_mask(n)));
    field_guards(&mut out, body, "return 3");
    bank_guards(&mut out, body, "return 3");
    for op in &body.ops {
        match *op {
            FirOp::LaneRead { field, bias, .. } => {
                writeln!(out,"if fields[{field}]<{} || fields[{field}]-{}>=uint64(len(lane_registers)/{n}) {{ return 3 }}",literal(bias),literal(bias)).unwrap();
            }
            FirOp::LaneWrite { field, .. } => {
                writeln!(
                    out,
                    "if fields[{field}]>=uint64(len(lane_registers)/{n}) {{ return 3 }}"
                )
                .unwrap();
            }
            _ => {}
        }
    }
    writeln!(
        out,
        "fields=append([]uint64(nil),fields...)\nvar v [{}][{n}]uint64; _=v",
        body.values.len()
    )
    .unwrap();
    let v = |id: ValueId, lane: bool| {
        format!(
            "v[{}][{}]",
            id.0,
            if lane && matches!(contract.domains[usize::from(id.0)], ValueDomain::Lanes(_)) {
                "lane"
            } else {
                "0"
            }
        )
    };
    for op in &body.ops {
        match *op {
            FirOp::LaneMaskRead { output, .. } => {
                writeln!(out, "{}=exec", v(output, false)).unwrap();
            }
            FirOp::RegisterRead { output, field } => {
                writeln!(
                    out,
                    "{}=registers[fields[{field}]]&{}",
                    v(output, false),
                    mask(body, output)
                )
                .unwrap();
            }
            FirOp::RegisterWrite { field, value } => {
                writeln!(out, "registers[fields[{field}]]={}", v(value, false)).unwrap();
            }
            FirOp::FlagRead { output, slot } => {
                writeln!(out, "{}=flags[{slot}]", v(output, false)).unwrap();
            }
            FirOp::FlagWrite { slot, value } => {
                writeln!(out, "flags[{slot}]={}", v(value, false)).unwrap();
            }
            FirOp::LaneRead {
                output,
                field,
                bias,
                ..
            } => {
                writeln!(out,"for lane:=0;lane<{n};lane++ {{ {}=lane_registers[(fields[{field}]-{})*{n}+uint64(lane)]&{} }}",v(output,true),literal(bias),mask(body,output)).unwrap();
            }
            FirOp::LaneWrite {
                field,
                value,
                mask: m,
            } => {
                writeln!(out,"for lane:=0;lane<{n};lane++ {{ if {} & (uint64(1)<<uint(lane))!=0 {{ lane_registers[fields[{field}]*{n}+uint64(lane)]={} }} }}",v(m,false),v(value,true)).unwrap();
            }
            FirOp::IntAddWrap {
                output,
                left,
                right,
            } => {
                let statement = format!(
                    "{}=({}+{})&{}",
                    v(output, true),
                    v(left, true),
                    v(right, true),
                    mask(body, output)
                );
                if matches!(
                    contract.domains[usize::from(output.0)],
                    ValueDomain::Lanes(_)
                ) {
                    writeln!(out, "for lane:=0;lane<{n};lane++ {{ {statement} }}").unwrap();
                } else {
                    writeln!(out, "{statement}").unwrap();
                }
            }
            _ => return Err(error("unsupported Go wave operation")),
        }
    }
    out.push_str("return 0\n}\n");
    Ok(out)
}
