use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum NotebookEvent {
    PlanDelta { payload: String },
    #[serde(untagged)]
    RawOther(serde_json::Value),
}

fn main() {
    let msg = r#"{"type": "StatusUpdate", "role": "thinker"}"#;
    let res = serde_json::from_str::<NotebookEvent>(msg);
    println!("{:?}", res);
    
    // Also test serialization back out
    if let Ok(event) = res {
        println!("Serialized: {:?}", serde_json::to_string(&event).unwrap());
    }
}