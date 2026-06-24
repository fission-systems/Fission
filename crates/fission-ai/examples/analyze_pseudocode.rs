use fission_ai::provider::PseudocodeAnalyzer;
use fission_ai::provider::mock::MockProvider;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pseudocode = r#"
// Decompiled function: process_data
void process_data(int param_1, char *param_2) {
    int local_1 = 0;
    if (param_1 > 42) {
        local_1 = param_1 * 2;
    } else {
        local_1 = param_1 + 10;
    }
    printf("Result: %d\n", local_1);
}
"#;

    println!("=== Input Pseudocode ===");
    println!("{}", pseudocode.trim());
    println!();

    // Construct a MockProvider
    let provider = MockProvider::new("mock-model".to_string());

    // Use the PseudocodeAnalyzer trait to analyze it
    println!("=== Performing Analysis ===");
    let analysis = provider.analyze_pseudocode(pseudocode).await?;

    println!("=== Resulting Analysis ===");
    println!("{}", analysis);

    Ok(())
}
