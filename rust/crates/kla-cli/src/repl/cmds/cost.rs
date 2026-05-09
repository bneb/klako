use crate::repl::LiveCli;
use crate::reporting::format_cost_report;

impl LiveCli {
    pub fn print_cost(&self) {
        let usage = self.runtime.usage().cumulative_usage();
        println!("{}", format_cost_report(usage));
    }
}
