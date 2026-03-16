use super::PostProcessor;
use super::condition::negate_condition;
use fission_loader::loader::types::{InferredFieldInfo, InferredTypeInfo};

#[test]
fn test_switch_from_if_else_assign_multiline() {
    let input = r#"undefined8 __Z8day_namei(int param_1)
{
  undefined8 result;
  if (!param_1) {
    result = "Sunday";
  }
  else if (param_1 == 1) {
    result = "Monday";
  }
  else if (param_1 == 2) {
    result = "Tuesday";
  }
  else if (param_1 == 3) {
    result = "Wednesday";
  }
  else {
    result = "Unknown";
  }
  return result;
}"#;
    let output = PostProcessor::reconstruct_switch_from_if_else_assign(input);
    eprintln!("OUTPUT:\n{}", output);
    assert!(output.contains("switch"), "must contain switch: {}", output);
    assert!(output.contains("case"), "must contain case: {}", output);
    assert!(output.contains("return"), "must contain return: {}", output);
}

#[test]
fn test_cluster_switch_case_runs_with_identical_bodies() {
    let input = r#"switch (x) {
case 1:
  goto label_out;
case 2:
  goto label_out;
case 3:
  goto label_out;
default:
  break;
}"#;

    let output = PostProcessor::cluster_switch_case_runs(input);
    assert_eq!(
        output.matches("goto label_out;").count(),
        1,
        "must keep only one shared goto: {}",
        output
    );
    assert!(
        output.contains("case 1:"),
        "must preserve case labels: {}",
        output
    );
    assert!(
        output.contains("case 2:"),
        "must preserve clustered case labels: {}",
        output
    );
    assert!(
        output.contains("case 3:"),
        "must preserve clustered case labels: {}",
        output
    );
}

#[test]
fn test_cluster_switch_case_runs_with_linear_assignment() {
    let input = r#"switch (x) {
case 0x61:
  y = 0x31;
  z = 1;
  goto label_out;
case 0x62:
  y = 0x32;
  z = 1;
  goto label_out;
case 0x63:
  y = 0x33;
  z = 1;
  goto label_out;
default:
  break;
}"#;

    let output = PostProcessor::cluster_switch_case_runs(input);
    assert_eq!(
        output.matches("goto label_out;").count(),
        1,
        "must share tail goto once: {}",
        output
    );
    assert_eq!(
        output.matches("z = 1;").count(),
        1,
        "must keep common suffix once: {}",
        output
    );
    assert!(
        output.contains("y = (ulonglong)(x) - 0x30;"),
        "must synthesize linear assignment from switch variable: {}",
        output
    );
}

#[test]
fn test_cluster_switch_case_runs_with_linear_assignment_and_final_fallthrough() {
    let input = r#"switch (x) {
case 0x61:
  y = 0x31;
  z = 1;
  goto label_out;
case 0x62:
  y = 0x32;
  z = 1;
  goto label_out;
case 0x63:
  y = 0x33;
  z = 1;
label_out:
  return y;
}"#;

    let output = PostProcessor::cluster_switch_case_runs(input);
    assert_eq!(
        output.matches("goto label_out;").count(),
        0,
        "must use shared fallthrough tail: {}",
        output
    );
    assert_eq!(
        output.matches("z = 1;").count(),
        1,
        "must keep shared prefix once: {}",
        output
    );
    assert!(
        output.contains("y = (ulonglong)(x) - 0x30;"),
        "must synthesize linear assignment for clustered fallthrough cases: {}",
        output
    );
}

#[test]
fn test_negate_condition_basic_cases() {
    assert_eq!(negate_condition("x >= 10"), "x < 10");
    assert_eq!(negate_condition("!done"), "done");
    assert_eq!(negate_condition("x == 0"), "x != 0");
    assert_eq!(
        negate_condition("complex_call(a, b)"),
        "!(complex_call(a, b))"
    );
}

#[test]
fn test_while_true_to_while_cond_simple() {
    let input = r#"while (true) {
  if (i >= n) break;
  sum = sum + i;
}"#;

    let output = PostProcessor::while_true_to_while_cond(input);
    assert!(
        output.contains("while (i < n)"),
        "must negate break condition: {}",
        output
    );
    assert!(
        output.contains("sum = sum + i;"),
        "must preserve body: {}",
        output
    );
}

