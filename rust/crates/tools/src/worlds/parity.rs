use serde::Deserialize;
use serde_json::{json, Value};
use schemars::JsonSchema;
use regex::Regex;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum ParityWorldInput { 
    AnalyzeParity { 
        reference_code: String, 
        proposed_code: String,
        language: String 
    } 
}

pub fn execute_parity_world(i: ParityWorldInput) -> Result<Value, String> {
    match i {
        ParityWorldInput::AnalyzeParity { reference_code, proposed_code, language } => {
            let ref_stats = extract_stats(&reference_code, &language);
            let prop_stats = extract_stats(&proposed_code, &language);
            
            let mut deviations = vec![];
            let mut score: f64 = 1.0;

            // 1. Indentation Mismatch
            if ref_stats.indent_size != prop_stats.indent_size {
                deviations.push(format!("Indentation mismatch: Reference is {} spaces, proposed is {} spaces.", ref_stats.indent_size, prop_stats.indent_size));
                score -= 0.3;
            }

            // 2. Naming Convention Mismatch
            // We use a ratio to detect if the "dominant" style changed
            if ref_stats.snake_case_count > ref_stats.camel_case_count && prop_stats.camel_case_count > prop_stats.snake_case_count {
                deviations.push("Naming convention mismatch: Reference is dominant snake_case, proposed is dominant camelCase.".to_string());
                score -= 0.4;
            } else if ref_stats.camel_case_count > ref_stats.snake_case_count && prop_stats.snake_case_count > prop_stats.camel_case_count {
                deviations.push("Naming convention mismatch: Reference is dominant camelCase, proposed is dominant snake_case.".to_string());
                score -= 0.4;
            }

            // 3. Brace Style (Simplified check for Rust/JS)
            if (language == "rust" || language == "javascript" || language == "typescript")
                && ref_stats.braces_on_new_line != prop_stats.braces_on_new_line {
                    deviations.push("Brace style mismatch: Reference and proposed use different newline conventions for curly braces.".to_string());
                    score -= 0.2;
                }

            Ok(json!({
                "parity_score": score.max(0.0),
                "deviations": deviations,
                "language": language
            }))
        }
    }
}

struct CodeStats {
    indent_size: usize,
    snake_case_count: usize,
    camel_case_count: usize,
    braces_on_new_line: bool,
}

fn extract_stats(code: &str, _lang: &str) -> CodeStats {
    let mut indent_sizes = vec![];
    let mut snake_case = 0;
    let mut camel_case = 0;
    let mut new_line_braces = 0;
    let mut same_line_braces = 0;

    // Indentation heuristic: collect all leading whitespace lengths
    for line in code.lines() {
        let trimmed = line.trim_start();
        if !trimmed.is_empty() && line.len() > trimmed.len() {
            indent_sizes.push(line.len() - trimmed.len());
        }
    }
    
    // Find the most frequent indentation delta (naive block size)
    let mut counts = std::collections::HashMap::new();
    for &size in &indent_sizes {
        *counts.entry(size).or_insert(0) += 1;
    }
    let indent_size = counts.into_iter().max_by_key(|&(_, count)| count).map_or(0, |(size, _)| size);

    // Naming heuristics
    let re_snake = Regex::new(r"\b[a-z][a-z0-9]*_[a-z0-9_]+\b").unwrap();
    let re_camel = Regex::new(r"\b[a-z][a-z0-9]*[A-Z][a-zA-Z0-9]+\b").unwrap();

    for line in code.lines() {
        snake_case += re_snake.find_iter(line).count();
        camel_case += re_camel.find_iter(line).count();
        
        if line.trim() == "{" {
            new_line_braces += 1;
        } else if line.trim().ends_with('{') {
            same_line_braces += 1;
        }
    }

    CodeStats {
        indent_size,
        snake_case_count: snake_case,
        camel_case_count: camel_case,
        braces_on_new_line: new_line_braces > same_line_braces,
    }
}
