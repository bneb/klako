use serde_json::json;
use tools::execute_tool;

#[tokio::test]
async fn parity_world_detects_naming_and_indentation_mismatches() {
    // Reference Python code: snake_case, 4 spaces
    let reference = r#"
def calculate_metrics(team_id):
    score = get_score(team_id)
    return score * 1.5
"#;

    // Proposed Python code: camelCase, 2 spaces
    let proposed = r#"
def calculateMetrics(teamId):
  score = getScore(teamId)
  return score * 1.5
"#;

    let payload = json!({
        "operation": "analyze_parity",
        "reference_code": reference,
        "proposed_code": proposed,
        "language": "python"
    });
    
    let res = execute_tool("ParityWorld", &payload).await.expect("parity analysis should succeed");
    let out: serde_json::Value = serde_json::from_str(&res).unwrap();
    
    // Assert low parity score
    let score = out["parity_score"].as_f64().expect("should have parity_score");
    assert!(score < 0.5, "Score {} should be low for major mismatches", score);
    
    // Assert deviations caught
    let deviations = out["deviations"].as_array().expect("should have deviations");
    let dev_str = serde_json::to_string(deviations).unwrap().to_lowercase();
    assert!(dev_str.contains("indentation"), "Should catch indentation mismatch");
    assert!(dev_str.contains("naming"), "Should catch naming convention mismatch");
}
