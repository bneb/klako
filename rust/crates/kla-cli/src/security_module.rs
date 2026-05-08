use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum NotebookEvent {
    PlanDelta { payload: String },
    #[serde(untagged)]
    RawOther(serde_json::Value),
}

#[derive(Debug, PartialEq)]
pub enum SecurityError {
    PayloadTooLarge,
    XssDetected,
}

pub fn sanitize_and_verify(payload: String) -> Result<String, SecurityError> {
    if payload.len() > 250_000 {
        return Err(SecurityError::PayloadTooLarge);
    }
    
    let lower = payload.to_lowercase();
    if lower.contains("<script") || lower.contains("<iframe") {
        return Err(SecurityError::XssDetected);
    }
    
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_script_tag_xss() {
        let malicious_payload = "Here is my plan: <script>alert('XSS')</script>".to_string();
        let result = sanitize_and_verify(malicious_payload);
        assert_eq!(result, Err(SecurityError::XssDetected));
        
        let iframe_payload = "<iframe src='javascript:alert(1)'></iframe>".to_string();
        let result = sanitize_and_verify(iframe_payload);
        assert_eq!(result, Err(SecurityError::XssDetected));
    }

    #[test]
    fn test_valid_markdown_passes() {
        let clean_payload = "**Bold** and _italic_ and `code` with <br>".to_string();
        let result = sanitize_and_verify(clean_payload.clone());
        assert_eq!(result, Ok(clean_payload));
    }

    #[test]
    fn test_dos_payload_length_attack() {
        // Create an excessively long payload that should be rejected
        let massive_payload = "A".repeat(300_000); 
        let result = sanitize_and_verify(massive_payload);
        assert_eq!(result, Err(SecurityError::PayloadTooLarge));
    }
}
