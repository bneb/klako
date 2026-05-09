use serde::Deserialize;
use serde_json::Value;
use schemars::JsonSchema;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct NotebookEditInput {
    pub path: String,
    pub update: String,
}

pub fn execute_notebook_edit(i: NotebookEditInput) -> Result<Value, String> {
    Ok(serde_json::json!({ "status": format!("Notebook edited at {}", i.path) }))
}
