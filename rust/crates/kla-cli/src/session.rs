use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

use runtime::Session;

/// A reference to a managed session on disk: its unique ID and file path.
#[derive(Debug, Clone)]
pub struct SessionHandle {
    pub id: String,
    pub path: PathBuf,
}

/// Lightweight summary of a persisted session used for listings.
#[derive(Debug, Clone)]
pub struct ManagedSessionSummary {
    pub id: String,
    pub path: PathBuf,
    pub modified_epoch_secs: u64,
    pub message_count: usize,
}

/// Return (and create if missing) the `.claw/sessions` directory under the cwd.
pub fn sessions_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let path = cwd.join(".kla").join("sessions");
    fs::create_dir_all(&path)?;
    Ok(path)
}

/// Allocate a new unique session handle (does not write anything to disk yet).
pub fn create_managed_session_handle() -> Result<SessionHandle, Box<dyn std::error::Error>> {
    let id = generate_session_id();
    let path = sessions_dir()?.join(format!("{id}.json"));
    Ok(SessionHandle { id, path })
}

/// Generate a timestamp-based session ID, e.g. `"session-1712345678901"`.
pub fn generate_session_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("session-{millis}")
}

/// Resolve a loose session reference (path or bare ID) into a `SessionHandle`.
pub fn resolve_session_reference(
    reference: &str,
) -> Result<SessionHandle, Box<dyn std::error::Error>> {
    let direct = PathBuf::from(reference);
    let path = if direct.exists() {
        direct
    } else {
        sessions_dir()?.join(format!("{reference}.json"))
    };
    if !path.exists() {
        return Err(format!("session not found: {reference}").into());
    }
    let id = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(reference)
        .to_string();
    Ok(SessionHandle { id, path })
}

/// Return all managed sessions sorted by most-recently-modified first.
pub fn list_managed_sessions() -> Result<Vec<ManagedSessionSummary>, Box<dyn std::error::Error>> {
    let mut sessions = Vec::new();
    for entry in fs::read_dir(sessions_dir()?)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let metadata = entry.metadata()?;
        let modified_epoch_secs = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let message_count = Session::load_from_path(&path)
            .map(|session| session.messages.len())
            .unwrap_or_default();
        let id = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown")
            .to_string();
        sessions.push(ManagedSessionSummary {
            id,
            path,
            modified_epoch_secs,
            message_count,
        });
    }
    sessions.sort_by(|left, right| right.modified_epoch_secs.cmp(&left.modified_epoch_secs));
    Ok(sessions)
}

/// Format `epoch_secs` as a human-readable relative duration, e.g. `"5m ago"`.
pub fn format_relative_timestamp(epoch_secs: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(epoch_secs);
    let elapsed = now.saturating_sub(epoch_secs);
    match elapsed {
        0..=59 => format!("{elapsed}s ago"),
        60..=3_599 => format!("{}m ago", elapsed / 60),
        3_600..=86_399 => format!("{}h ago", elapsed / 3_600),
        _ => format!("{}d ago", elapsed / 86_400),
    }
}

/// Render a formatted session list, marking `active_session_id` as current.
pub fn render_session_list(
    active_session_id: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let sessions = list_managed_sessions()?;
    let mut lines = vec![
        "Sessions".to_string(),
        format!("  Directory         {}", sessions_dir()?.display()),
    ];
    if sessions.is_empty() {
        lines.push("  No managed sessions saved yet.".to_string());
        return Ok(lines.join("\n"));
    }
    for session in sessions {
        let marker = if session.id == active_session_id {
            "● current"
        } else {
            "○ saved"
        };
        lines.push(format!(
            "  {id:<20} {marker:<10} {msgs:>3} msgs · updated {modified}",
            id = session.id,
            msgs = session.message_count,
            modified = format_relative_timestamp(session.modified_epoch_secs),
        ));
        lines.push(format!("    {}", session.path.display()));
    }
    Ok(lines.join("\n"))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_session_id_has_expected_prefix() {
        let id = generate_session_id();
        assert!(
            id.starts_with("session-"),
            "expected 'session-' prefix, got: {id}"
        );
    }

    #[test]
    fn generate_session_id_is_unique_across_calls() {
        // Two rapid calls should still differ because they include millis.
        // In practice they *could* collide in sub-ms timing but this is a
        // best-effort smoke test only.
        let a = generate_session_id();
        // Sleep briefly to guarantee uniqueness.
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = generate_session_id();
        assert_ne!(a, b, "session IDs should differ across calls");
    }

    #[test]
    fn format_relative_timestamp_seconds() {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ts = now_secs.saturating_sub(30);
        let result = format_relative_timestamp(ts);
        assert!(result.ends_with("s ago"), "got: {result}");
    }

    #[test]
    fn format_relative_timestamp_minutes() {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ts = now_secs.saturating_sub(90);
        let result = format_relative_timestamp(ts);
        assert!(result.ends_with("m ago"), "got: {result}");
    }

    #[test]
    fn format_relative_timestamp_hours() {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ts = now_secs.saturating_sub(7200);
        let result = format_relative_timestamp(ts);
        assert!(result.ends_with("h ago"), "got: {result}");
    }

    #[test]
    fn format_relative_timestamp_days() {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ts = now_secs.saturating_sub(172_800);
        let result = format_relative_timestamp(ts);
        assert!(result.ends_with("d ago"), "got: {result}");
    }

    #[test]
    fn resolve_session_reference_errors_for_nonexistent_id() {
        let result = resolve_session_reference("__no_such_session_klako_test__");
        assert!(result.is_err());
    }
}