#[test]
fn test_while_true_to_for_loop_simple() {
    let input = r#"i = 0;
while (true) {
  if (i >= n) break;
  sum = sum + i;
  i++;
}"#;

    let output = PostProcessor::while_true_to_for_loop(input);
    assert!(
        output.contains("for (i = 0; i < n; i++)"),
        "must convert to for-loop header: {}",
        output
    );
    assert!(
        output.contains("sum = sum + i;"),
        "must preserve loop body: {}",
        output
    );
}

#[test]
fn test_mul_pow2_to_shift_bitwise_context() {
    let input = r#"x = a * 0x100 | b;
y = c * 8 + d;"#;

    let output = PostProcessor::mul_pow2_to_shift(input);
    assert!(
        output.contains("a << 8 | b"),
        "must convert in bitwise context: {}",
        output
    );
    assert!(
        output.contains("c * 8 + d"),
        "must not convert in non-bitwise context: {}",
        output
    );
}

#[test]
fn test_goto_cleanup_removes_self_fallthrough_goto() {
    let input = r#"int test(void)
{
  goto label_1;
label_1:
  return 1;
}"#;

    let output = PostProcessor::cleanup_gotos(input);
    assert!(
        !output.contains("goto label_1;"),
        "must remove no-op goto: {}",
        output
    );
    assert!(
        !output.contains("label_1:"),
        "must remove dead label after cleanup: {}",
        output
    );
    assert!(
        output.contains("return 1;"),
        "must preserve return body: {}",
        output
    );
}

#[test]
fn test_goto_cleanup_inlines_single_use_label_block() {
    let input = r#"int test(int x)
{
  if (x == 0) {
    goto label_1;
  }
  return 2;
label_1:
  return 1;
}"#;

    let output = PostProcessor::cleanup_gotos(input);
    assert!(
        !output.contains("goto label_1;"),
        "must inline single-use label body: {}",
        output
    );
    assert!(
        !output.contains("label_1:"),
        "must remove inlined label: {}",
        output
    );
    assert!(
        output.contains("return 1;"),
        "must preserve inlined return: {}",
        output
    );
}

#[test]
fn test_goto_cleanup_folds_canonical_if_else() {
    let input = r#"int test(int x)
{
  int y;
  if (x == 0) goto label_true;
  goto label_false;
label_true:
  y = 1;
  goto label_end;
label_false:
  y = 2;
label_end:
  return y;
}"#;

    let output = PostProcessor::cleanup_gotos(input);
    assert!(
        output.contains("if (x == 0) {"),
        "must reconstruct if block: {}",
        output
    );
    assert!(
        output.contains("} else {"),
        "must reconstruct else block: {}",
        output
    );
    assert!(
        !output.contains("goto label_true;"),
        "must eliminate then goto: {}",
        output
    );
    assert!(
        !output.contains("goto label_false;"),
        "must eliminate else goto: {}",
        output
    );
    assert!(
        !output.contains("label_true:"),
        "must eliminate then label: {}",
        output
    );
    assert!(
        !output.contains("label_false:"),
        "must eliminate else label: {}",
        output
    );
    assert!(
        !output.contains("label_end:"),
        "must eliminate join label: {}",
        output
    );
}

#[test]
fn test_goto_cleanup_folds_guarded_if_goto() {
    let input = r#"int test(int x)
{
  if (x == 0) goto label_end;
  x = x + 1;
  return x;
label_end:
  return 0;
}"#;

    let output = PostProcessor::cleanup_gotos(input);
    assert!(
        output.contains("if (x != 0) {"),
        "must negate guarded goto into if-body: {}",
        output
    );
    assert!(
        output.contains("x = x + 1;"),
        "must keep guarded body: {}",
        output
    );
    assert!(
        output.contains("return x;"),
        "must keep guarded return: {}",
        output
    );
    assert!(
        !output.contains("goto label_end;"),
        "must remove guarded goto: {}",
        output
    );
}

#[test]
fn test_goto_cleanup_threads_chained_goto_labels() {
    let input = r#"int test(int x)
{
  if (x == 0) goto label_1;
  return 2;
label_1:
  goto label_2;
label_2:
  return 1;
}"#;

    let output = PostProcessor::cleanup_gotos(input);
    assert!(
        !output.contains("goto label_1;"),
        "must retarget chained goto entry: {}",
        output
    );
    assert!(
        !output.contains("goto label_2;"),
        "must remove intermediate goto-only label: {}",
        output
    );
    assert!(
        !output.contains("label_1:"),
        "must remove dead intermediate label: {}",
        output
    );
    assert!(
        output.contains("return 1;"),
        "must preserve final destination body: {}",
        output
    );
}

