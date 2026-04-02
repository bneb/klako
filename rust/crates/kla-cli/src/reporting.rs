use std::env;
use std::path::{Path, PathBuf};
use runtime::{TokenUsage, Session, ConfigLoader, ProjectContext};
use crate::{VERSION, BUILD_TARGET, GIT_SHA, DEFAULT_DATE};

pub(crate) struct StatusContext {
    pub(crate) cwd: PathBuf,
    pub(crate) session_path: Option<PathBuf>,
    pub(crate) loaded_config_files: usize,
    pub(crate) discovered_config_files: usize,
    pub(crate) memory_file_count: usize,
    pub(crate) project_root: Option<PathBuf>,
    pub(crate) git_branch: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct StatusUsage {
    pub(crate) message_count: usize,
    pub(crate) turns: u32,
    pub(crate) latest: TokenUsage,
    pub(crate) cumulative: TokenUsage,
    pub(crate) estimated_tokens: usize,
}

pub(crate) fn format_model_report(model: &str, message_count: usize, turns: u32) -> String {
    format!(
        "Model
  Current          {model}
  Session          {message_count} messages · {turns} turns

Aliases
  pro              gemini-3.1-pro-preview
  flash            gemini-3-flash-preview
  opus             claude-opus-4-6

Next
  /model           Show the current model
  /model <name>    Switch models for this REPL session"
    )
}

pub(crate) fn format_model_switch_report(previous: &str, next: &str, message_count: usize) -> String {
    format!(
        "Model updated
  Previous         {previous}
  Current          {next}
  Preserved        {message_count} messages
  Tip              Existing conversation context stayed attached"
    )
}

pub(crate) fn format_permissions_report(mode: &str) -> String {
    let modes = [
        ("read-only", "Read/search tools only", mode == "read-only"),
        (
            "workspace-write",
            "Edit files inside the workspace",
            mode == "workspace-write",
        ),
        (
            "danger-full-access",
            "Unrestricted tool access",
            mode == "danger-full-access",
        ),
    ]
    .into_iter()
    .map(|(name, description, is_current)| {
        let marker = if is_current {
            "● current"
        } else {
            "○ available"
        };
        format!("  {name:<18} {marker:<11} {description}")
    })
    .collect::<Vec<_>>()
    .join("\n");

    let effect = match mode {
        "read-only" => "Only read/search tools can run automatically",
        "workspace-write" => "Editing tools can modify files in the workspace",
        "danger-full-access" => "All tools can run without additional sandbox limits",
        _ => "Unknown permission mode",
    };

    format!(
        "Permissions
  Active mode      {mode}
  Effect           {effect}

Modes
{modes}

Next
  /permissions              Show the current mode
  /permissions <mode>       Switch modes for subsequent tool calls"
    )
}

pub(crate) fn format_permissions_switch_report(previous: &str, next: &str) -> String {
    format!(
        "Permissions updated
  Previous mode    {previous}
  Active mode      {next}
  Applies to       Subsequent tool calls in this REPL
  Tip              Run /permissions to review all available modes"
    )
}

pub(crate) fn format_cost_report(usage: TokenUsage) -> String {
    format!(
        "Cost
  Input tokens     {}
  Output tokens    {}
  Cache create     {}
  Cache read       {}
  Total tokens     {}

Next
  /status          See session + workspace context
  /compact         Trim local history if the session is getting large",
        usage.input_tokens,
        usage.output_tokens,
        usage.cache_creation_input_tokens,
        usage.cache_read_input_tokens,
        usage.total_tokens(),
    )
}

pub(crate) fn format_resume_report(session_path: &str, message_count: usize, turns: u32) -> String {
    format!(
        "Session resumed
  Session file     {session_path}
  History          {message_count} messages · {turns} turns
  Next             /status · /diff · /export"
    )
}

pub(crate) fn format_compact_report(removed: usize, resulting_messages: usize, skipped: bool) -> String {
    if skipped {
        format!(
            "Compact
  Result           skipped
  Reason           Session is already below the compaction threshold
  Messages kept    {resulting_messages}"
        )
    } else {
        format!(
            "Compact
  Result           compacted
  Messages removed {removed}
  Messages kept    {resulting_messages}
  Tip              Use /status to review the trimmed session"
        )
    }
}

pub(crate) fn format_status_report(
    model: &str,
    usage: StatusUsage,
    permission_mode: &str,
    context: &StatusContext,
) -> String {
    let session_info = format!(
        "Session
  Model            {model}
  Permissions      {permission_mode}
  Activity         {} messages · {} turns
  Tokens           est {} · latest {} · total {}",
        usage.message_count,
        usage.turns,
        usage.estimated_tokens,
        usage.latest.total_tokens(),
        usage.cumulative.total_tokens(),
    );

    let usage_info = format!(
        "Usage
  Cumulative input {}
  Cumulative output {}
  Cache create     {}
  Cache read       {}
  Total tokens     {}",
        usage.cumulative.input_tokens,
        usage.cumulative.output_tokens,
        usage.cumulative.cache_creation_input_tokens,
        usage.cumulative.cache_read_input_tokens,
        usage.cumulative.total_tokens(),
    );

    let context_info = format!(
        "Context
  Workspace        {}
  Branch           {}
  Directory        {}
  Session file     {}
  Config files     {} loaded · {} discovered
  Memory files     {}",
        context.project_root.as_ref().map(|p: &PathBuf| p.file_name().unwrap().to_str().unwrap()).unwrap_or("none"),
        context.git_branch.as_deref().unwrap_or("none"),
        context.cwd.display(),
        context.session_path.as_ref().map(|p: &PathBuf| p.display().to_string()).unwrap_or("none".to_string()),
        context.loaded_config_files,
        context.discovered_config_files,
        context.memory_file_count,
    );

    format!("{session_info}\n\n{usage_info}\n\n{context_info}\n\nNext\n  /cost            Review token expenditure\n  /diff            Review current workspace changes\n  /export          Save session transcript to a file")
}

pub(crate) fn render_config_report(section: Option<&str>) -> Result<String, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let config = loader.load()?;
    let entries = config.loaded_entries();

