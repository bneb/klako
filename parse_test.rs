fn main() {
    let s = r#"{"name": "WebSearch", "arguments": {"query": "Creating 2.5D driving video game homage to 'Cruis'n USA', Tron-inspired visuals, and Vaporwave/Waverace aesthetic"}}"#;
    let val: Result<serde_json::Value, _> = serde_json::from_str(s);
    println!("{:?}", val.is_ok());
}
