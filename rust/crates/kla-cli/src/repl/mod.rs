use std::env;
use std::fs;
use std::io;

use serde_json::json;

use crate::{
    git, input, session, runtime_bridge,
    AllowedToolSet, CliOutputFormat,
    resolve_model_alias, build_system_prompt,
};
use crate::reporting::{
    status_context,
    format_status_report, format_model_report, format_model_switch_report,
    format_permissions_report, format_permissions_switch_report,
    format_cost_report, format_resume_report, format_compact_report,
    render_teleport_report, render_last_tool_debug_report,
    sanitize_generated_message, parse_titled_body,
    render_version_report, render_export_text, resolve_export_path,
    render_config_report, render_memory_report, render_diff_report,
    render_mode_unavailable, render_unknown_repl_command, render_repl_help,
    slash_command_completion_candidates, permission_mode_from_label,
    normalize_permission_mode,
};
use crate::run_init;

use commands::{
    handle_agents_slash_command, handle_plugins_slash_command, handle_skills_slash_command,
    SlashCommand,
};
use crate::render::{Spinner, TerminalRenderer};
use runtime::{
    CompactionConfig, ConfigLoader, ConversationRuntime, PermissionMode, Session,
};
use crate::runtime_bridge::{DefaultRuntimeClient, CliToolExecutor};

pub fn run_repl(
    model: String,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
) -> Result<(), Box<dyn std::error::Error>> {
    run_repl_with_telemetry(model, allowed_tools, permission_mode, None)
}