#[test]
fn test_goto_cleanup_inlines_multi_use_terminal_label() {
    let input = r#"int test(int x)
{
  if (x == 0) goto label_ret;
  if (x == 1) goto label_ret;
  return 2;
label_ret:
  return 1;
}"#;

    let output = PostProcessor::cleanup_gotos(input);
    assert!(
        !output.contains("goto label_ret;"),
        "must inline terminal label references: {}",
        output
    );
    assert!(
        !output.contains("label_ret:"),
        "must remove dead terminal label: {}",
        output
    );
    assert!(
        output.matches("return 1;").count() >= 2,
        "must duplicate terminal body at call sites: {}",
        output
    );
}

#[test]
fn test_goto_cleanup_inlines_fallthrough_terminal_chain() {
    let input = r#"int test(int x)
{
  int y;
  y = 2;
  if (x == 0) goto label_zero;
  return y;
label_zero:
  y = 0;
label_ret:
  return y;
}"#;

    let output = PostProcessor::cleanup_gotos(input);
    assert!(
        !output.contains("goto label_zero;"),
        "must inline fallthrough chain entry: {}",
        output
    );
    assert!(
        !output.contains("label_zero:"),
        "must remove dead prefix label: {}",
        output
    );
    assert!(
        output.contains("y = 0;"),
        "must preserve setup statement before terminal tail: {}",
        output
    );
    assert!(
        output.contains("return y;"),
        "must preserve terminal tail after inline: {}",
        output
    );
}

#[test]
fn test_goto_cleanup_merges_adjacent_if_gotos_to_same_target() {
    let input = r#"int test(int a, int b)
{
  if (a < 0) goto label_bad;
  if (b < 0) goto label_bad;
  return 0;
label_bad:
  return 1;
}"#;

    let output = PostProcessor::cleanup_gotos(input);
    assert!(
        output.contains("if (((a < 0) || (b < 0)))") || output.contains("if ((a < 0) || (b < 0))"),
        "must merge adjacent guards with same target: {}",
        output
    );
    assert!(
        !output.contains("if (b < 0) goto label_bad;"),
        "must collapse second guard: {}",
        output
    );
}

#[test]
fn test_goto_cleanup_sinks_nonterminal_label_prefix_for_goto() {
    let input = r#"int test(int x)
{
  goto label_prefix;
label_prefix:
  y = 1;
label_body:
  if (x > 0) {
    return y;
  }
  return 0;
}"#;

    let output = PostProcessor::cleanup_gotos(input);
    assert!(
        !output.contains("goto label_prefix;"),
        "must retarget prefix goto: {}",
        output
    );
    assert!(
        !output.contains("label_prefix:"),
        "must remove dead prefix label: {}",
        output
    );
    assert!(
        output.contains("y = 1;"),
        "must preserve sunk prefix statement: {}",
        output
    );
    assert!(
        output.contains("return y;"),
        "must preserve original body semantics after sinking: {}",
        output
    );
}

#[test]
fn test_goto_cleanup_sinks_nonterminal_label_prefix_for_if_goto() {
    let input = r#"int test(int x)
{
  if (x == 0) goto label_prefix;
  return 0;
label_prefix:
  y = 1;
label_body:
  if (x > 0) {
    return y;
  }
  return 2;
}"#;

    let output = PostProcessor::cleanup_gotos(input);
    assert!(
        !output.contains("goto label_prefix;"),
        "must eliminate old prefix target: {}",
        output
    );
    assert!(
        !output.contains("label_prefix:"),
        "must remove dead prefix label: {}",
        output
    );
    assert!(
        output.contains("y = 1;"),
        "must preserve prefix statement inside guard: {}",
        output
    );
    assert!(
        output.contains("return y;"),
        "must preserve body reached through sunk prefix: {}",
        output
    );
    assert!(
        output.contains("return 2;"),
        "must preserve non-prefix path: {}",
        output
    );
}

#[test]
fn test_goto_loop_to_do_while() {
    let input = r#"int test(int n)
{
  int i;
  i = 0;
loop_1:
  sum = sum + i;
  i++;
  if (i < n) goto loop_1;
  return sum;
}"#;

    let output = PostProcessor::goto_loop_to_do_while(input);
    assert!(
        output.contains("do {"),
        "must form do-while body: {}",
        output
    );
    assert!(
        output.contains("} while (i < n);"),
        "must form do-while tail condition: {}",
        output
    );
    assert!(
        !output.contains("loop_1:"),
        "must remove loop label: {}",
        output
    );
    assert!(
        !output.contains("goto loop_1;"),
        "must remove back-edge goto: {}",
        output
    );
}

