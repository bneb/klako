use std::env;
fn main() {
    println!("Loading config...");
    let cwd = env::current_dir().unwrap();
    let loader = runtime::config::ConfigLoader::default_for(&cwd);
    let cfg = loader.load().unwrap();
    println!("topology: {:#?}", cfg.feature_config().agency_topology());
}