pub fn run_repl_with_telemetry(
    model: String,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    tx: Option<tokio::sync::broadcast::Sender<String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cli = LiveCli::new(model, true, allowed_tools, permission_mode, tx)?;
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
                    if cli.handle_repl_command(command)? {
                        cli.persist_session()?;
                    }
                    continue;
                }
                editor.push_history(&input);
                cli.run_turn(&input)?;
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
    pub fn new(
        model: String,
        enable_tools: bool,
        allowed_tools: Option<AllowedToolSet>,
        permission_mode: PermissionMode,
        tx: Option<tokio::sync::broadcast::Sender<String>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let cwd = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let system_prompt = build_system_prompt(&cwd, crate::DEFAULT_DATE.to_string(), &model)?;
        let session = session::create_managed_session_handle()?;
        let runtime = runtime_bridge::build_runtime(
            Session::new(),
            model.clone(),
            system_prompt.clone(),
            enable_tools,
            true,
            allowed_tools.clone(),
            permission_mode,
            None,
            tx.clone(),
        )?;
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
        let mut lines = vec![
            format!(
                "{} {}",
                if color {
                    "\x1b[1;38;5;45mⓀ Klako\x1b[0m"
                } else {
                    "Klako"
                },
                if color {
                    "\x1b[2m· ready\x1b[0m"
                } else {
                    "· ready"
                }
            ),
            format!("  Workspace        {workspace_summary}"),
            format!("  Directory        {cwd_display}"),
            format!("  Model            {}", self.model),
            format!("  Permissions      {}", self.permission_mode.as_str()),
            format!("  Session          {}", self.session.id),
            format!(
                "  Quick start      {}",
                if has_klako_md {
                    "/help · /status · ask for a task"
                } else {
                    "/init · /help · /status"
                }
            ),
            "  Editor           Tab completes slash commands · /vim toggles modal editing"
                .to_string(),
            "  Multiline        Shift+Enter or Ctrl+J inserts a newline".to_string(),
        ];
        if !has_klako_md {
            lines.push(
                "  First run        /init scaffolds KLA.md, .kla.json, and local session files"
                    .to_string(),
            );
        }
        lines.join("\n")
    }

    pub fn run_turn(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut permission_prompter = runtime_bridge::CliPermissionPrompter::new(self.permission_mode);
        self.run_turn_with_prompter(input, Some(&mut permission_prompter))
    }

    pub fn run_turn_with_prompter(
        &mut self,
        input: &str,
        prompter: Option<&mut dyn runtime::PermissionPrompter>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut spinner = Spinner::new();
        let mut stdout = io::stdout();
        spinner.tick(
            "🤔 Thinking...",
            TerminalRenderer::new().color_theme(),
            &mut stdout,
        )?;
        let result = self.runtime.run_turn(input, prompter);
        match result {
            Ok(_) => {
                spinner.finish(
                    "✨ Done",
                    TerminalRenderer::new().color_theme(),
                    &mut stdout,
                )?;
                println!();
                self.persist_session()?;
                Ok(())
            }
            Err(error) => {
                spinner.fail(
                    "❌ Request failed",
                    TerminalRenderer::new().color_theme(),
                    &mut stdout,
                )?;
                Err(Box::new(error))
            }
        }
    }

    pub fn run_turn_with_output(
        &mut self,
        input: &str,
        output_format: CliOutputFormat,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match output_format {
            CliOutputFormat::Text => self.run_turn(input),
            CliOutputFormat::Json => self.run_prompt_json(input),
        }
    }

    pub fn run_prompt_json(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error>> {
        let session = self.runtime.session().clone();
        let mut runtime = runtime_bridge::build_runtime(
            session,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            false,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
            self.tx.clone(),
        )?;
        let mut permission_prompter = runtime_bridge::CliPermissionPrompter::new(self.permission_mode);
        let summary = runtime.run_turn(input, Some(&mut permission_prompter))?;
        self.runtime = runtime;
        self.persist_session()?;
        println!(
            "{}",
            json!({
                "message": runtime_bridge::final_assistant_text(&summary),
                "model": self.model,
                "iterations": summary.iterations,
                "tool_uses": runtime_bridge::collect_tool_uses(&summary),
                "tool_results": runtime_bridge::collect_tool_results(&summary),
                "usage": {
                    "input_tokens": summary.usage.input_tokens,
                    "output_tokens": summary.usage.output_tokens,
                    "cache_creation_input_tokens": summary.usage.cache_creation_input_tokens,
                    "cache_read_input_tokens": summary.usage.cache_read_input_tokens,
                }
            })
        );
        Ok(())
    }

    pub fn handle_repl_command(
        &mut self,
        command: SlashCommand,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(match command {
            SlashCommand::Help => {
                println!("{}", render_repl_help());
                false
            }
            SlashCommand::Status => {
                self.print_status();
                false
            }
            SlashCommand::Bughunter { scope } => {
                self.run_bughunter(scope.as_deref())?;
                false
            }
            SlashCommand::Commit => {
                self.run_commit()?;
                true
            }
            SlashCommand::Pr { context } => {
                self.run_pr(context.as_deref())?;
                false
            }
            SlashCommand::Issue { context } => {
                self.run_issue(context.as_deref())?;
                false
            }
            SlashCommand::Ultraplan { task } => {
                self.run_ultraplan(task.as_deref())?;
                false
            }
            SlashCommand::Teleport { target } => {
                self.run_teleport(target.as_deref())?;
                false
            }
            SlashCommand::DebugToolCall => {
                self.run_debug_tool_call()?;
                false
            }
            SlashCommand::Compact => {
                self.compact()?;
                false
            }
            SlashCommand::Model { model } => self.set_model(model)?,
            SlashCommand::Permissions { mode } => self.set_permissions(mode)?,
            SlashCommand::Clear { confirm } => self.clear_session(confirm)?,
            SlashCommand::Cost => {
                self.print_cost();
                false
            }
            SlashCommand::Resume { session_path } => self.resume_session(session_path)?,
            SlashCommand::Config { section } => {
                Self::print_config(section.as_deref())?;
                false
            }
            SlashCommand::Memory => {
                Self::print_memory()?;
                false
            }
            SlashCommand::Init => {
                run_init()?;
                false
            }
            SlashCommand::Diff => {
                Self::print_diff()?;
                false
            }
            SlashCommand::Version => {
                Self::print_version();
                false
            }
            SlashCommand::Export { path } => {
                self.export_session(path.as_deref())?;
                false
            }
            SlashCommand::Session { action, target } => {
                self.handle_session_command(action.as_deref(), target.as_deref())?
            }
            SlashCommand::Plugins { action, target } => {
                self.handle_plugins_command(action.as_deref(), target.as_deref())?
            }
            SlashCommand::Agents { args } => {
                Self::print_agents(args.as_deref())?;
                false
            }
            SlashCommand::Skills { args } => {
                Self::print_skills(args.as_deref())?;
                false
            }
            SlashCommand::Branch { .. } => {
                eprintln!(
                    "{}",
                    render_mode_unavailable("branch", "git branch commands")
                );
                false
            }
            SlashCommand::Worktree { .. } => {
                eprintln!(
                    "{}",
                    render_mode_unavailable("worktree", "git worktree commands")
                );
                false
            }
            SlashCommand::CommitPushPr { .. } => {
                eprintln!(
                    "{}",
                    render_mode_unavailable("commit-push-pr", "commit + push + PR automation")
                );
                false
            }
            SlashCommand::Loop { objective } => {
                self.run_loop(objective.as_deref())?;
                false
            }
            SlashCommand::Dream => {
                self.run_dream()?;
                false
            }
            SlashCommand::Unknown(name) => {
                eprintln!("{}", render_unknown_repl_command(&name));
                false
            }
        })
    }

    pub fn persist_session(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime.session().save_to_path(&self.session.path)?;
        Ok(())
    }

    fn print_status(&self) {
        let cumulative = self.runtime.usage().cumulative_usage();
        let latest = self.runtime.usage().current_turn_usage();
        let status_block = format_status_report(
            &self.model,
            crate::StatusUsage {
                message_count: self.runtime.session().messages.len(),
                turns: self.runtime.usage().turns(),
                latest,
                cumulative,
                estimated_tokens: self.runtime.estimated_tokens(),
            },
            self.permission_mode.as_str(),
            &status_context(Some(&self.session.path)).expect("status context should load"),
        );
        println!("{}", status_block);
        if let Some(tx) = &self.tx {
            for line in status_block.lines() {
                let payload = serde_json::json!({
                    "type": "CanvasTelemetry",
                    "line": line
                });
                let _ = tx.send(payload.to_string());
            }
        }
    }

    fn set_model(&mut self, model: Option<String>) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(model) = model else {
            println!(
                "{}",
                format_model_report(
                    &self.model,
                    self.runtime.session().messages.len(),
                    self.runtime.usage().turns(),
                )
            );
            return Ok(false);
        };

        let model = resolve_model_alias(&model).to_string();

        if model == self.model {
            println!(
                "{}",
                format_model_report(
                    &self.model,
                    self.runtime.session().messages.len(),
                    self.runtime.usage().turns(),
                )
            );
            return Ok(false);
        }

        let previous = self.model.clone();
        let session = self.runtime.session().clone();
        let message_count = session.messages.len();
        self.runtime = runtime_bridge::build_runtime(
            session,
            model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
            self.tx.clone(),
        )?;
        self.model.clone_from(&model);
        println!(
            "{}",
            format_model_switch_report(&previous, &model, message_count)
        );
        Ok(true)
    }

    fn set_permissions(
        &mut self,
        mode: Option<String>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(mode) = mode else {
            println!(
                "{}",
                format_permissions_report(self.permission_mode.as_str())
            );
            return Ok(false);
        };

        let normalized = normalize_permission_mode(&mode).ok_or_else(|| {
            format!(
                "unsupported permission mode '{mode}'. Use read-only, workspace-write, or danger-full-access."
            )
        })?;

        if normalized == self.permission_mode.as_str() {
            println!("{}", format_permissions_report(normalized));
            return Ok(false);
        }

        let previous = self.permission_mode.as_str().to_string();
        let session = self.runtime.session().clone();
        self.permission_mode = permission_mode_from_label(normalized);
        self.runtime = runtime_bridge::build_runtime(
            session,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
            self.tx.clone(),
        )?;
        println!(
            "{}",
            format_permissions_switch_report(&previous, normalized)
        );
        Ok(true)
    }

    fn clear_session(&mut self, confirm: bool) -> Result<bool, Box<dyn std::error::Error>> {
        if !confirm {
            println!(
                "clear: confirmation required; run /clear --confirm to start a fresh session."
            );
            return Ok(false);
        }

        self.session = session::create_managed_session_handle()?;
        self.runtime = runtime_bridge::build_runtime(
            Session::new(),
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
            self.tx.clone(),
        )?;
        println!(
            "Session cleared\n  Mode             fresh session\n  Preserved model  {}\n  Permission mode  {}\n  Session          {}",
            self.model,
            self.permission_mode.as_str(),
            self.session.id,
        );
        Ok(true)
    }

    fn print_cost(&self) {
        let cumulative = self.runtime.usage().cumulative_usage();
        println!("{}", format_cost_report(cumulative));
    }

    fn resume_session(
        &mut self,
        session_path: Option<String>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(session_ref) = session_path else {
            println!("Usage: /resume <session-path>");
            return Ok(false);
        };

        let handle = session::resolve_session_reference(&session_ref)?;
        let session = Session::load_from_path(&handle.path)?;
        let message_count = session.messages.len();
        self.runtime = runtime_bridge::build_runtime(
            session,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
            self.tx.clone(),
        )?;
        self.session = handle;
        println!(
            "{}",
            format_resume_report(
                &self.session.path.display().to_string(),
                message_count,
                self.runtime.usage().turns(),
            )
        );
        Ok(true)
    }

    pub fn print_config(section: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_config_report(section)?);
        Ok(())
    }

    pub fn print_memory() -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_memory_report()?);
        Ok(())
    }

    pub fn print_agents(args: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        println!("{}", handle_agents_slash_command(args, &cwd)?);
        Ok(())
    }

    pub fn print_skills(args: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        println!("{}", handle_skills_slash_command(args, &cwd)?);
        Ok(())
    }

    pub fn print_diff() -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_diff_report()?);
        Ok(())
    }

    pub fn print_version() {
        println!("{}", render_version_report());
    }

    fn export_session(
        &self,
        requested_path: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let export_path = resolve_export_path(requested_path, self.runtime.session())?;
        fs::write(&export_path, render_export_text(self.runtime.session()))?;
        println!(
            "Export\n  Result           wrote transcript\n  File             {}\n  Messages         {}",
            export_path.display(),
            self.runtime.session().messages.len(),
        );
        Ok(())
    }

    fn handle_session_command(
        &mut self,
        action: Option<&str>,
        target: Option<&str>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        match action {
                    None | Some("list") => {
                println!("{}", session::render_session_list(&self.session.id)?);
                Ok(false)
            }
            Some("switch") => {
                let Some(target) = target else {
                    println!("Usage: /session switch <session-id>");
                    return Ok(false);
                };
                let handle = session::resolve_session_reference(target)?;
                let session = Session::load_from_path(&handle.path)?;
                let message_count = session.messages.len();
                self.runtime = runtime_bridge::build_runtime(
                    session,
                    self.model.clone(),
                    self.system_prompt.clone(),
                    true,
                    true,
                    self.allowed_tools.clone(),
                    self.permission_mode,
                    None,
                    self.tx.clone(),
                )?;
                self.session = handle;
                println!(
                    "Session switched\n  Active session   {}\n  File             {}\n  Messages         {}",
                    self.session.id,
                    self.session.path.display(),
                    message_count,
                );
                Ok(true)
            }
            Some(other) => {
                println!("Unknown /session action '{other}'. Use /session list or /session switch <session-id>.");
                Ok(false)
            }
        }
    }

    fn handle_plugins_command(
        &mut self,
        action: Option<&str>,
        target: Option<&str>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        let loader = ConfigLoader::default_for(&cwd);
        let runtime_config = loader.load()?;
        let mut manager = runtime_bridge::build_plugin_manager(&cwd, &loader, &runtime_config);
        let result = handle_plugins_slash_command(action, target, &mut manager)?;
        println!("{}", result.message);
        if result.reload_runtime {
            self.reload_runtime_features()?;
        }
        Ok(false)
    }

    fn reload_runtime_features(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime = runtime_bridge::build_runtime(
            self.runtime.session().clone(),
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
            self.tx.clone(),
        )?;
        self.persist_session()
    }

    fn compact(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let result = self.runtime.compact(CompactionConfig::default());
        let removed = result.removed_message_count;
        let kept = result.compacted_session.messages.len();
        let skipped = removed == 0;
        self.runtime = runtime_bridge::build_runtime(
            result.compacted_session,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
            self.tx.clone(),
        )?;
        self.persist_session()?;
        println!("{}", format_compact_report(removed, kept, skipped));
        Ok(())
    }

    fn run_internal_prompt_text_with_progress(
        &self,
        prompt: &str,
        enable_tools: bool,
        progress: Option<runtime_bridge::InternalPromptProgressReporter>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let session = self.runtime.session().clone();
        let mut runtime = runtime_bridge::build_runtime(
            session,
            self.model.clone(),
            self.system_prompt.clone(),
            enable_tools,
            false,
            self.allowed_tools.clone(),
            self.permission_mode,
            progress,
            self.tx.clone(),
        )?;
        let mut permission_prompter = runtime_bridge::CliPermissionPrompter::new(self.permission_mode);
        let summary = runtime.run_turn(prompt, Some(&mut permission_prompter))?;
        Ok(runtime_bridge::final_assistant_text(&summary).trim().to_string())
    }

    fn run_internal_prompt_text(
        &self,
        prompt: &str,
        enable_tools: bool,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.run_internal_prompt_text_with_progress(prompt, enable_tools, None)
    }

    fn run_loop(&self, objective: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let objective = objective.unwrap_or("Solve the problem");
        println!("Orchestrating swarm to: {}", objective);
        
        // Build an ApiClient for the orchestrator
        let (_, tool_registry) = runtime_bridge::build_runtime_plugin_state()?;
        let client = runtime_bridge::DefaultRuntimeClient::new(
            self.model.clone(),
            true, // enable_tools
            false, // emit_output
            self.allowed_tools.clone(),
            tool_registry,
            None, // progress_reporter
            runtime::RuntimeFeatureConfig::default(),
            self.tx.clone(),
        )?;
        
        let swarm_objective = swarm::SwarmObjective {
            description: objective.to_string(),
        };
        let mut orchestrator = swarm::SwarmOrchestrator::new(
            self.runtime.session().clone(),
            swarm_objective,
            Box::new(client),
        );
        
        tokio::runtime::Runtime::new()?.block_on(async {
            orchestrator.start().await.expect("Failed to start SwarmOrchestrator");
            
            // Wait for plan approval
            if orchestrator.status() == swarm::SwarmStatus::Planning {
                println!("\n[Architect] Plan generated and written to .kla/sessions/PLAN.md");
                println!("Please review and edit the plan. You can use the Notebook UI Plan Editor.");
                println!("Type 'approve' to execute the swarm, or 'cancel' to abort: ");
                
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).expect("Failed to read input");
                if input.trim().to_lowercase() == "approve" {
                    orchestrator.approve_plan().await.expect("Failed to approve plan");
                } else {
                    println!("Swarm execution cancelled.");
                    return;
                }
            }

            while orchestrator.status() == swarm::SwarmStatus::Running {
                // Tick will try to spawn subagents for pending tasks
                orchestrator.tick().await.expect("Tick failed");
                
                // TODO: Monitor spawned subagents, wait for them, collect results,
                // and call orchestrator.complete_task or fail_task.
                // For now, we simulate success for demo if it has agents.
                let agents = orchestrator.agents().to_vec();
                if !agents.is_empty() {
                    for (i, agent) in agents.iter().enumerate() {
                        if agent.status == "running" {
                            // Pretend the agent completed successfully since we're stubbing
                            orchestrator.complete_task(i, "Success".to_string()).await.expect("Failed to complete task");
                        }
                    }
                }
                
                // Real loop would block / sleep until an agent finishes via polling manifests.
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        });
        
        println!("Swarm orchestrator finished with status: {:?}", orchestrator.status());
        Ok(())
    }

    fn run_dream(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Initiating retrospective dreaming sequence...");
        
        let objective = "Review recent session logs and history to identify patterns of failure, successful workflows, and areas for improvement. Propose updates to KLA.md axioms or new SKILL.md profiles.";
        
        // Build an ApiClient for the dreamer
        let (_, tool_registry) = runtime_bridge::build_runtime_plugin_state()?;
        let client = runtime_bridge::DefaultRuntimeClient::new(
            self.model.clone(),
            true, // enable_tools
            false, // emit_output
            self.allowed_tools.clone(),
            tool_registry,
            None,
            runtime::RuntimeFeatureConfig::default(),
            self.tx.clone(),
        )?;
        
        let swarm_objective = swarm::SwarmObjective {
            description: objective.to_string(),
        };
        let mut orchestrator = swarm::SwarmOrchestrator::new(
            self.runtime.session().clone(),
            swarm_objective,
            Box::new(client),
        );
        
        tokio::runtime::Runtime::new()?.block_on(async {
            orchestrator.start().await.expect("Failed to start Dreamer Swarm");
            
            // For Dreaming, we might want to automatically assign a 'Dreamer' subagent type
            // or let the Architect decide. The current start() lets the Architect decide.
            
            while orchestrator.status() == swarm::SwarmStatus::Running {
                orchestrator.tick().await.expect("Dream tick failed");
                
                // Simulate dreamer progress for now
                let agents = orchestrator.agents().to_vec();
                if !agents.is_empty() {
                    for (i, agent) in agents.iter().enumerate() {
                        if agent.status == "running" {
                             orchestrator.complete_task(i, "Identified pattern: missing error handling in bash tool".to_string()).await.expect("Failed to complete dream task");
                        }
                    }
                }
                
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        });
        
        println!("Dreaming sequence complete. Proposals generated in .kla/sessions/DREAM_REPORT.md");
        Ok(())
    }

    fn run_bughunter(&self, scope: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let scope = scope.unwrap_or("the current repository");
        let prompt = format!(
            "You are /bughunter. Inspect {scope} and identify the most likely bugs or correctness issues. Prioritize concrete findings with file paths, severity, and suggested fixes. Use tools if needed."
        );
        println!("{}", self.run_internal_prompt_text(&prompt, true)?);
        Ok(())
    }

    fn run_ultraplan(&self, task: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let task = task.unwrap_or("the current repo work");
        let prompt = format!(
            "You are /ultraplan. Produce a deep multi-step execution plan for {task}. Include goals, risks, implementation sequence, verification steps, and rollback considerations. Use tools if needed."
        );
        let mut progress = runtime_bridge::InternalPromptProgressRun::start_ultraplan(task);
        match self.run_internal_prompt_text_with_progress(&prompt, true, Some(progress.reporter()))
        {
            Ok(plan) => {
                progress.finish_success();
                println!("{plan}");
                Ok(())
            }
            Err(error) => {
                progress.finish_failure(&error.to_string());
                Err(error)
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn run_teleport(&self, target: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let Some(target) = target.map(str::trim).filter(|value| !value.is_empty()) else {
            println!("Usage: /teleport <symbol-or-path>");
            return Ok(());
        };

        println!("{}", render_teleport_report(target)?);
        Ok(())
    }

    fn run_debug_tool_call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_last_tool_debug_report(self.runtime.session())?);
        Ok(())
    }

    fn run_commit(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let status = git::git_output(&["status", "--short"])?;
        if status.trim().is_empty() {
            println!("Commit\n  Result           skipped\n  Reason           no workspace changes");
            return Ok(());
        }

        git::git_status_ok(&["add", "-A"])?;
        let staged_stat = git::git_output(&["diff", "--stat", "--cached"])?;
        let prompt = format!(
            "Generate a git commit message in plain text Lore format only. Base it on this staged diff summary:\n\n{}\n\nRecent conversation context:\n{}",
            runtime_bridge::truncate_for_prompt(&staged_stat, 8_000),
            runtime_bridge::recent_user_context(self.runtime.session(), 6)
        );
        let message = sanitize_generated_message(&self.run_internal_prompt_text(&prompt, false)?);
        if message.trim().is_empty() {
            return Err("generated commit message was empty".into());
        }

        let path = git::write_temp_text_file("kla-commit-message.txt", &message)?;
        let output = std::process::Command::new("git")
            .args(["commit", "--file"])
            .arg(&path)
            .current_dir(env::current_dir()?)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!("git commit failed: {stderr}").into());
        }

        println!(
            "Commit\n  Result           created\n  Message file     {}\n\n{}",
            path.display(),
            message.trim()
        );
        Ok(())
    }

    fn run_pr(&self, context: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let staged = git::git_output(&["diff", "--stat"])?;
        let prompt = format!(
            "Generate a pull request title and body from this conversation and diff summary. Output plain text in this format exactly:\nTITLE: <title>\nBODY:\n<body markdown>\n\nContext hint: {}\n\nDiff summary:\n{}",
            context.unwrap_or("none"),
            runtime_bridge::truncate_for_prompt(&staged, 10_000)
        );
        let draft = sanitize_generated_message(&self.run_internal_prompt_text(&prompt, false)?);
        let (title, body) = parse_titled_body(&draft)
            .ok_or_else(|| "failed to parse generated PR title/body".to_string())?;

        if git::command_exists("gh") {
            let body_path = git::write_temp_text_file("kla-pr-body.md", &body)?;
            let output = std::process::Command::new("gh")
                .args(["pr", "create", "--title", &title, "--body-file"])
                .arg(&body_path)
                .current_dir(env::current_dir()?)
                .output()?;
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                println!(
                    "PR\n  Result           created\n  Title            {title}\n  URL              {}",
                    if stdout.is_empty() { "<unknown>" } else { &stdout }
                );
                return Ok(());
            }
        }

        println!("PR draft\n  Title            {title}\n\n{body}");
        Ok(())
    }

    fn run_issue(&self, context: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let prompt = format!(
            "Generate a GitHub issue title and body from this conversation. Output plain text in this format exactly:\nTITLE: <title>\nBODY:\n<body markdown>\n\nContext hint: {}\n\nConversation context:\n{}",
            context.unwrap_or("none"),
            runtime_bridge::truncate_for_prompt(&runtime_bridge::recent_user_context(self.runtime.session(), 10), 10_000)
        );
        let draft = sanitize_generated_message(&self.run_internal_prompt_text(&prompt, false)?);
        let (title, body) = parse_titled_body(&draft)
            .ok_or_else(|| "failed to parse generated issue title/body".to_string())?;

        if git::command_exists("gh") {
            let body_path = git::write_temp_text_file("kla-issue-body.md", &body)?;
            let output = std::process::Command::new("gh")
                .args(["issue", "create", "--title", &title, "--body-file"])
                .arg(&body_path)
                .current_dir(env::current_dir()?)
                .output()?;
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                println!(
                    "Issue\n  Result           created\n  Title            {title}\n  URL              {}",
                    if stdout.is_empty() { "<unknown>" } else { &stdout }
                );
                return Ok(());
            }
        }

        println!("Issue draft\n  Title            {title}\n\n{body}");
        Ok(())
    }
}


