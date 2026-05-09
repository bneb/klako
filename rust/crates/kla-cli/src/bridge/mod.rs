pub mod client;
pub mod executor;
pub mod prompter;
pub mod progress;
pub mod helpers;

pub use client::DefaultRuntimeClient;
pub use executor::CliToolExecutor;
pub use prompter::CliPermissionPrompter;
pub use progress::{InternalPromptProgressReporter, InternalPromptProgressRun};
pub use helpers::{build_runtime, build_runtime_plugin_state, final_assistant_text};

use std::env;
use api::{AuthSource, resolve_startup_auth_source};
use runtime::ConfigLoader;

pub async fn resolve_cli_auth_source() -> Result<AuthSource, Box<dyn std::error::Error>> {
    Ok(resolve_startup_auth_source(|| {
        let cwd = env::current_dir().map_err(api::ApiError::from)?;
        let config = ConfigLoader::default_for(&cwd).load().map_err(|error| {
            api::ApiError::Auth(format!("failed to load runtime Klako OAuth config: {error}"))
        })?;
        Ok(config.oauth().cloned())
    }).await?)
}

#[must_use] 
pub fn permission_policy(
    mode: runtime::PermissionMode,
    _tool_registry: &tools::GlobalToolRegistry,
) -> runtime::PermissionPolicy {
    runtime::PermissionPolicy::new(mode)
}
