use crate::repl::LiveCli;
use crate::reporting::{
    format_compact_report, render_config_report,
};
use commands::{handle_agents_slash_command, handle_plugins_slash_command, handle_skills_slash_command};
use crate::bridge;

impl LiveCli {
    pub async fn compact(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let result = self.runtime.compact_session().await?;
        println!("{}", format_compact_report(result.removed_message_count, self.runtime.session().messages.len(), false));
        Ok(())
    }

    pub async fn run_settings(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Klako Settings (Interactive TUI coming soon)");
        println!("{}", render_config_report(None)?);
        Ok(())
    }

    pub async fn handle_session_command(&mut self, action: Option<&str>, _target: Option<&str>) -> Result<bool, Box<dyn std::error::Error>> {
        if let Some("list") = action {
            println!("Listing sessions (coming soon)");
            Ok(false)
        } else {
            println!("Usage: /session <list|load|save>");
            Ok(false)
        }
    }

    pub async fn handle_plugins_command(&mut self, action: Option<&str>, target: Option<&str>) -> Result<bool, Box<dyn std::error::Error>> {
        let cwd = std::env::current_dir()?;
        let loader = runtime::ConfigLoader::default_for(&cwd);
        let runtime_config = loader.load()?;
        let mut manager = bridge::helpers::build_plugin_manager(&cwd, &loader, &runtime_config);
        let res = handle_plugins_slash_command(action, target, &mut manager)?;
        println!("{}", res.message);
        Ok(false)
    }

    pub fn print_agents(args: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let cwd = std::env::current_dir()?;
        println!("{}", handle_agents_slash_command(args, &cwd)?);
        Ok(())
    }

    pub fn print_skills(args: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let cwd = std::env::current_dir()?;
        println!("{}", handle_skills_slash_command(args, &cwd)?);
        Ok(())
    }
}
