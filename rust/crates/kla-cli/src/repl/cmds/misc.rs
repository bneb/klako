use crate::repl::LiveCli;
use crate::reporting::{
    render_config_report, render_memory_report, render_diff_report, render_version_report,
};

impl LiveCli {
    pub fn print_config(section: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_config_report(section)?);
        Ok(())
    }

    pub fn print_memory() -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_memory_report()?);
        Ok(())
    }

    pub fn print_diff() -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_diff_report()?);
        Ok(())
    }

    pub fn print_version() {
        println!("{}", render_version_report());
    }
}
