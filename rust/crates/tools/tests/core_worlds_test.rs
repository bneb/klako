use serde_json::json;
use tools::execute_tool;
use std::sync::Mutex;
use std::sync::OnceLock;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn setup_temp_memory() -> (std::path::PathBuf, String) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("kla-test-memory-{}", unique));
    std::fs::create_dir_all(&path).expect("create temp dir");
    let path_str = path.to_str().expect("path to str").to_string();
    (path, path_str)
}

#[tokio::test]
async fn memory_world_persists_and_retrieves_facts() {
    let _lock = env_lock().lock().unwrap();
    let (temp_dir, temp_dir_str) = setup_temp_memory();
    std::env::set_var("KLA_MEMORY_DIR", &temp_dir_str);

    let save_payload = json!({
        "operation": "save_memory",
        "fact": "Home is at 65th and 29th AVE NE, Seattle",
        "scope": "project"
    });
    
    let save_res = execute_tool("MemoryWorld", &save_payload).await.expect("save should succeed");
    assert!(save_res.contains("Memory saved"));

    let get_payload = json!({
        "operation": "get_memory",
        "query": "Home"
    });
    
    let get_res = execute_tool("MemoryWorld", &get_payload).await.expect("get should succeed");
    assert!(get_res.contains("65th and 29th AVE NE"));

    std::env::remove_var("KLA_MEMORY_DIR");
    let _ = std::fs::remove_dir_all(temp_dir);
}

#[tokio::test]
async fn memory_world_upserts_with_keys() {
    let _lock = env_lock().lock().unwrap();
    let (temp_dir, temp_dir_str) = setup_temp_memory();
    std::env::set_var("KLA_MEMORY_DIR", &temp_dir_str);

    let key = "team_elo_arsenal";
    
    let save1 = json!({
        "operation": "save_memory",
        "fact": "2067",
        "scope": "project",
        "key": key
    });
    execute_tool("MemoryWorld", &save1).await.expect("first save should succeed");

    let save2 = json!({
        "operation": "save_memory",
        "fact": "2068",
        "scope": "project",
        "key": key
    });
    execute_tool("MemoryWorld", &save2).await.expect("second save should succeed");

    let get_payload = json!({
        "operation": "get_memory_by_key",
        "key": key
    });
    
    let get_res = execute_tool("MemoryWorld", &get_payload).await.expect("get should succeed");
    let out: serde_json::Value = serde_json::from_str(&get_res).unwrap();
    assert_eq!(out["result"]["fact"].as_str().unwrap(), "2068");

    std::env::remove_var("KLA_MEMORY_DIR");
    let _ = std::fs::remove_dir_all(temp_dir);
}
