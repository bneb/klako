use crate::repl::LiveCli;
use crate::reporting::{
    render_teleport_report, render_last_tool_debug_report,
    sanitize_generated_message, parse_titled_body,
};

impl LiveCli {
    pub async fn run_bughunter(&self, scope: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let scope = scope.unwrap_or("the current directory");
        let prompt = format!("Act as a Bug Hunter. Scan {scope} for potential logic errors, security vulnerabilities, or performance bottlenecks. Report your findings clearly.");
        println!("{}", self.run_internal_prompt_text(&prompt, true).await?);
        Ok(())
    }

    pub async fn run_commit(&self) -> Result<(), Box<dyn std::error::Error>> {
        let prompt = "Analyze the current workspace changes and generate a high-quality, descriptive git commit message following conventional commits. Return ONLY the title and body, separated by a blank line. Prefix the title with 'TITLE: ' and the body with 'BODY:' on a new line.";
        let res = self.run_internal_prompt_text(prompt, true).await?;
        if let Some((title, body)) = parse_titled_body(&res) {
            let sanitized_title = sanitize_generated_message(&title);
            
            println!("\nProposed Commit:\n{sanitized_title}\n\n{body}");
            print!("Confirm commit? (y/N): ");
            use std::io::Write;
            let _ = std::io::stdout().flush();
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if input.trim().to_lowercase() == "y" {
                crate::git::git_commit(&sanitized_title, &body)?;
                println!("Committed.");
            } else {
                println!("Commit aborted.");
            }
        } else {
            println!("Failed to parse commit message from assistant response.");
        }
        Ok(())
    }

    pub async fn run_pr(&self, context: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let context = context.unwrap_or("the current branch changes");
        let prompt = format!("Generate a Pull Request description for {context}. Include a summary of changes and why they are necessary.");
        println!("{}", self.run_internal_prompt_text(&prompt, true).await?);
        Ok(())
    }

    pub async fn run_issue(&self, context: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let context = context.unwrap_or("the reported problem");
        let prompt = format!("Draft a technical GitHub issue for {context}. Include steps to reproduce, expected behavior, and actual behavior.");
        println!("{}", self.run_internal_prompt_text(&prompt, true).await?);
        Ok(())
    }

    pub async fn run_ultraplan(&self, task: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let task = task.unwrap_or("the next major feature");
        let prompt = format!("Act as a Staff Engineer. Create a detailed, multi-phase execution plan for {task}. Break it down into verifiable sub-tasks.");
        println!("{}", self.run_internal_prompt_text(&prompt, true).await?);
        Ok(())
    }

    pub fn run_teleport(&self, target: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_teleport_report(target.unwrap_or("current"))?);
        Ok(())
    }

    pub fn run_debug_tool_call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_last_tool_debug_report(self.runtime.session())?);
        Ok(())
    }
}
