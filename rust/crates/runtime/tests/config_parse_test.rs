use std::path::PathBuf;
use runtime::ConfigLoader;

#[test]
fn test_load_actual_kla_json() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // crates
    path.pop(); // rust
    path.pop(); // klako
    let cwd = path.clone();
    let home = path.join(".kla");
    let loader = ConfigLoader::new(cwd, home);
    match loader.load() {
        Ok(config) => {
            println!("Successfully loaded config");
            assert!(config.agency_topology().is_some(), "Agency topology should be present");
        },
        Err(e) => panic!("Failed to load config: {}", e),
    }
}
