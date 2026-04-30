//! Ghidra `ConstTpl` / `.sla` const element parity (reference-only audit).
//!
//! Vendor sources (read-only; not linked at runtime):
//! - `ConstTpl.java` — runtime `type` ints `REAL` … `J_FLOWDEST_SIZE` for `fix` / `fixSpace`.
//! - `SlaFormat.java` — `ELEM_CONST_*` element id() values for `ConstTpl.decode`.
//!
//! Fission: `decode_const_tpl` in `compiler/sla/templates.rs`.
//! Extra `CompiledConstTpl` variants `Integer` / `RelativeAddress` are IR-only (not from SLA const decode).

use std::collections::BTreeMap;

use crate::compiler::{CompiledConstTpl, CompiledHandleSelector, CompiledSpaceRef};
use crate::compiler::sla::sla_format;
use crate::compiler::sla::{PackedAttrValue, PackedElement, decode_const_tpl_for_tests};

fn elem(id: u32, attrs: &[(u32, PackedAttrValue)]) -> PackedElement {
    PackedElement {
        id,
        attrs: attrs.iter().cloned().collect(),
        children: Vec::new(),
    }
}

/// `SlaFormat.java` element id() values for const_tpl (Ghidra 12.0.4 `SlaFormat.java` L103–193).
#[test]
fn sla_const_element_ids_match_ghidra_slaformat_java() {
    assert_eq!(sla_format::ELEM_CONST_REAL, 1);
    assert_eq!(sla_format::ELEM_CONST_SPACEID, 3);
    assert_eq!(sla_format::ELEM_CONST_HANDLE, 4);
    assert_eq!(sla_format::ELEM_CONST_RELATIVE, 31);
    assert_eq!(sla_format::ELEM_CONST_START, 80);
    assert_eq!(sla_format::ELEM_CONST_NEXT, 81);
    assert_eq!(sla_format::ELEM_CONST_NEXT2, 82);
    assert_eq!(sla_format::ELEM_CONST_CURSPACE, 83);
    assert_eq!(sla_format::ELEM_CONST_CURSPACE_SIZE, 84);
    assert_eq!(sla_format::ELEM_CONST_FLOWREF, 85);
    assert_eq!(sla_format::ELEM_CONST_FLOWREF_SIZE, 86);
    assert_eq!(sla_format::ELEM_CONST_FLOWDEST, 87);
    assert_eq!(sla_format::ELEM_CONST_FLOWDEST_SIZE, 88);
}

/// Ghidra `ConstTpl.decode` accepts exactly these tags; `decode_const_tpl` must decode each with minimal attributes.
#[test]
fn decode_const_tpl_supports_exact_ghidra_const_tpl_decode_set() {
    let mut spaces = BTreeMap::new();
    spaces.insert(
        1,
        CompiledSpaceRef {
            name: "ram".to_string(),
            index: 1,
            word_size: 1,
            addr_size: 8,
            sleigh_delay_slots: 1,
            sleigh_is_ram_class: true,
            sleigh_is_unique_space: false,
        },
    );

    assert!(matches!(
        decode_const_tpl_for_tests(
            &elem(sla_format::ELEM_CONST_REAL, &[(
                sla_format::ATTR_VAL,
                PackedAttrValue::Unsigned(0x42),
            )]),
            &spaces,
        )
        .unwrap(),
        CompiledConstTpl::Real { value } if value == 0x42
    ));

    assert!(matches!(
        decode_const_tpl_for_tests(
            &elem(sla_format::ELEM_CONST_HANDLE, &[
                (sla_format::ATTR_VAL, PackedAttrValue::Signed(0)),
                (sla_format::ATTR_S, PackedAttrValue::Signed(1)),
            ]),
            &spaces,
        )
        .unwrap(),
        CompiledConstTpl::Handle {
            handle_index: 0,
            selector: CompiledHandleSelector::Offset,
            plus: None,
        }
    ));

    assert!(matches!(
        decode_const_tpl_for_tests(
            &elem(sla_format::ELEM_CONST_SPACEID, &[(
                sla_format::ATTR_SPACE,
                PackedAttrValue::SpaceIndex(1),
            )]),
            &spaces,
        )
        .unwrap(),
        CompiledConstTpl::SpaceId(ref s) if s.index == 1
    ));

    assert!(matches!(
        decode_const_tpl_for_tests(
            &elem(sla_format::ELEM_CONST_RELATIVE, &[(
                sla_format::ATTR_VAL,
                PackedAttrValue::Unsigned(3),
            )]),
            &spaces,
        )
        .unwrap(),
        CompiledConstTpl::Relative { value } if value == 3
    ));

    for (id, expect) in [
        (sla_format::ELEM_CONST_START, CompiledConstTpl::InstStart),
        (sla_format::ELEM_CONST_NEXT, CompiledConstTpl::InstNext),
        (sla_format::ELEM_CONST_NEXT2, CompiledConstTpl::InstNext2),
        (sla_format::ELEM_CONST_CURSPACE, CompiledConstTpl::CurSpace),
        (
            sla_format::ELEM_CONST_CURSPACE_SIZE,
            CompiledConstTpl::CurSpaceSize,
        ),
        (sla_format::ELEM_CONST_FLOWREF, CompiledConstTpl::FlowRef),
        (
            sla_format::ELEM_CONST_FLOWREF_SIZE,
            CompiledConstTpl::FlowRefSize,
        ),
        (sla_format::ELEM_CONST_FLOWDEST, CompiledConstTpl::FlowDest),
        (
            sla_format::ELEM_CONST_FLOWDEST_SIZE,
            CompiledConstTpl::FlowDestSize,
        ),
    ] {
        let got = decode_const_tpl_for_tests(&elem(id, &[]), &spaces).unwrap();
        assert_eq!(got, expect, "const element id {id}");
    }
}

#[test]
fn compiled_const_tpl_ir_only_variants_are_present() {
    let _ = CompiledConstTpl::Integer {
        value: 0,
        size: 1,
    };
    let _ = CompiledConstTpl::RelativeAddress;
}

/// `OperandSymbol.encode` (`Ghidra 12.0.4`) uses `ATTRIB_CODE` / `ATTRIB_MINLEN` from `SlaFormat.java` id().
#[test]
fn sla_operand_sym_attr_ids_match_ghidra_operand_symbol_encode() {
    assert_eq!(sla_format::ATTR_CODE, 7);
    assert_eq!(sla_format::ATTR_MINLEN, 18);
}