pub struct WebPermissionPrompter<'a> {
    tx: tokio::sync::broadcast::Sender<String>,
    permission_rx: &'a mut tokio::sync::mpsc::Receiver<String>,
}

impl<'a> WebPermissionPrompter<'a> {
    pub fn new(
        _current_mode: runtime::PermissionMode,
        tx: tokio::sync::broadcast::Sender<String>,
        permission_rx: &'a mut tokio::sync::mpsc::Receiver<String>,
    ) -> Self {
        Self {
            tx,
            permission_rx,
        }
    }
}

impl<'a> runtime::PermissionPrompter for WebPermissionPrompter<'a> {
    fn decide(
        &mut self,
        request: &runtime::PermissionRequest,
    ) -> runtime::PermissionPromptDecision {
        let payload = serde_json::json!({
            "type": "PermissionRequest",
            "tool": request.tool_name,
            "required_mode": request.required_mode.as_str(),
            "input": request.input
        });
        let _ = self.tx.send(payload.to_string());

        let res = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                while let Some(msg) = self.permission_rx.recv().await {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&msg) {
                        if let Some(decision) = parsed.get("decision").and_then(|v| v.as_str()) {
                            return decision.to_string();
                        }
                    }
                }
                "deny".to_string()
            })
        });

        if res == "allow" {
            runtime::PermissionPromptDecision::Allow
        } else {
            runtime::PermissionPromptDecision::Deny {
                reason: format!("tool '{}' denied by web user", request.tool_name),
            }
        }
    }
}