#[test]
fn test_goto_loop_then_do_while_to_for() {
    let input = r#"int test(int n)
{
  int i;
  i = 0;
loop_1:
  sum = sum + i;
  i++;
  if (i < n) goto loop_1;
  return sum;
}"#;

    let output = PostProcessor::do_while_to_for(&PostProcessor::goto_loop_to_do_while(input));
    assert!(
        output.contains("for (i = 0; i < n; i++) {"),
        "must promote goto loop through do-while into for-loop: {}",
        output
    );
    assert!(
        output.contains("sum = sum + i;"),
        "must preserve loop body: {}",
        output
    );
}

#[test]
fn test_promote_rect_param_for_get_client_rect() {
    let input = r#"ulonglong FUN_0x140006260(longlong param_1,uint8_t (*param_2) [16])
{
  if (flag) {
    iVar1 = GetClientRect(xVar2,param_2);
  }
  else {
    *param_2 = CONCAT016(0,local_3c);
  }
}"#;

    let output = PostProcessor::promote_rect_params(input);
    assert!(
        output.contains("LPRECT param_2"),
        "must promote param declaration: {}",
        output
    );
    assert!(
        output.contains("GetClientRect(xVar2,param_2)"),
        "must preserve API call: {}",
        output
    );
    assert!(
        output.contains("*(uint8_t (*)[16])param_2 = CONCAT016(0,local_3c);"),
        "must preserve whole-object write via cast: {}",
        output
    );
}

#[test]
fn test_promote_struct_param_for_wsa_startup() {
    let input = r#"int FUN_0x180001000(uint8_t (*param_1) [408])
{
  if (flag) {
    return WSAStartup(0x202,param_1);
  }
  *param_1 = CONCAT0408(0,local_198);
  return 0;
}"#;

    let output = PostProcessor::promote_rect_params(input);
    assert!(
        output.contains("LPWSADATA param_1"),
        "must promote param declaration from signature DB: {}",
        output
    );
    assert!(
        output.contains("WSAStartup(0x202,param_1)"),
        "must preserve dynamic API trigger: {}",
        output
    );
    assert!(
        output.contains("*(uint8_t (*)[408])param_1 = CONCAT0408(0,local_198);"),
        "must preserve whole-object write via sized cast: {}",
        output
    );
}

#[test]
fn test_promote_struct_param_for_get_message_w() {
    let input = r#"BOOL FUN_0x180001100(uint8_t (*param_1) [48])
{
  if (flag) {
    return GetMessageW(param_1,(HWND)0x0,0,0);
  }
  *param_1 = CONCAT048(0,local_30);
  return 0;
}"#;

    let output = PostProcessor::promote_rect_params(input);
    assert!(
        output.contains("LPMSG param_1"),
        "must promote MSG pointer params from user32 signature DB: {}",
        output
    );
    assert!(
        output.contains("GetMessageW(param_1,(HWND)0x0,0,0)"),
        "must preserve GetMessageW trigger: {}",
        output
    );
    assert!(
        output.contains("*(uint8_t (*)[48])param_1 = CONCAT048(0,local_30);"),
        "must preserve sized cast for MSG writes: {}",
        output
    );
}

#[test]
fn test_clean_slate_rect_whole_object_write() {
    let input = r#"ulonglong FUN_0x140006260(longlong param_1,LPRECT param_2)
{
  uint8_t local_3c [16];
  if (flag) {
    iVar1 = GetClientRect(xVar2,param_2);
    uVar3 = CONCAT71(Var4,iVar1 != 0);
  }
  else {
    *(uint8_t (*)[16])param_2 = CONCAT016(0,local_3c);
    uVar3 = CONCAT71(Var4,1);
  }
  return (uVar3 % 4294967296);
}"#;

    let output = PostProcessor::clean_ghidra_artifacts(input);
    assert!(
        output.contains("RECT local_3c;"),
        "must promote 16-byte temp to RECT: {}",
        output
    );
    assert!(
        output.contains("*param_2 = local_3c;"),
        "must collapse whole-object write to struct assignment: {}",
        output
    );
    assert!(
        output.contains("uVar3 = (ulonglong)(iVar1 != 0);"),
        "must simplify CONCAT71 scalar assignment: {}",
        output
    );
    assert!(
        output.contains("uVar3 = (ulonglong)(1);"),
        "must simplify branch constant assignment: {}",
        output
    );
    assert!(
        output.contains("return uVar3;"),
        "must remove redundant 32-bit truncation on promoted scalar: {}",
        output
    );
}