    if entries.is_empty() {
        return Ok("Configuration\n  Status           no .kla.json files discovered".to_string());
    }

    let mut lines = vec![format!(
        "Configuration\n  Status           {} files loaded",
        entries.len()
    )];

    for entry in entries {
        lines.push(format!("  File             {}", entry.path.display()));
    }

    if let Some(section_name) = section {
        if let Some(value) = config.merged().get(section_name) {
            lines.push(format!("  [{section_name}]\n{}", crate::reporting::indent_block(&value.render(), 4)));
        }
    } else {
        for key in config.merged().keys() {
            lines.push(format!("    {key}"));
        }
    }

    Ok(lines.join("\n"))
}

pub(crate) fn render_memory_report() -> Result<String, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let context = runtime::ProjectContext::discover_with_git(&cwd, "2026-03-31")?;
    let files = context.instruction_files;

    if files.is_empty() {
        return Ok("Memory\n  Status           no memory or instruction files discovered".to_string());
    }

    let mut lines = vec![format!("Memory\n  Status           {} files loaded", files.len())];
    for file in files {
        lines.push(format!("  File             {}", file.path.display()));
    }
    Ok(lines.join("\n"))
}

pub(crate) fn render_diff_report() -> Result<String, Box<dyn std::error::Error>> {
    let output = crate::git::git_output(&["diff", "--stat"])?;
    if output.trim().is_empty() {
        return Ok("Diff\n  Status           no changes discovered".to_string());
    }

    let mut lines = vec!["Diff".to_string(), "  Status           uncommitted changes".to_string(), "  Summary".to_string()];
    for line in output.lines() {
        lines.push(format!("    {line}"));
    }
    Ok(lines.join("\n"))
}

pub(crate) fn render_teleport_report(target: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut lines = vec![
        "Teleport".to_string(),
        format!("  Target           {target}"),
        "  Result           looking for symbols...".to_string(),
    ];

    let output = crate::git::git_output(&["grep", "-l", target])?;
    if output.trim().is_empty() {
        lines.push("  Match            none discovered".to_string());
    } else {
        lines.push("  Match            discovered in:".to_string());
        for line in output.lines().take(5) {
            lines.push(format!("    {line}"));
        }
        let total = output.lines().count();
        if total > 5 {
            lines.push(format!("    ... and {} more", total - 5));
        }
    }

    Ok(lines.join("\n"))
}

pub(crate) fn render_last_tool_debug_report(session: &Session) -> Result<String, Box<dyn std::error::Error>> {
    let last_tool_call = session.messages.iter().rev().find_map(|msg| {
        msg.blocks.iter().find_map(|block| {
            if let runtime::ContentBlock::ToolUse { id, name, input } = block {
                Some((id, name, input))
            } else {
                None
            }
        })
    });

    let Some((id, name, input)) = last_tool_call else {
        return Ok("Debug Tool Call\n  Status           no tool calls in recent history".to_string());
    };

    Ok(format!(
        "Debug Tool Call
  Name             {name}
  ID               {id}
  Input            {}
",
        input
    ))
}

pub(crate) fn render_version_report() -> String {
    let git_sha = GIT_SHA.unwrap_or("unknown");
    let target = BUILD_TARGET.unwrap_or("unknown");
    format!("Klako CLI v{VERSION} ({git_sha}) [{target}]")
}

