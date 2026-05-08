use serde_json::json;
use tools::execute_tool;

#[test]
fn discovery_world_extracts_symbol_map() {
    let payload = json!({
        "operation": "get_repo_map",
        "dir_path": "/Users/kevin/projects/klako/rust/crates/tools/src/worlds"
    });
    
    let res = execute_tool("DiscoveryWorld", &payload).expect("discovery should succeed");
    let out: serde_json::Value = serde_json::from_str(&res).unwrap();
    
    // Assert that we see our files in the map
    let map = out.get("map").expect("should have map");
    assert!(map.get("core.rs").is_some());
    assert!(map.get("parity.rs").is_some());
    
    // Assert symbol extraction
    let core_symbols = &map["core.rs"]["symbols"];
    assert!(core_symbols.as_array().unwrap().iter().any(|s| s["name"] == "execute_memory_world"));

    // Assert dependency extraction
    let discovery_deps = &map["discovery.rs"]["dependencies"];
    assert!(discovery_deps.as_array().unwrap().iter().any(|d| d.as_str().unwrap() == "serde"));
    assert!(discovery_deps.as_array().unwrap().iter().any(|d| d.as_str().unwrap() == "walkdir"));
}