#[test]
fn test_clean_slate_wsadata_whole_object_write() {
    let input = r#"int FUN_0x180001000(LPWSADATA param_1)
{
  uint8_t local_198 [408];
  *(uint8_t (*)[408])param_1 = CONCAT0408(0,local_198);
  return 0;
}"#;

    let output = PostProcessor::clean_ghidra_artifacts(input);
    assert!(
        output.contains("WSADATA local_198;"),
        "must promote sized temp to target struct: {}",
        output
    );
    assert!(
        output.contains("*param_1 = local_198;"),
        "must collapse whole-object write for non-RECT structs too: {}",
        output
    );
}

#[test]
fn test_clean_slate_real_world_rect_output() {
    let input = r#"ulonglong FUN_0x140006260(longlong param_1,LPRECT param_2)
{
  int iVar1;
  uint64_t xVar2;
  uint64_t unaff_RBX;
  undefined7 Var4;
  ulonglong uVar3;
  uint8_t local_68 [40];
  uint32_t local_40;
  RECT local_3c;
  ulonglong local_18;
  
  local_18 = _DAT_140132040 ^ (ulonglong)local_68;
  Var4 = (undefined7)((ulonglong)unaff_RBX >> 8);
  if ((DAT_140132f78 == (code *)0x0) || (DAT_140132f70 == (code *)0x0)) {
    xVar2 = GetDesktopWindow();
    iVar1 = GetClientRect(xVar2,param_2);
    uVar3 = CONCAT71(Var4,iVar1 != 0);
  }
  else {
    xVar2 = (*DAT_140132f70)(param_1->field_28,2);
    local_40 = 0x28;
    (*DAT_140132f78)(xVar2,&local_40);
    *param_2 = local_3c;
    uVar3 = CONCAT71(Var4,1);
  }
  FUN_0x1400bdff0(local_18 ^ (ulonglong)local_68);
  return (uVar3 % 4294967296);
}"#;

    let output = PostProcessor::clean_ghidra_artifacts(input);
    assert!(
        !output.contains("CONCAT71"),
        "must eliminate CONCAT71 in real-world shape: {}",
        output
    );
    assert!(
        output.contains("uVar3 = (ulonglong)(iVar1 != 0);"),
        "must simplify conditional branch assignment: {}",
        output
    );
    assert!(
        output.contains("return uVar3;"),
        "must remove redundant masked return after CONCAT cleanup: {}",
        output
    );
}

#[test]
fn test_aggregate_sweep_rewrites_local_whole_object_concat_copy() {
    let input = r#"void test(uint64_t *param_2)
{
  uint8_t local_80 [16];
  local_80 = CONCAT016(0,*(uint8_t (*) [16])(param_2 + 4));
}"#;

    let output = PostProcessor::normalize_aggregate_copies(input);
    assert!(
        output.contains("typedef struct { uint8_t bytes[16]; } fission_agg16;"),
        "must inject aggregate typedef once when rewriting wide copy: {}",
        output
    );
    assert!(
        output.contains("*(fission_agg16 *)&local_80 = *(fission_agg16 *)(param_2 + 4);"),
        "must rewrite CONCAT whole-object copy into aggregate assignment: {}",
        output
    );
    assert!(
        !output.contains("local_80 = CONCAT016"),
        "must remove raw CONCAT whole-object residue: {}",
        output
    );
}

#[test]
fn test_aggregate_sweep_rewrites_pointer_destination_whole_object_copy() {
    let input = r#"void test(uint8_t *param_1,uint8_t (*param_2) [16])
{
  *(uint8_t (*) [16])(param_1 + 8) = CONCAT016(0,*param_2);
}"#;

    let output = PostProcessor::normalize_aggregate_copies(input);
    assert!(
        output.contains("*(fission_agg16 *)(param_1 + 8) = *(fission_agg16 *)param_2;"),
        "must rewrite pointer-destination whole-object copy into aggregate-typed assignment: {}",
        output
    );
    assert!(
        !output.contains("CONCAT016(0,*param_2)"),
        "must eliminate source CONCAT residue: {}",
        output
    );
}

#[test]
fn test_aggregate_sweep_rewrites_zero_initialized_whole_object_copy() {
    let input = r#"void test(void)
{
  uint8_t local_678 [16];
  local_678 = CONCAT016(0,ZEXT816(0));
}"#;

    let output = PostProcessor::normalize_aggregate_copies(input);
    assert!(
        output.contains("*(fission_agg16 *)&local_678 = (fission_agg16){0};"),
        "must rewrite zero-initialized whole-object CONCAT into aggregate literal: {}",
        output
    );
    assert!(
        !output.contains("CONCAT016(0,ZEXT816(0))"),
        "must eliminate zero aggregate CONCAT residue: {}",
        output
    );
}

