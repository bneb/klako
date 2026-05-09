use crate::repl::LiveCli;

impl LiveCli {
    pub async fn run_review(&self, path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let target_path = path.unwrap_or(".");
        println!("Initiating autonomous code review for: {target_path}");
        
        let prompt = format!(
            "You are /review. Inspect the following path and identify code smells, potential bugs, and opportunities for refactoring. \
            Be critical but constructive. Path: {target_path}"
        );
        
        println!("{}", self.run_internal_prompt_text(&prompt, true).await?);
        Ok(())
    }
}
