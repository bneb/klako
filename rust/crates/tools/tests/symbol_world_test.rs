use serde_json::json;
use tools::execute_tool;

#[tokio::test]
async fn symbol_world_locates_exact_definitions() {
    let payload = json!({
        "operation": "get_definitions",
        "symbol_name": "execute_memory_world",
        "dir_path": "/Users/kevin/projects/klako/rust/crates/tools/src"
    });
    
    let res = execute_tool("SymbolWorld", &payload).await.expect("symbol lookup should succeed");
    let out: serde_json::Value = serde_json::from_str(&res).unwrap();
    
    // Assert we found the definition in the modular core.rs file
    let defs = out.get("definitions").expect("should have definitions");
    assert!(defs.as_array().unwrap().iter().any(|d| {
        d["path"].as_str().unwrap().contains("core.rs") &&
        d["line"].as_u64().unwrap() > 0
    }));
}