#[test]
fn test_aggregate_sweep_removes_noop_self_copy() {
    let input = r#"void test(void)
{
  typedef struct { uint8_t bytes[16]; } fission_agg16;
  *(fission_agg16 *)&local_ef8 = *(fission_agg16 *)&local_ef8;
  return;
}"#;

    let output = PostProcessor::normalize_aggregate_copies(input);
    assert!(
        !output.contains("*(fission_agg16 *)&local_ef8 = *(fission_agg16 *)&local_ef8;"),
        "must remove no-op aggregate self-copy residue: {}",
        output
    );
    assert!(
        output.contains("return;"),
        "must preserve surrounding code: {}",
        output
    );
}

#[test]
fn test_var_sweep_inlines_single_use_temp_into_call() {
    let input = r#"void test(void)
{
  uVar55 = uVar14;
  foo(uVar55);
}"#;

    let output = PostProcessor::inline_single_use_temps(input);
    assert!(
        !output.contains("uVar55 = uVar14;"),
        "must remove trivial temp assignment: {}",
        output
    );
    assert!(
        output.contains("foo(uVar14);"),
        "must inline temp into immediate call site: {}",
        output
    );
}

#[test]
fn test_var_sweep_inlines_single_use_temp_into_return() {
    let input = r#"ulonglong test(int iVar1)
{
  uVar3 = (ulonglong)(iVar1 != 0);
  return uVar3;
}"#;

    let output = PostProcessor::inline_single_use_temps(input);
    assert!(
        !output.contains("uVar3 ="),
        "must remove single-use temp before return: {}",
        output
    );
    assert!(
        output.contains("return (ulonglong)(iVar1 != 0);"),
        "must inline cast expression into return: {}",
        output
    );
}

#[test]
fn test_field_offset_replacement_preserves_metadata_precedence() {
    let metadata = InferredTypeInfo {
        name: "MetaType".to_string(),
        mangled_name: String::new(),
        kind: "struct".to_string(),
        fields: vec![InferredFieldInfo {
            name: "meta_field".to_string(),
            type_name: "int".to_string(),
            offset: 0x18,
            size: 4,
        }],
        size: 0,
        metadata_address: 0x1000,
    };
    let loader = InferredTypeInfo {
        name: "LoaderType".to_string(),
        mangled_name: String::new(),
        kind: "struct".to_string(),
        fields: vec![InferredFieldInfo {
            name: "loader_field".to_string(),
            type_name: "int".to_string(),
            offset: 0x18,
            size: 4,
        }],
        size: 0,
        metadata_address: 0,
    };
    let input = "value = *(param_1 + 0x18);";
    let output = PostProcessor::new()
        .with_inferred_types(vec![metadata, loader])
        .replace_field_offsets(input);
    assert!(
        output.contains("param_1->meta_field/* @0x18 */"),
        "must keep metadata field name precedence: {}",
        output
    );
    assert!(
        !output.contains("loader_field"),
        "must not let loader field override metadata field: {}",
        output
    );
}

#[test]
fn test_field_offset_replacement_normalizes_pointer_offset_aliases() {
    let inferred = InferredTypeInfo {
        name: "Session".to_string(),
        mangled_name: String::new(),
        kind: "struct".to_string(),
        fields: vec![InferredFieldInfo {
            name: "cfg".to_string(),
            type_name: "int".to_string(),
            offset: 0xbc8,
            size: 8,
        }],
        size: 0,
        metadata_address: 0x2000,
    };
    let input = r#"unique0x00004880 = register0x00000008 + 0xbc8;
value = *unique0x00004880;"#;
    let output = PostProcessor::new()
        .with_inferred_types(vec![inferred])
        .replace_field_offsets(input);
    assert!(
        output.contains("value = register0x00000008->cfg/* @0xbc8 */;"),
        "must rewrite temp pointer alias into field access: {}",
        output
    );
    assert!(
        !output.contains("unique0x00004880 ="),
        "must remove consumed pointer-offset alias assignment: {}",
        output
    );
}

