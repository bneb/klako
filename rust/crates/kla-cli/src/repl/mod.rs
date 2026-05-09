use std::env;
use std::io;

use crate::{
    input, session, bridge,
    AllowedToolSet, build_system_prompt,
};
use crate::reporting::{
    status_context,
    render_repl_help, render_unknown_repl_command,
    slash_command_completion_candidates,
};
use crate::run_init;

use commands::SlashCommand;
use runtime::{
    ConversationRuntime, PermissionMode, Session,
};
use crate::bridge::{DefaultRuntimeClient, CliToolExecutor};

pub mod prompter;
pub mod notebook;
pub mod cmds;

pub async fn run_repl(
    model: String,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
) -> Result<(), Box<dyn std::error::Error>> {
    run_repl_with_telemetry(model, allowed_tools, permission_mode, None).await
}

pub async fn run_repl_with_telemetry(
    model: String,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    tx: Option<tokio::sync::broadcast::Sender<String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cli = LiveCli::new(model, true, allowed_tools, permission_mode, tx).await?;
    let mut editor = input::LineEditor::new("> ", slash_command_completion_candidates());
    println!("{}", cli.startup_banner());

    loop {
        match editor.read_line()? {
            input::ReadOutcome::Submit(input) => {
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if matches!(trimmed, "/exit" | "/quit") {
                    cli.persist_session()?;
                    break;
                }
                if let Some(command) = SlashCommand::parse(trimmed) {
                    if cli.handle_repl_command(command).await? {
                        cli.persist_session()?;
                    }
                    continue;
                }
                editor.push_history(&input);
                cli.run_turn(&input).await?;
            }
            input::ReadOutcome::Cancel => {}
            input::ReadOutcome::Exit => {
                cli.persist_session()?;
                break;
            }
        }
    }

    Ok(())
}

pub struct LiveCli {
    pub(crate) model: String,
    pub(crate) allowed_tools: Option<AllowedToolSet>,
    pub(crate) permission_mode: PermissionMode,
    pub(crate) system_prompt: Vec<String>,
    pub(crate) runtime: ConversationRuntime<DefaultRuntimeClient, CliToolExecutor>,
    pub(crate) session: session::SessionHandle,
    pub(crate) tx: Option<tokio::sync::broadcast::Sender<String>>,
}

impl LiveCli {
    pub async fn new(
        model: String,
        enable_tools: bool,
        allowed_tools: Option<AllowedToolSet>,
        permission_mode: PermissionMode,
        tx: Option<tokio::sync::broadcast::Sender<String>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let cwd = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let system_prompt = build_system_prompt(&cwd, crate::DEFAULT_DATE.to_string(), &model)?;
        let session = session::create_managed_session_handle()?;
        let runtime = bridge::build_runtime(
            Session::new(),
            model.clone(),
            system_prompt.clone(),
            enable_tools,
            true,
            allowed_tools.clone(),
            permission_mode,
            None,
            tx.clone(),
        ).await?;

        // Start Proactive Context Indexer
        let index_path = cwd.join(".klako/SWARM_GRAPH.db");
        let _ = std::fs::create_dir_all(cwd.join(".klako"));
        if let Ok(index) = runtime::index::CodebaseIndex::open(&index_path) {
            let daemon = runtime::index::IndexerDaemon::new(index, cwd.clone());
            tokio::spawn(async move {
                let _ = daemon.run().await;
            });
        }

        let cli = Self {
            model,
            allowed_tools,
            permission_mode,
            system_prompt,
            runtime,
            session,
            tx,
        };
        cli.persist_session()?;
        Ok(cli)
    }

