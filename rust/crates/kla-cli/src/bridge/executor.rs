use std::io;
use runtime::{ToolError, ToolExecutor};
use tools::GlobalToolRegistry;
use crate::reporting::format_tool_result;
use crate::render::TerminalRenderer;

pub struct CliToolExecutor {
    pub(crate) allowed_tools: Option<crate::AllowedToolSet>,
    pub(crate) emit_output: bool,
    pub(crate) tool_registry: GlobalToolRegistry,
    pub(crate) renderer: TerminalRenderer,
}

impl CliToolExecutor {
    #[must_use] 
    pub fn new(
        allowed_tools: Option<crate::AllowedToolSet>,
        emit_output: bool,
        tool_registry: GlobalToolRegistry,
    ) -> Self {
        Self {
            allowed_tools,
            emit_output,
            tool_registry,
            renderer: TerminalRenderer::new(),
        }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for CliToolExecutor {
    async fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        if self
            .allowed_tools
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(tool_name))
        {
            return Err(ToolError::new(format!(
                "tool `{tool_name}` is not enabled by the current --allowedTools setting"
            )));
        }
        let value = serde_json::from_str(input)
            .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
        match self.tool_registry.execute(tool_name, &value).await {
            Ok(output) => {
                if self.emit_output {
                    let markdown = format_tool_result(tool_name, &output, false);
                    self.renderer
                        .stream_markdown(&markdown, &mut io::stdout())
                        .map_err(|error: io::Error| ToolError::new(error.to_string()))?;
                }
                Ok(output)
            }
            Err(error) => {
                if self.emit_output {
                    let markdown = format_tool_result(tool_name, &error, true);
                    self.renderer
                        .stream_markdown(&markdown, &mut io::stdout())
                        .map_err(|stream_error: io::Error| ToolError::new(stream_error.to_string()))?;
                }
                Err(ToolError::new(error))
            }
        }
    }
}
