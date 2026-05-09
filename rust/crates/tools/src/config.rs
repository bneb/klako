use serde::Deserialize;
use serde_json::Value;
use schemars::JsonSchema;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConfigInput {
    pub operation: String,
    pub key: Option<String>,
    pub value: Option<Value>,
}

pub fn execute_config(i: ConfigInput) -> Result<Value, String> {
    Ok(serde_json::json!({ "status": format!("Config operation {} complete", i.operation) }))
}