    pub fn startup_banner(&self) -> String {
        use std::io::IsTerminal;
        let color = io::stdout().is_terminal();
        let cwd = env::current_dir().ok();
        let cwd_display = cwd.as_ref().map_or_else(
            || "<unknown>".to_string(),
            |path| path.display().to_string(),
        );
        let workspace_name = cwd
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("workspace");
        let git_branch = status_context(Some(&self.session.path))
            .ok()
            .and_then(|context| context.git_branch);
        let workspace_summary = git_branch.as_deref().map_or_else(
            || workspace_name.to_string(),
            |branch| format!("{workspace_name} · {branch}"),
        );
        let has_klako_md = cwd
            .as_ref()
            .is_some_and(|path| path.join("KLA.md").is_file());
        
        let mut banner = format!(
            "{} {}\n",
            if color { "\x1b[1;38;5;45mⓀ Klako\x1b[0m" } else { "Klako" },
            if color { "\x1b[2m· ready\x1b[0m" } else { "· ready" }
        );
        banner.push_str(&format!("  Workspace        {workspace_summary}\n"));
        banner.push_str(&format!("  Directory        {cwd_display}\n"));
        banner.push_str(&format!("  Model            {}\n", self.model));
        banner.push_str(&format!("  Permissions      {}\n", self.permission_mode.as_str()));
        banner.push_str(&format!("  Session          {}\n", self.session.id));
        banner.push_str(&format!(
            "  Quick start      {}\n",
            if has_klako_md { "/help · /status · ask for a task" } else { "/init · /help · /status" }
        ));
        banner.push_str("  Editor           Tab completes slash commands · /vim toggles modal editing\n");
        banner.push_str("  Multiline        Shift+Enter or Ctrl+J inserts a newline");
        
        banner
    }

    pub async fn handle_repl_command(&mut self, command: SlashCommand) -> Result<bool, Box<dyn std::error::Error>> {
        match command {
            SlashCommand::Help => {
                println!("{}", render_repl_help());
                Ok(false)
            }
            SlashCommand::Status => {
                self.print_status();
                Ok(false)
            }
            SlashCommand::Bughunter { scope } => {
                self.run_bughunter(scope.as_deref()).await?;
                Ok(false)
            }
            SlashCommand::Commit => {
                self.run_commit().await?;
                Ok(true)
            }
            SlashCommand::Pr { context } => {
                self.run_pr(context.as_deref()).await?;
                Ok(false)
            }
            SlashCommand::Issue { context } => {
                self.run_issue(context.as_deref()).await?;
                Ok(false)
            }
            SlashCommand::Ultraplan { task } => {
                self.run_ultraplan(task.as_deref()).await?;
                Ok(false)
            }
            SlashCommand::Teleport { target } => {
                self.run_teleport(target.as_deref())?;
                Ok(false)
            }
            SlashCommand::DebugToolCall => {
                self.run_debug_tool_call()?;
                Ok(false)
            }
            SlashCommand::Compact => {
                self.compact().await?;
                Ok(false)
            }
            SlashCommand::Model { model } => self.set_model(model).await,
            SlashCommand::Permissions { mode } => self.set_permissions(mode).await,
            SlashCommand::Clear { confirm } => self.clear_session(confirm).await,
            SlashCommand::Cost => {
                self.print_cost();
                Ok(false)
            }
            SlashCommand::Settings => {
                self.run_settings().await?;
                Ok(false)
            }
            SlashCommand::Resume { session_path } => self.resume_session(session_path).await,
            SlashCommand::Config { section } => {
                Self::print_config(section.as_deref())?;
                Ok(false)
            }
            SlashCommand::Memory => {
                Self::print_memory()?;
                Ok(false)
            }
            SlashCommand::Init => {
                run_init()?;
                Ok(false)
            }
            SlashCommand::Diff => {
                Self::print_diff()?;
                Ok(false)
            }
            SlashCommand::Version => {
                Self::print_version();
                Ok(false)
            }
            SlashCommand::Export { path } => {
                self.export_session(path.as_deref())?;
                Ok(false)
            }
            SlashCommand::Session { action, target } => {
                self.handle_session_command(action.as_deref(), target.as_deref()).await
            }
            SlashCommand::Plugins { action, target } => {
                self.handle_plugins_command(action.as_deref(), target.as_deref()).await
            }
            SlashCommand::Agents { args } => {
                Self::print_agents(args.as_deref())?;
                Ok(false)
            }
            SlashCommand::Skills { args } => {
                Self::print_skills(args.as_deref())?;
                Ok(false)
            }
            SlashCommand::Loop { objective, budget } => {
                self.run_loop(objective.as_deref(), budget).await?;
                Ok(false)
            }
            SlashCommand::Retro => {
                self.run_retro().await?;
                Ok(false)
            }
            SlashCommand::Design { feature } => {
                self.run_design(feature.as_deref()).await?;
                Ok(false)
            }
            SlashCommand::Map { path } => {
                self.run_map(path.as_deref()).await?;
                Ok(false)
            }
            SlashCommand::Rewind { task_index } => {
                self.run_rewind(task_index).await?;
                Ok(false)
            }
            SlashCommand::Review { path } => {
                self.run_review(path.as_deref()).await?;
                Ok(false)
            }
            _ => {
                println!("{}", render_unknown_repl_command("unknown"));
                Ok(false)
            }
        }
    }

