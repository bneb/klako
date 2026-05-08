use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;
use regex::Regex;
use schemars::JsonSchema;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum DiscoveryWorldInput { 
    GetRepoMap { dir_path: String } 
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: String,
    pub line: usize,
}

pub fn execute_discovery_world(i: DiscoveryWorldInput) -> Result<Value, String> {
    match i {
        DiscoveryWorldInput::GetRepoMap { dir_path } => {
            let root = Path::new(&dir_path);
            if !root.exists() { return Err(format!("Path not found: {}", dir_path)); }
            
            let mut map = json!({});
            
            // Regexes for common symbols
            let re_rust_fn = Regex::new(r"fn\s+([a-zA-Z0-0_]+)\s*\(").unwrap();
            let re_rust_struct = Regex::new(r"struct\s+([a-zA-Z0-0_]+)").unwrap();
            let re_py_fn = Regex::new(r"def\s+([a-zA-Z0-0_]+)\s*\(").unwrap();
            let re_py_class = Regex::new(r"class\s+([a-zA-Z0-0_]+)").unwrap();
            
            // Dependency regexes
            let re_rust_use = Regex::new(r"use\s+([a-zA-Z0-0_:]+)").unwrap();
            let re_py_import = Regex::new(r"(?:import|from)\s+([a-zA-Z0-0_\.]+)").unwrap();
            let re_js_import = Regex::new(r#"import.*from\s+['"]([^'"]+)['"]"#).unwrap();

            for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    let path = entry.path();
                    let rel_path = path.strip_prefix(root).unwrap().to_string_lossy().to_string();
                    
                    if let Ok(content) = fs::read_to_string(path) {
                        let mut symbols = vec![];
                        let mut dependencies = std::collections::HashSet::new();
                        for (idx, line) in content.lines().enumerate() {
                            if let Some(cap) = re_rust_fn.captures(line) {
                                symbols.push(json!({"name": &cap[1], "kind": "function", "line": idx + 1}));
                            } else if let Some(cap) = re_rust_struct.captures(line) {
                                symbols.push(json!({"name": &cap[1], "kind": "struct", "line": idx + 1}));
                            } else if let Some(cap) = re_py_fn.captures(line) {
                                symbols.push(json!({"name": &cap[1], "kind": "function", "line": idx + 1}));
                            } else if let Some(cap) = re_py_class.captures(line) {
                                symbols.push(json!({"name": &cap[1], "kind": "class", "line": idx + 1}));
                            }
                            
                            if let Some(cap) = re_rust_use.captures(line) {
                                let module_path = cap[1].split("::").next().unwrap_or(&cap[1]).to_string();
                                if !module_path.is_empty() && module_path != "crate" && module_path != "super" {
                                     dependencies.insert(module_path);
                                }
                            } else if let Some(cap) = re_py_import.captures(line) {
                                dependencies.insert(cap[1].split('.').next().unwrap_or(&cap[1]).to_string());
                            } else if let Some(cap) = re_js_import.captures(line) {
                                dependencies.insert(cap[1].to_string());
                            }
                        }
                        
                        let deps_vec: Vec<String> = dependencies.into_iter().collect();
                        map[rel_path] = json!({
                            "symbols": symbols,
                            "dependencies": deps_vec,
                            "line_count": content.lines().count()
                        });
                    }
                }
            }
            
            Ok(json!({ "map": map }))
        }
    }
}
