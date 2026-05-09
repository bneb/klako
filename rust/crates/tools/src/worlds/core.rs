use serde::Deserialize;
use serde_json::{json, Value};
use std::env;
use std::path::PathBuf;
use schemars::JsonSchema;

// --- Memory World ---
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum MemoryWorldInput { 
    SaveMemory { fact: String, scope: String, key: Option<String> },
    GetMemory { query: String },
    GetMemoryByKey { key: String }
}

pub fn execute_memory_world(i: MemoryWorldInput) -> Result<Value, String> {
    let memory_dir = env::var("KLA_MEMORY_DIR").map_or_else(|_| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(".kla"), PathBuf::from);
    let memory_path = memory_dir.join("memory.json");
    if let Some(parent) = memory_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    
    let mut memory: Vec<Value> = if memory_path.exists() {
        let content = std::fs::read_to_string(&memory_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        vec![]
    };

    match i {
        MemoryWorldInput::SaveMemory { fact, scope, key } => {
            if let Some(k) = &key {
                memory.retain(|m| m.get("key").and_then(|v| v.as_str()) != Some(k));
            }
            memory.push(json!({ 
                "fact": fact, 
                "scope": scope, 
                "key": key,
                "timestamp": chrono::Utc::now().to_rfc3339() 
            }));
            std::fs::write(&memory_path, serde_json::to_string_pretty(&memory).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            Ok(json!({ "status": "Memory saved", "key": key }))
        },
        MemoryWorldInput::GetMemory { query } => {
            let results: Vec<&Value> = memory.iter()
                .filter(|m| m["fact"].as_str().unwrap_or("").contains(&query))
                .collect();
            Ok(json!({ "results": results }))
        },
        MemoryWorldInput::GetMemoryByKey { key } => {
            let result = memory.iter()
                .find(|m| m.get("key").and_then(|v| v.as_str()) == Some(&key));
            Ok(json!({ "result": result }))
        }
    }
}

// --- Temporal World ---
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum TemporalWorldInput {
    GetCurrentTime,
    ScheduleTask { description: String, timestamp: String },
}

pub fn execute_temporal_world(i: TemporalWorldInput) -> Result<Value, String> {
    match i {
        TemporalWorldInput::GetCurrentTime => {
            Ok(json!({ "now": chrono::Utc::now().to_rfc3339() }))
        },
        TemporalWorldInput::ScheduleTask { description, timestamp } => {
            Ok(json!({ "status": "Task scheduled", "description": description, "at": timestamp }))
        }
    }
}

// --- Logistics World ---
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum LogisticsWorldInput {
    CalculateRoute { origin: String, destination: String },
}

pub fn execute_logistics_world(i: LogisticsWorldInput) -> Result<Value, String> {
    match i {
        LogisticsWorldInput::CalculateRoute { origin, destination } => {
            Ok(json!({ "origin": origin, "destination": destination, "distance_km": 42.0 }))
        }
    }
}

// --- Live World ---
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum LiveWorldInput {
    GetSystemMetrics,
}

pub fn execute_live_world(i: LiveWorldInput) -> Result<Value, String> {
    match i {
        LiveWorldInput::GetSystemMetrics => {
            Ok(json!({ "cpu_usage": 15.4, "memory_free_gb": 8.2 }))
        }
    }
}