pub(crate) fn render_export_text(session: &Session) -> String {
    let mut transcript = Vec::new();
    for msg in &session.messages {
        let role = match msg.role {
            runtime::MessageRole::User => "USER",
            runtime::MessageRole::Assistant => "ASSISTANT",
            runtime::MessageRole::System => "SYSTEM",
            runtime::MessageRole::Tool => "TOOL",
        };
        transcript.push(format!("{role}:"));
        for block in &msg.blocks {
            match block {
                runtime::ContentBlock::Text { text } => transcript.push(text.clone()),
                runtime::ContentBlock::ToolUse { name, input, .. } => {
                    transcript.push(format!("TOOL USE: {name}({input})"));
                }
                runtime::ContentBlock::ToolResult { output, .. } => {
                    transcript.push(format!("TOOL RESULT: {output}"));
                }
            }
        }
        transcript.push(String::new());
    }
    transcript.join("\n")
}

pub(crate) fn default_export_filename(session: &Session) -> String {
    format!("kla-transcript-{}.txt", session.messages.len())
}

pub(crate) fn resolve_export_path(
    requested_path: Option<&str>,
    session: &Session,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = requested_path {
        Ok(PathBuf::from(path))
    } else {
        let mut path = env::current_dir()?;
        path.push(default_export_filename(session));
        Ok(path)
    }
}

pub(crate) fn sanitize_generated_message(message: &str) -> String {
    message
        .lines()
        .filter(|line| !line.trim().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub(crate) fn parse_titled_body(draft: &str) -> Option<(String, String)> {
    let mut lines = draft.lines();
    let title = lines
        .find(|line| line.starts_with("TITLE: "))?
        .trim_start_matches("TITLE: ")
        .to_string();
    let body = lines
        .skip_while(|line| !line.starts_with("BODY:"))
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n");
    Some((title, body))
}

pub(crate) fn status_context(
    session_path: Option<&Path>,
) -> Result<StatusContext, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let discovered_config_files = loader.discover().len();
    let runtime_config = loader.load()?;
    let project_context = ProjectContext::discover_with_git(&cwd, DEFAULT_DATE)?;
    let (project_root, git_branch) =
        crate::git::parse_git_status_metadata(project_context.git_status.as_deref());
    Ok(StatusContext {
        cwd,
        session_path: session_path.map(Path::to_path_buf),
        loaded_config_files: runtime_config.loaded_entries().len(),
        discovered_config_files,
        memory_file_count: project_context.instruction_files.len(),
        project_root,
        git_branch,
    })
}

pub(crate) fn init_klako_md() -> Result<String, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    Ok(crate::init::initialize_repo(&cwd)?.render())
}