#[test]
fn test_process_normalizes_pointer_offset_aliases_without_field_info() {
    let inferred = InferredTypeInfo {
        name: "Dummy".to_string(),
        mangled_name: String::new(),
        kind: "struct".to_string(),
        fields: vec![],
        size: 0,
        metadata_address: 0,
    };
    let input = r#"unique0x00004880 = register0x00000008 + 0xbc8;
value = *unique0x00004880;"#;
    let output = PostProcessor::new()
        .with_inferred_types(vec![inferred])
        .process(input);
    assert!(
        output.contains("value = register0x00000008[0xbc8];"),
        "must still normalize pointer-offset aliases even without field metadata: {}",
        output
    );
    assert!(
        !output.contains("unique0x00004880 ="),
        "must remove consumed alias assignment in full process path: {}",
        output
    );
}

#[test]
fn test_process_normalizes_pointer_offset_aliases_in_non_deref_uses() {
    let inferred = InferredTypeInfo {
        name: "Dummy".to_string(),
        mangled_name: String::new(),
        kind: "struct".to_string(),
        fields: vec![],
        size: 0,
        metadata_address: 0,
    };
    let input = r#"unique0x00004880 = register0x00000008 + 0xbc8;
value = foo(unique0x00004880);"#;
    let output = PostProcessor::new()
        .with_inferred_types(vec![inferred])
        .process(input);
    assert!(
        output.contains("value = foo((register0x00000008 + 0xbc8));"),
        "must normalize pointer-offset aliases in non-deref uses: {}",
        output
    );
    assert!(
        !output.contains("unique0x00004880 ="),
        "must remove consumed alias assignment for non-deref uses: {}",
        output
    );
}

#[test]
fn test_field_offset_replacement_rewrites_decimal_array_index_for_x86_surface() {
    let inferred = InferredTypeInfo {
        name: "WinMergeState".to_string(),
        mangled_name: String::new(),
        kind: "struct".to_string(),
        fields: vec![InferredFieldInfo {
            name: "flags".to_string(),
            type_name: "uint".to_string(),
            offset: 24,
            size: 4,
        }],
        size: 0,
        metadata_address: 0x3000,
    };
    let input = "value = register0x00000014[24];";
    let output = PostProcessor::new()
        .with_inferred_types(vec![inferred])
        .replace_field_offsets(input);
    assert!(
        output.contains("value = register0x00000014->flags/* @24 */;"),
        "must rewrite decimal x86-style register index into field access: {}",
        output
    );
}

#[test]
fn test_var_sweep_inlines_named_result_into_return() {
    let input = r#"ulonglong test(int iVar1)
{
  result = (ulonglong)(iVar1 != 0);
  return result;
}"#;

    let output = PostProcessor::inline_single_use_temps(input);
    assert!(
        !output.contains("result ="),
        "must remove redundant named result temporary before return: {}",
        output
    );
    assert!(
        output.contains("return (ulonglong)(iVar1 != 0);"),
        "must inline named result temporary into return: {}",
        output
    );
}

#[test]
fn test_var_sweep_inlines_two_step_temp_chain_into_return() {
    let input = r#"ulonglong test(int iVar1)
{
  uVar3 = (ulonglong)(iVar1 != 0);
  result = uVar3;
  return result;
}"#;

    let output = PostProcessor::inline_single_use_temps(input);
    assert!(
        !output.contains("uVar3 ="),
        "must eliminate first temp in forwarding chain: {}",
        output
    );
    assert!(
        !output.contains("result ="),
        "must eliminate second temp in forwarding chain: {}",
        output
    );
    assert!(
        output.contains("return (ulonglong)(iVar1 != 0);"),
        "must inline forwarding chain into final return: {}",
        output
    );
}

#[test]
fn test_var_sweep_keeps_temp_when_use_is_not_immediate() {
    let input = r#"int test(int x)
{
  iVar18 = x + 1;
  side_effect();
  return iVar18;
}"#;

    let output = PostProcessor::inline_single_use_temps(input);
    assert!(
        output.contains("iVar18 = x + 1;"),
        "must keep temp when an intervening side-effect blocks safe inlining: {}",
        output
    );
    assert!(
        output.contains("return iVar18;"),
        "must preserve original use when not inlined: {}",
        output
    );
}

#[test]
fn test_var_sweep_inlines_temp_across_declaration_gap() {
    let input = r#"int test(int x)
{
  iVar18 = x + 1;
  int local_4;
  local_4 = iVar18;
  return local_4;
}"#;

    let output = PostProcessor::inline_single_use_temps(input);
    assert!(
        !output.contains("iVar18 = x + 1;"),
        "must inline temp across declaration-only gap: {}",
        output
    );
    assert!(
        output.contains("local_4 = (x + 1);") || output.contains("local_4 = x + 1;"),
        "must preserve expression at final use site: {}",
        output
    );
}

