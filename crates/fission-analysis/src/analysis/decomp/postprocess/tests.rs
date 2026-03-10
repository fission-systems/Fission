use super::PostProcessor;
use super::condition::negate_condition;

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