pub(crate) fn indent_block(value: &str, spaces: usize) -> String {
    let indent = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{indent}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn render_repl_help() -> String {
    [
        "Interactive REPL".to_string(),
        "  Quick start          Ask a task in plain English or use one of the core commands below."
            .to_string(),
        "  Core commands        /help · /status · /model · /permissions · /compact".to_string(),
        "  Exit                 /exit or /quit".to_string(),
        "  Vim mode             /vim toggles modal editing".to_string(),
        "  History              Up/Down recalls previous prompts".to_string(),
        "  Completion           Tab cycles slash command matches".to_string(),
        "  Cancel               Ctrl-C clears input (or exits on an empty prompt)".to_string(),
        "  Multiline            Shift+Enter or Ctrl+J inserts a newline".to_string(),
        String::new(),
        commands::render_slash_command_help(),
    ]
    .join("\n")
}

pub(crate) fn render_unknown_repl_command(name: &str) -> String {
    let mut lines = vec![
        "Unknown slash command".to_string(),
        format!("  Command          /{name}"),
    ];
    append_repl_command_suggestions(&mut lines, name);
    lines.join("\n")
}

pub(crate) fn append_repl_command_suggestions(lines: &mut Vec<String>, name: &str) {
    let suggestions = suggest_repl_commands(name);
    if suggestions.is_empty() {
        lines.push("  Try              /help shows the full slash command map".to_string());
        return;
    }

    lines.push("  Try              /help shows the full slash command map".to_string());
    lines.push("Suggestions".to_string());
    lines.extend(
        suggestions
            .into_iter()
            .map(|suggestion| format!("  {suggestion}")),
    );
}

pub(crate) fn suggest_repl_commands(name: &str) -> Vec<String> {
    let normalized = name.trim().trim_start_matches('/').to_ascii_lowercase();
    if normalized.is_empty() {
        return Vec::new();
    }

    let mut ranked = slash_command_completion_candidates()
        .into_iter()
        .filter_map(|candidate: String| {
            let raw = candidate.trim_start_matches('/').to_ascii_lowercase();
            let distance = edit_distance(&normalized, &raw);
            let prefix_match = raw.starts_with(&normalized) || normalized.starts_with(&raw);
            let near_match = distance <= 2;
            (prefix_match || near_match).then_some((distance, candidate))
        })
        .collect::<Vec<_>>();
    ranked.sort();
    ranked.dedup_by(|left, right| left.1 == right.1);
    ranked
        .into_iter()
        .map(|(_, candidate)| candidate)
        .take(3)
        .collect()
}

pub(crate) fn edit_distance(left: &str, right: &str) -> usize {
    if left == right {
        return 0;
    }
    if left.is_empty() {
        return right.chars().count();
    }
    if right.is_empty() {
        return left.chars().count();
    }

    let right_chars = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right_chars.len()).collect::<Vec<_>>();
    let mut current = vec![0; right_chars.len() + 1];

    for (left_index, left_char) in left.chars().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right_chars.iter().enumerate() {
            let substitution_cost = usize::from(left_char != *right_char);
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(previous[right_index] + substitution_cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[right_chars.len()]
}

pub(crate) fn append_slash_command_suggestions(lines: &mut Vec<String>, name: &str) {
    let suggestions = suggest_slash_commands(name);
    if suggestions.is_empty() {
        lines.push("  Try              /help shows the full slash command map".to_string());
    } else {
        lines.push(format!("  Try              {}", suggestions.join(", ")));
        lines.push("  Help             /help shows the full slash command map".to_string());
    }
}

pub(crate) fn suggest_slash_commands(name: &str) -> Vec<String> {
    commands::suggest_slash_commands(name, 3)
        .into_iter()
        .take(3)
        .collect()
}

pub(crate) fn render_mode_unavailable(command: &str, label: &str) -> String {
    [
        "Command unavailable in this REPL mode".to_string(),
        format!("  Command          /{command}"),
        format!("  Feature          {label}"),
        "  Tip              Use /help to find currently wired REPL commands".to_string(),
    ]
    .join("\n")
}

pub(crate) fn format_direct_slash_command_error(command: &str, is_unknown: bool) -> String {
    let trimmed = command.trim().trim_start_matches('/');
    let mut lines = vec![
        "Direct slash command unavailable".to_string(),
        format!("  Command          /{trimmed}"),
    ];
    if is_unknown {
        append_slash_command_suggestions(&mut lines, trimmed);
    } else {
        lines.push("  Try              Start `kla` to use interactive slash commands".to_string());
        lines.push(
            "  Tip              Resume-safe commands also work with `kla --resume SESSION.json ...`"
                .to_string(),
        );
    }
    lines.join("\n")
}

pub(crate) fn normalize_permission_mode(value: &str) -> Option<&'static str> {
    match value.to_lowercase().as_str() {
        "read-only" | "readonly" | "read" | "r" => Some("read-only"),
        "workspace-write" | "workspace" | "write" | "w" => Some("workspace-write"),
        "danger-full-access" | "danger" | "full" | "d" => Some("danger-full-access"),
        _ => None,
    }
}

pub(crate) fn permission_mode_from_label(mode: &str) -> runtime::PermissionMode {
    match mode {
        "read-only" => runtime::PermissionMode::ReadOnly,
        "workspace-write" => runtime::PermissionMode::WorkspaceWrite,
        "danger-full-access" => runtime::PermissionMode::DangerFullAccess,
        other => panic!("unsupported permission mode label: {other}"),
    }
}

pub(crate) fn render_cli_error(problem: &str) -> String {
    let mut lines = vec!["Error".to_string()];
    for (index, line) in problem.lines().enumerate() {
        let label = if index == 0 {
            "  Problem          "
        } else {
            "                   "
        };
        lines.push(format!("{label}{line}"));
    }
    lines.push("  Help             kla --help".to_string());
    lines.join("\n")
}

pub(crate) fn slash_command_completion_candidates() -> Vec<String> {
    let mut candidates = commands::slash_command_specs()
        .iter()
        .flat_map(|spec| {
            std::iter::once(spec.name)
                .chain(spec.aliases.iter().copied())
                .map(|name| format!("/{name}"))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    candidates.extend([
        String::from("/vim"),
        String::from("/exit"),
        String::from("/quit"),
    ]);
    candidates.sort();
    candidates.dedup();
    candidates
}

pub(crate) fn format_tool_result(name: &str, output: &str, is_error: bool) -> String {
    let icon = if is_error {
        "\x1b[1;31m✗\x1b[0m"
    } else {
        "\x1b[1;32m✓\x1b[0m"
    };

    if is_error {
        let summary = if output.len() > 160 {
            format!("{}...", &output[..157])
        } else {
            output.to_string()
        };
        return if summary.is_empty() {
            format!("{icon} \x1b[38;5;245m{name}\x1b[0m")
        } else {
            format!("{icon} \x1b[38;5;245m{name}\x1b[0m\n\x1b[38;5;203m{summary}\x1b[0m")
        };
    }

    format!("{icon} \x1b[38;5;245m{name}\x1b[0m")
}
