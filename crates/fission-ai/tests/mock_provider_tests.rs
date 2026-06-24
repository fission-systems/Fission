use fission_ai::provider::PseudocodeAnalyzer;
use fission_ai::provider::mock::{MockProvider, generate_mock_report};

#[test]
fn test_mock_report_empty_string() {
    let code = "";
    let report = generate_mock_report(code);
    assert!(report.contains("Target Function**: unknown_function"));
    // Since variables is empty, it should default to param_1 and local_1
    assert!(report.contains("`param_1`"));
    assert!(report.contains("`local_1`"));
}

#[test]
fn test_mock_report_malformed_code() {
    let code = "no_parentheses_or_functions_here";
    let report = generate_mock_report(code);
    assert!(report.contains("Target Function**: unknown_function"));
    assert!(report.contains("`param_1`"));
    assert!(report.contains("`local_1`"));
}

#[test]
fn test_mock_report_entirely_comments() {
    let code = "// void commented_func() {\n/* void block_commented() {\n* asterisks \n*/";
    let report = generate_mock_report(code);
    assert!(report.contains("Target Function**: unknown_function"));
    assert!(report.contains("`param_1`"));
    assert!(report.contains("`local_1`"));
}

#[test]
fn test_mock_report_multiline_block_comment_flaw() {
    // This tests the limitation that extract_function_name does not track multiline comment state.
    let code = "/*\nvoid fake_func() {\n}\n*/\nvoid real_func() {}";
    let report = generate_mock_report(code);
    // Because the line with fake_func() does not start with /*, //, or *,
    // it will be extracted even though it's inside a block comment.
    assert!(report.contains("Target Function**: fake_func"));
}

#[test]
fn test_mock_report_nested_and_function_pointers() {
    // Nested functions/complex lines
    let code = "void outer(int a, void (*inner)(int)) { }";
    let report = generate_mock_report(code);
    assert!(report.contains("Target Function**: outer"));

    // Function pointer declaration line
    let code = "void (*foo)(int) = NULL;";
    let report = generate_mock_report(code);
    // Note: Due to the simple parser design, it extracts "void" as the target function name
    // because the first open parenthesis is after (*foo), making before_paren equal to "void (".
    assert!(report.contains("Target Function**: void"));
}

#[test]
fn test_mock_report_assignment_with_parenthesis() {
    // An assignment line with parenthesis
    let code = "x = (y + z);";
    let report = generate_mock_report(code);
    // The parser incorrectly extracts "x" as the function name
    assert!(report.contains("Target Function**: x"));
}

#[test]
fn test_mock_report_unicode_and_emojis() {
    // UTF-8 strings with emojis and multi-byte characters
    let code = "void 🕵️‍♂️_function(int param_🕵️) { /* ❄️ */ }";
    let report = generate_mock_report(code);
    // Emojis are not alphanumeric or '_', so they act as delimiters/word boundaries.
    // The tokenizer splits "void 🕵️‍♂️_function" and extracts "_function" as the function name.
    assert!(report.contains("Target Function**: _function"));
    // Similarly, "param_🕵️" is split into "param_" and the emoji, extracting "param_".
    assert!(report.contains("`param_`"));
}

#[test]
fn test_mock_report_multiple_functions() {
    let code = "void first_func() {}\nvoid second_func() {}";
    let report = generate_mock_report(code);
    assert!(report.contains("Target Function**: first_func"));
}

#[test]
fn test_mock_report_variables_extraction() {
    let code = "void my_func(int param_0, int local_2, int uVar3, int iVar4) { }";
    let report = generate_mock_report(code);
    assert!(report.contains("Target Function**: my_func"));
    assert!(report.contains("`iVar4`"));
    assert!(report.contains("`local_2`"));
    assert!(report.contains("`param_0`"));
    assert!(report.contains("`uVar3`"));
}

#[test]
fn test_mock_report_control_flow_keywords() {
    let code = "if (x == y) {\n  while (true) {\n    for (int i=0; i<10; i++) {\n      switch (val) {\n        return (0);\n      }\n    }\n  }\n}\nvoid actual_func() {}";
    let report = generate_mock_report(code);
    assert!(report.contains("Target Function**: actual_func"));
}

#[tokio::test]
async fn test_analyzer_interface_with_edge_cases() {
    let provider = MockProvider::new("test-model".to_string());

    // Test empty string
    let result = provider.analyze_pseudocode("").await.unwrap();
    assert!(result.contains("Target Function**: unknown_function"));

    // Test normal function via trait
    let result = provider
        .analyze_pseudocode("void hello_world() {}")
        .await
        .unwrap();
    assert!(result.contains("Target Function**: hello_world"));
}