#[test]
fn test_stack_normalization_renames_exposed_stack_locals() {
    let input = r#"int test(void)
{
  uint uStack_48;
  uint _axStack_938;
  uint local_48;
  uStack_48 = 1;
  _axStack_938 = 2;
  local_48 = uStack_48;
  return local_48 + _axStack_938;
}"#;

    let output = PostProcessor::normalize_stack_artifacts(input);
    assert!(
        !output.contains("uStack_48"),
        "must rename exposed stack variable: {}",
        output
    );
    assert!(
        !output.contains("_axStack_938"),
        "must rename underscore-prefixed stack variable: {}",
        output
    );
    assert!(
        output.contains("local_48_2"),
        "must avoid colliding with pre-existing local_48: {}",
        output
    );
    assert!(
        output.contains("local_938"),
        "must normalize underscore-prefixed stack name: {}",
        output
    );
}

#[test]
fn test_stack_normalization_rewrites_piece_access() {
    let input = r#"int test(void)
{
  if ((int)axStack_848._12_4_ < 10) {
    return 1;
  }
  return 0;
}"#;

    let output =
        PostProcessor::normalize_piece_accesses(&PostProcessor::normalize_stack_artifacts(input));
    assert!(
        output.contains("((uint32_t *)&local_848)[3]"),
        "must rewrite stack piece access into typed local index form: {}",
        output
    );
    assert!(
        !output.contains("axStack_848._12_4_"),
        "must remove raw sub-variable syntax: {}",
        output
    );
}

#[test]
fn test_piece_sweep_rewrites_global_scalar_piece_access() {
    let input = r#"int test(void)
{
  if (DAT_140132020._0_1_ == 0) {
    DAT_140132020._0_1_ = 1;
  }
  return DAT_140132020._0_1_;
}"#;

    let output = PostProcessor::normalize_piece_accesses(input);
    assert!(
        output.contains("*(uint8_t *)&DAT_140132020"),
        "must rewrite global 1-byte piece access into explicit pointer cast: {}",
        output
    );
    assert!(
        !output.contains("DAT_140132020._0_1_"),
        "must remove raw global piece syntax: {}",
        output
    );
}

#[test]
fn test_piece_sweep_rewrites_wide_local_piece_access() {
    let input = r#"void test(void)
{
  axVar10 = local_848._1_15_;
  local_838._0_12_ = ZEXT812(0);
}"#;

    let output = PostProcessor::normalize_piece_accesses(input);
    assert!(
        output.contains("*(uint8_t (*)[15])((uint8_t *)&local_848 + 1)"),
        "must rewrite 15-byte piece access using array-pointer cast: {}",
        output
    );
    assert!(
        output.contains("*(uint8_t (*)[12])&local_838"),
        "must rewrite 12-byte whole-piece access using array-pointer cast: {}",
        output
    );
    assert!(
        !output.contains("local_848._1_15_"),
        "must remove raw wide piece syntax: {}",
        output
    );
    assert!(
        !output.contains("local_838._0_12_"),
        "must remove raw whole-piece syntax: {}",
        output
    );
}

#[test]
fn test_piece_sweep_uses_short_zero_offset_wide_cast() {
    let input = r#"void test(void)
{
  local_838._0_12_ = other;
}"#;

    let output = PostProcessor::normalize_piece_accesses(input);
    assert!(
        output.contains("*(uint8_t (*)[12])&local_838"),
        "must shorten zero-offset wide cast to direct address-of form: {}",
        output
    );
    assert!(
        !output.contains("*(uint8_t (*)[12])(uint8_t *)&local_838"),
        "must avoid redundant byte-pointer cast at offset zero: {}",
        output
    );
}

#[test]
fn test_piece_sweep_rewrites_explicit_byte_pointer_access_to_typed_index() {
    let input = r#"void test(void)
{
  *(uint64_t *)((uint8_t *)&local_ed0 + 8) = value;
  x = *(uint16_t *)((uint8_t *)&local_b18 + 8);
}"#;

    let output = PostProcessor::normalize_piece_accesses(input);
    assert!(
        output.contains("((uint64_t *)&local_ed0)[1] = value;"),
        "must rewrite local byte-pointer store into typed index form: {}",
        output
    );
    assert!(
        output.contains("x = ((uint16_t *)&local_b18)[4];"),
        "must rewrite local byte-pointer load into typed index form: {}",
        output
    );
    assert!(
        !output.contains("((uint8_t *)&local_ed0 + 8)"),
        "must remove redundant byte-pointer arithmetic for typed local access: {}",
        output
    );
}
