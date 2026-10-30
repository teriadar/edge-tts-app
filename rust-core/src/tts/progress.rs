use std::io::{self, Write};
use std::time::Instant;

use crate::tts::constants::{MP3_BITRATE_BPS, TICKS_PER_SECOND};

const FULL_BLOCK: char = '█';
const PARTIAL_BLOCKS: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];
const MIN_BAR_WIDTH: usize = 3;
const DEFAULT_WIDTH: usize = 80;

pub fn format_time(seconds: f64) -> String {
    let total_secs = seconds.max(0.0) as u64;
    let minutes = total_secs / 60;
    let secs = total_secs % 60;
    let hours = minutes / 60;
    let mins = minutes % 60;
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, mins, secs)
    } else {
        format!("{:02}:{:02}", mins, secs)
    }
}

pub fn format_bytes(num_bytes: u64) -> String {
    if num_bytes < 1024 * 1024 {
        format!("{}KB", num_bytes / 1024)
    } else {
        format!("{:.1}MB", (num_bytes as f64) / 1024.0 / 1024.0)
    }
}

pub fn render_bar(fraction: f64, width: usize) -> String {
    let clamped = fraction.clamp(0.0, 1.0);
    let filled = clamped * (width as f64);
    let whole = filled as usize;
    if whole >= width {
        return std::iter::repeat_n(FULL_BLOCK, width).collect();
    }
    let partial_idx = (((filled - (whole as f64)) * 8.0) as usize).min(PARTIAL_BLOCKS.len() - 1);
    let mut s = String::with_capacity(width * 3);
    for _ in 0..whole {
        s.push(FULL_BLOCK);
    }
    s.push(PARTIAL_BLOCKS[partial_idx]);
    for _ in 0..(width - whole - 1) {
        s.push(' ');
    }
    s
}

pub struct ProgressBar {
    text: String,
    width: usize,
    min_interval_secs: f64,
    cursor: usize,
    audio_bytes: u64,
    timeline_ticks: u64,
    start: Instant,
    last_render: Option<Instant>,
    fraction: f64,
    complete: bool,
    finished: bool,
}

impl ProgressBar {
    pub fn new(text: impl Into<String>, width: Option<usize>, min_interval_secs: f64) -> Self {
        Self {
            text: text.into(),
            width: width.unwrap_or(DEFAULT_WIDTH - 1),
            min_interval_secs,
            cursor: 0,
            audio_bytes: 0,
            timeline_ticks: 0,
            start: Instant::now(),
            last_render: None,
            fraction: 0.0,
            complete: false,
            finished: false,
        }
    }

    pub fn text_fraction(&self) -> f64 {
        if self.text.is_empty() {
            return 0.0;
        }
        ((self.cursor as f64) / (self.text.len() as f64)).min(1.0)
    }

    pub fn fraction(&mut self) -> f64 {
        if self.complete {
            return 1.0;
        }
        let estimated_total = self.estimated_total_seconds();
        if estimated_total > 0.0 {
            let current = (self.received_seconds() / estimated_total).min(1.0);
            if current > self.fraction {
                self.fraction = current;
            }
        }
        self.fraction
    }

    fn received_seconds(&self) -> f64 {
        (self.audio_bytes as f64) * 8.0 / (MP3_BITRATE_BPS as f64)
    }

    fn estimated_total_seconds(&self) -> f64 {
        let tf = self.text_fraction();
        if tf <= 0.0 || self.timeline_ticks == 0 {
            return 0.0;
        }
        (self.timeline_ticks as f64) / (TICKS_PER_SECOND as f64) / tf
    }

    pub fn advance(&mut self, spoken_text: &str) {
        if spoken_text.is_empty() || self.cursor >= self.text.len() {
            return;
        }
        if let Some(rel_idx) = self.text[self.cursor..].find(spoken_text) {
            self.cursor += rel_idx + spoken_text.len();
        }
    }

    pub fn set_audio_timeline(&mut self, ticks: u64) {
        if ticks > self.timeline_ticks {
            self.timeline_ticks = ticks;
        }
    }

    pub fn add_audio_bytes(&mut self, num_bytes: u64) {
        self.audio_bytes += num_bytes;
    }

    pub fn render(&mut self) {
        let now = Instant::now();
        if let Some(last) = self.last_render {
            if now.duration_since(last).as_secs_f64() < self.min_interval_secs {
                return;
            }
        }
        self.last_render = Some(now);
        let line = self.line();
        self.write_stderr(&line, "");
    }

    pub fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.complete = true;
        let line = self.line();
        self.write_stderr(&line, "\n");
    }

    pub fn close(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        if self.last_render.is_some() {
            let mut stderr = io::stderr();
            let _ = stderr.write_all(b"\n");
            let _ = stderr.flush();
        }
    }

    fn line(&mut self) -> String {
        let frac = self.fraction();
        let elapsed = self.start.elapsed().as_secs_f64();
        let eta = if frac > 0.0 {
            format_time(elapsed / frac - elapsed)
        } else {
            "--:--".to_string()
        };
        let prefix = format!("{:3}%|", (frac * 100.0) as u32);
        let suffix = format!(
            "| {}  {}<{}",
            format_bytes(self.audio_bytes),
            format_time(elapsed),
            eta
        );
        let used = prefix.len() + suffix.len();
        let bar_width = self.width.saturating_sub(used).max(MIN_BAR_WIDTH);
        format!("{}{}{}", prefix, render_bar(frac, bar_width), suffix)
    }

    fn write_stderr(&self, line: &str, end: &str) {
        let mut stderr = io::stderr();
        let _ = write!(stderr, "\r{:width$}{}", line, end, width = self.width);
        let _ = stderr.flush();
    }
}
