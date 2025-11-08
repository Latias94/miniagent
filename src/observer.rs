use std::path::Path;

pub trait AgentObserver: Send + Sync {
    fn on_log_file(&self, _path: &Path) {}
    fn on_retry(&self, _attempt: u32, _next_delay_secs: f32, _error: &str) {}
    fn on_summarize_start(&self, _before: usize, _threshold: usize) {}
    fn on_summarize_done(&self, _after: usize) {}
    fn on_thinking(&self, _text: &str) {}
    fn on_assistant_text(&self, _text: &str) {}
    fn on_tool_call(&self, _name: &str, _args_preview: &str) {}
    fn on_tool_result(&self, _name: &str, _success: bool, _preview: &str) {}
}

pub struct ConsoleObserver;

impl ConsoleObserver {
    pub fn new() -> Self {
        Self
    }
}

impl AgentObserver for ConsoleObserver {
    fn on_log_file(&self, path: &Path) {
        use colored::*;
        println!("{} {}", "Log file:".dimmed(), path.display());
    }
    fn on_retry(&self, attempt: u32, next_delay_secs: f32, error: &str) {
        use colored::*;
        println!(
            "{} LLM call failed (attempt {}): {}",
            "!".yellow(),
            attempt,
            error
        );
        println!(
            "{} Retrying in {:.1}s (attempt {})...",
            " ".dimmed(),
            next_delay_secs,
            attempt + 1
        );
    }
    fn on_summarize_start(&self, before: usize, threshold: usize) {
        use colored::*;
        println!(
            "\n{} Token estimate: {}/{}",
            "*".yellow().bold(),
            before,
            threshold
        );
        println!(
            "{} Triggering message history summarization...",
            "*".yellow().bold()
        );
    }
    fn on_summarize_done(&self, after: usize) {
        use colored::*;
        println!(
            "{} Summary completed, tokens reduced to {}",
            "✓".green(),
            after
        );
    }
    fn on_thinking(&self, text: &str) {
        use colored::*;
        println!(
            "\n{}\n{}",
            "Thinking:".magenta().bold(),
            format!("{}", text).dimmed()
        );
    }
    fn on_assistant_text(&self, text: &str) {
        use colored::*;
        println!("\n{}\n{}", "Assistant:".bright_blue().bold(), text);
    }
    fn on_tool_call(&self, name: &str, args_preview: &str) {
        use colored::*;
        println!("\n{} {}", "Tool Call:".yellow().bold(), name.cyan().bold());
        for line in args_preview.lines() {
            println!("   {}", line.dimmed());
        }
    }
    fn on_tool_result(&self, _name: &str, success: bool, preview: &str) {
        use colored::*;
        if success {
            println!("{} {}", "Result:".green(), preview);
        } else {
            println!("{} {}", "Error:".red().bold(), preview.red());
        }
    }
}
