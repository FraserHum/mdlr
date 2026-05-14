use std::io::IsTerminal;
use std::time::Instant;

use indicatif::{
    MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle,
};

fn colored(code: &str, text: &str) -> String {
    if std::env::var_os("NO_COLOR").is_some() {
        text.to_string()
    } else {
        format!("\x1b[{code}m{text}\x1b[0m")
    }
}

fn finish_line(
    icon: &str,
    color: &str,
    name: &str,
    suffix: &str,
    elapsed: &str,
) -> String {
    let icon = colored(color, icon);
    if suffix.is_empty() {
        format!("{icon} {name:<24} {elapsed}")
    } else {
        format!(
            "{icon} {name} ({suffix}){:>w$}",
            elapsed,
            w = 24usize.saturating_sub(name.len() + suffix.len() + 3)
        )
    }
}

pub struct CheckProgress {
    multi: MultiProgress,
    enabled: bool,
}

impl CheckProgress {
    pub fn new(quiet: bool) -> Self {
        let enabled = !quiet && std::io::stderr().is_terminal();
        let multi = if enabled {
            MultiProgress::with_draw_target(ProgressDrawTarget::stderr())
        } else {
            MultiProgress::with_draw_target(ProgressDrawTarget::hidden())
        };
        Self { multi, enabled }
    }

    pub fn start_spinner(&self, name: &str) -> SpinnerHandle {
        if !self.enabled {
            return SpinnerHandle {
                bar: ProgressBar::hidden(),
                start: Instant::now(),
                name: String::new(),
            };
        }
        let bar = self.multi.add(ProgressBar::new_spinner());
        bar.set_style(
            ProgressStyle::with_template("  {spinner:.cyan} {msg}")
                .unwrap()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
        );
        bar.set_message(format!("{name}..."));
        bar.enable_steady_tick(std::time::Duration::from_millis(80));
        SpinnerHandle { bar, start: Instant::now(), name: name.to_string() }
    }

    pub fn start_bar(&self, name: &str, total: u64) -> BarHandle {
        if !self.enabled {
            return BarHandle {
                bar: ProgressBar::hidden(),
                start: Instant::now(),
                name: String::new(),
            };
        }
        let bar = self.multi.add(ProgressBar::new(total));
        bar.set_style(
            ProgressStyle::with_template(
                "  {spinner:.cyan} {msg}  {pos}/{len}  {elapsed:.dim}",
            )
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
        );
        bar.set_message(name.to_string());
        bar.enable_steady_tick(std::time::Duration::from_millis(80));
        BarHandle { bar, start: Instant::now(), name: name.to_string() }
    }
}

pub struct SpinnerHandle {
    bar: ProgressBar,
    start: Instant,
    name: String,
}

impl SpinnerHandle {
    pub fn finish(self) {
        let elapsed = format_elapsed(self.start.elapsed());
        let msg = finish_line("\u{2713}", "32", &self.name, "", &elapsed);
        self.bar.set_style(ProgressStyle::with_template("  {msg}").unwrap());
        self.bar.finish_with_message(msg);
    }

    pub fn finish_warn(self, detail: &str) {
        let elapsed = format_elapsed(self.start.elapsed());
        let msg = finish_line("\u{26a0}", "33", &self.name, detail, &elapsed);
        self.bar.set_style(ProgressStyle::with_template("  {msg}").unwrap());
        self.bar.finish_with_message(msg);
    }
}

pub struct BarHandle {
    bar: ProgressBar,
    start: Instant,
    name: String,
}

impl BarHandle {
    pub fn set_position(&self, pos: u64) {
        self.bar.set_position(pos);
    }

    pub fn finish(self) {
        let elapsed = format_elapsed(self.start.elapsed());
        let msg = finish_line("\u{2713}", "32", &self.name, "", &elapsed);
        self.bar.set_style(ProgressStyle::with_template("  {msg}").unwrap());
        self.bar.finish_with_message(msg);
    }
}

fn format_elapsed(d: std::time::Duration) -> String {
    let secs = d.as_secs_f64();
    if secs >= 1.0 {
        format!("{secs:.1}s")
    } else {
        format!("{}ms", d.as_millis())
    }
}
