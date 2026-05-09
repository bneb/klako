use crate::repl::LiveCli;
use crate::reporting::{
    format_model_report, format_model_switch_report,
    format_permissions_report, format_permissions_switch_report,
    format_resume_report, render_export_text, resolve_export_path,
    permission_mode_from_label, normalize_permission_mode,
};
use crate::resolve_model_alias;

impl LiveCli {
    pub async fn set_model(&mut self, model: Option<String>) -> Result<bool, Box<dyn std::error::Error>> {
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
        self.model = model.clone();
        self.runtime.set_model(model);
        println!("{}", format_model_switch_report(&previous, &self.model, self.runtime.session().messages.len()));
        Ok(true)
    }

    pub async fn set_permissions(&mut self, mode: Option<String>) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(mode_label) = mode else {
            println!("{}", format_permissions_report(self.permission_mode.as_str()));
            return Ok(false);
        };

        let target_mode = normalize_permission_mode(&mode_label)
            .map_or(self.permission_mode, permission_mode_from_label);

        if target_mode == self.permission_mode {
            println!("{}", format_permissions_report(self.permission_mode.as_str()));
            return Ok(false);
        }

        let previous = self.permission_mode;
        self.permission_mode = target_mode;
        self.runtime.set_permission_mode(target_mode);
        println!(
            "{}",
            format_permissions_switch_report(previous.as_str(), self.permission_mode.as_str())
        );
        Ok(true)
    }

    pub async fn clear_session(&mut self, confirm: bool) -> Result<bool, Box<dyn std::error::Error>> {
        if !confirm {
            println!("Use /clear --confirm to permanently wipe the current session history.");
            return Ok(false);
        }

        self.runtime.clear_session();
        println!("Session history cleared.");
        Ok(true)
    }

    pub async fn resume_session(&mut self, session_path: Option<String>) -> Result<bool, Box<dyn std::error::Error>> {
        let path = session_path.map_or_else(|| self.session.path.clone(), std::path::PathBuf::from);
        let session = runtime::Session::load_from_path(&path)?;
        let message_count = session.messages.len();
        self.runtime.replace_session(session);
        println!("{}", format_resume_report(&path.display().to_string(), message_count, self.runtime.usage().turns()));
        Ok(true)
    }

    pub fn export_session(&self, path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let export_path = resolve_export_path(path, self.runtime.session())?;
        let text = render_export_text(self.runtime.session());
        std::fs::write(&export_path, text)?;
        println!("Session exported to {}", export_path.display());
        Ok(())
    }
}
