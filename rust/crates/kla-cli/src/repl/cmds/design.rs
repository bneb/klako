use crate::repl::LiveCli;

impl LiveCli {
    pub(crate) fn run_design(&self, feature: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let feature_target = feature.unwrap_or("the current system architecture");
        println!("Initiating collaborative design session for: {}", feature_target);
        
        let prompt = format!(
            "You are /design. We are starting a collaborative brainstorming session for: {}. \
            Please load your 'technical_design' skill. First, use DiscoveryWorld to analyze the relevant codebase context. \
            Then, ask me 1-3 targeted questions to clarify the architectural direction before drafting the formal Markdown design document.",
            feature_target
        );
        
        println!("{}", self.run_internal_prompt_text(&prompt, true)?);
        Ok(())
    }
}