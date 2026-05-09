use serde::Deserialize;
use serde_json::{json, Value};
use walkdir::WalkDir;
use std::fs;
use std::path::Path;
use regex::Regex;
use schemars::JsonSchema;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum SymbolWorldInput { 
    GetDefinitions { symbol_name: String, dir_path: String } 
}

pub fn execute_symbol_world(i: SymbolWorldInput) -> Result<Value, String> {
    match i {
        SymbolWorldInput::GetDefinitions { symbol_name, dir_path } => {
            let root = Path::new(&dir_path);
            if !root.exists() { return Err(format!("Path not found: {dir_path}")); }
            
            let mut definitions = vec![];
            
            // Refined regexes for exact symbol matching
            // Rust: fn name, struct name, enum name
            let re_rust = Regex::new(&format!(r"(?:fn|struct|enum|type)\s+{}\b", regex::escape(&symbol_name))).unwrap();
            // Python: def name, class name
            let re_py = Regex::new(&format!(r"(?:def|class)\s+{}\b", regex::escape(&symbol_name))).unwrap();
            // TS/JS: function name, class name, const name = () =>
            let re_ts = Regex::new(&format!(r"(?:function|class|const|let|var)\s+{}\b", regex::escape(&symbol_name))).unwrap();

            for entry in WalkDir::new(root).into_iter().filter_map(std::result::Result::ok) {
                if entry.file_type().is_file() {
                    let path = entry.path();
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                    
                    if let Ok(content) = fs::read_to_string(path) {
                        let re = match ext {
                            "rs" => &re_rust,
                            "py" => &re_py,
                            "ts" | "js" | "tsx" | "jsx" => &re_ts,
                            _ => continue,
                        };

                        for (idx, line) in content.lines().enumerate() {
                            if re.is_match(line) {
                                definitions.push(json!({
                                    "path": path.to_string_lossy().to_string(),
                                    "line": idx + 1,
                                    "context": line.trim()
                                }));
                            }
                        }
                    }
                }
            }
            
            Ok(json!({ "definitions": definitions, "symbol": symbol_name }))
        }
    }
}