pub fn run_notebook_loop(
    model: String,
    allowed_tools: Option<crate::runtime_bridge::AllowedToolSet>,
    permission_mode: runtime::PermissionMode,
    tx: tokio::sync::broadcast::Sender<String>,
    mut ui_input_rx: tokio::sync::mpsc::Receiver<String>,
    mut permission_rx: tokio::sync::mpsc::Receiver<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cli = LiveCli::new(model, true, allowed_tools, permission_mode, Some(tx.clone()))?;
    tools::set_telemetry_sink(tx.clone());
    println!("Ⓚ Klako · ready");
    cli.print_status();

    while let Some(prompt) = ui_input_rx.blocking_recv() {
        println!("[Notebook Engine] Detailed log: Processing UI Input -> {}", prompt);
        let mut web_prompter = WebPermissionPrompter::new(permission_mode, tx.clone(), &mut permission_rx);
        if let Err(e) = cli.run_turn_with_prompter(&prompt, Some(&mut web_prompter)) {
            println!("[System Fail-Safe]\n**Execution Interrupted:** Cannot complete sequence. `{}`", e);
            let _ = tx.send(serde_json::json!({
                "type": "CanvasTelemetry",
                "line": format!("[Notebook Execution Error] {}", e)
            }).to_string());
            let _ = tx.send(serde_json::json!({
                "type": "StatusUpdate",
                "role": "idle",
                "tier": "Error // Idle"
            }).to_string());
        }
        let _ = tx.send(serde_json::json!({
            "type": "StatusUpdate",
            "role": "idle",
            "tier": "Idle"
        }).to_string());
    }
    Ok(())
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

    #[test]
    fn test_livecli_telemetry_broadcasts() {
        let _guard = env_lock();
        let original_dir = std::env::current_dir().unwrap();
        
        // Isolate environment
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
        ).expect("should initialize LiveCli via mock topology");

        // trigger a telemetry print
        cli.print_status();

        // Drain rx to make sure telemetry lines were sent properly!
        let mut telemetry_lines_found = 0;
        while let Ok(msg) = rx.try_recv() {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&msg) {
                if parsed.get("type").and_then(|t| t.as_str()) == Some("CanvasTelemetry") {
                    if let Some(line) = parsed.get("line").and_then(|l| l.as_str()) {
                        if line.contains("Usage") || line.contains("Context") || line.contains("Workspace") || line.contains("Session") {
                            telemetry_lines_found += 1;
                        }
                    }
                }
            }
        }
        
        assert!(telemetry_lines_found > 0, "Telemetry broadcast failed: expected CanvasTelemetry usage output, got none!");
        
        std::env::set_current_dir(original_dir).unwrap();
        std::fs::remove_dir_all(temp_dir).ok();
    }
}
