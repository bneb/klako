use crate::VisualArtifact;

pub fn render_terminal_artifact(artifact: &VisualArtifact) -> String {
    match artifact {
        VisualArtifact::Histogram { title, data, bins } => render_ascii_histogram(title, data, *bins),
        VisualArtifact::GanttChart { title, events: _ } => format!("\n[Gantt Chart: {title}]\n(Visual support coming soon to terminal, use --browser mode for full rendering)"),
        VisualArtifact::Table { title, headers, rows } => render_ansi_table(title, headers, rows),
    }
}

fn render_ascii_histogram(title: &str, data: &[f64], bins: usize) -> String {
    if data.is_empty() { return format!("\n[{title}] No data available."); }
    
    let min = data.iter().copied().fold(f64::INFINITY, f64::min);
    let max = data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;
    let bin_width = if range == 0.0 { 1.0 } else { range / bins as f64 };
    
    let mut counts = vec![0; bins];
    for &val in data {
        let bin = ((val - min) / bin_width).floor() as usize;
        let bin = bin.min(bins - 1);
        counts[bin] += 1;
    }
    
    let max_count = counts.iter().copied().max().unwrap_or(1);
    let mut output = format!("\n--- {title} ---\n");
    
    for (i, &count) in counts.iter().enumerate() {
        let bin_start = min + (i as f64 * bin_width);
        let bar_len = (f64::from(count) / f64::from(max_count) * 40.0) as usize;
        let bar = "█".repeat(bar_len);
        output.push_str(&format!("{bin_start:>6.2} | {bar:<40} ({count})\n"));
    }
    output
}

fn render_ansi_table(title: &str, headers: &[String], rows: &[Vec<String>]) -> String {
    let mut output = format!("\n--- {title} ---\n");
    for header in headers {
        output.push_str(&format!("{header:<15} "));
    }
    output.push('\n');
    output.push_str(&"-".repeat(headers.len() * 16));
    output.push('\n');
    for row in rows {
        for col in row {
            output.push_str(&format!("{col:<15} "));
        }
        output.push('\n');
    }
    output
}