    pub fn persist_session(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime.session().save_to_path(&self.session.path)?;
        Ok(())
    }

    pub async fn run_turn(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut prompter = bridge::CliPermissionPrompter::new(self.permission_mode);
        self.run_turn_with_prompter(input, Some(&mut prompter)).await
    }

    pub async fn run_turn_with_prompter(
        &mut self,
        input: &str,
        prompter: Option<&mut dyn runtime::PermissionPrompter>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime.run_turn(input, prompter).await?;
        Ok(())
    }

    pub async fn run_turn_with_output(
        &mut self,
        input: &str,
        _format: crate::CliOutputFormat,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.run_turn(input).await?;
        Ok(())
    }

    pub async fn run_internal_prompt_text_with_progress(
        &self,
        prompt: &str,
        enable_tools: bool,
        progress: Option<bridge::InternalPromptProgressReporter>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let session = self.runtime.session().clone();
        let mut runtime = bridge::build_runtime(
            session,
            self.model.clone(),
            self.system_prompt.clone(),
            enable_tools,
            false,
            self.allowed_tools.clone(),
            self.permission_mode,
            progress,
            self.tx.clone(),
        ).await?;
        let mut permission_prompter = bridge::CliPermissionPrompter::new(self.permission_mode);
        let summary = runtime.run_turn(prompt, Some(&mut permission_prompter)).await?;
        Ok(bridge::final_assistant_text(&summary).trim().to_string())
    }

    pub async fn run_internal_prompt_text(
        &self,
        prompt: &str,
        enable_tools: bool,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.run_internal_prompt_text_with_progress(prompt, enable_tools, None).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[tokio::test]
    async fn test_livecli_telemetry_broadcasts() {
        let _guard = env_lock();
        let original_dir = std::env::current_dir().unwrap();
        
        let temp_dir = std::env::temp_dir().join(format!("klako-livecli-{}", std::process::id()));
        std::fs::create_dir_all(temp_dir.join(".kla")).unwrap();
        
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        std::env::set_var("KLA_CONFIG_HOME", temp_dir.join(".kla"));

        std::fs::write(
            temp_dir.join(".kla").join("settings.json"),
            r#"{
              "agency_topology": {
                "default_tier": "L0_thinker",
                "escalation_policy": "sequential_chain",
                "providers": {
                  "L0_thinker": {
                    "engine": "ollama",
                    "model": "llama3"
                  },
                  "L0_typist": {
                    "engine": "ollama",
                    "model": "claude-3-5-sonnet"
                  }
                }
              }
            }"#
        ).unwrap();

        let loader = runtime::ConfigLoader::new(temp_dir.clone(), temp_dir.join(".kla"));
        let _runtime_config = loader.load().expect("config should load");
        
        let (tx, mut rx) = tokio::sync::broadcast::channel(1024);
        
        std::env::set_current_dir(&temp_dir).unwrap();

        let cli = LiveCli::new(
            "llama3".to_string(), 
            true, 
            None, 
            runtime::PermissionMode::WorkspaceWrite, 
            Some(tx.clone())
        ).await.expect("should initialize LiveCli via mock topology");

        cli.print_status();

        let mut telemetry_lines_found = 0;
        while let Ok(msg) = rx.try_recv() {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&msg) {
                if parsed.get("type").and_then(|t| t.as_str()) == Some("CanvasTelemetry") {
                    telemetry_lines_found += 1;
                }
            }
        }
        
        assert!(telemetry_lines_found > 0);
        
        std::env::set_current_dir(original_dir).unwrap();
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
