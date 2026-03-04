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
