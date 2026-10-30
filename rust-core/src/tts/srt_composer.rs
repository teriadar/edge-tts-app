use std::cmp::Ordering;
use std::time::Duration;

use regex::Regex;

const SECONDS_IN_HOUR: u64 = 3600;
const SECONDS_IN_MINUTE: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subtitle {
    pub index: Option<usize>,
    pub start: Duration,
    pub end: Duration,
    pub content: String,
}

impl Subtitle {
    pub fn new(index: Option<usize>, start: Duration, end: Duration, content: impl Into<String>) -> Self {
        Self {
            index,
            start,
            end,
            content: content.into(),
        }
    }

    pub fn to_srt(&self, eol: Option<&str>) -> String {
        let eol_str = eol.unwrap_or("\n");
        let mut output_content = make_legal_content(&self.content);
        if eol_str != "\n" {
            output_content = output_content.replace('\n', eol_str);
        }

        format!(
            "{idx}{eol}{start} --> {end}{eol}{content}{eol}{eol}",
            idx = self.index.unwrap_or(0),
            start = duration_to_srt_timestamp(self.start),
            end = duration_to_srt_timestamp(self.end),
            content = output_content,
            eol = eol_str,
        )
    }
}

impl PartialOrd for Subtitle {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Subtitle {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.start, self.end, self.index).cmp(&(other.start, other.end, other.index))
    }
}

pub fn make_legal_content(content: &str) -> String {
    if !content.is_empty() && !content.starts_with('\n') && !content.contains("\n\n") {
        return content.to_string();
    }
    let multi_ws_re = Regex::new(r"\n\n+").unwrap();
    let trimmed = content.trim_matches('\n');
    multi_ws_re.replace_all(trimmed, "\n").into_owned()
}

pub fn duration_to_srt_timestamp(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hrs = total_secs / SECONDS_IN_HOUR;
    let secs_remainder = total_secs % SECONDS_IN_HOUR;
    let mins = secs_remainder / SECONDS_IN_MINUTE;
    let secs = secs_remainder % SECONDS_IN_MINUTE;
    let msecs = duration.subsec_millis();
    format!("{:02}:{:02}:{:02},{:03}", hrs, mins, secs, msecs)
}

fn should_skip_sub(subtitle: &Subtitle) -> bool {
    if subtitle.content.trim().is_empty() {
        return true;
    }
    if subtitle.start >= subtitle.end {
        return true;
    }
    false
}

pub fn sort_and_reindex(
    subtitles: &[Subtitle],
    start_index: usize,
    skip: bool,
) -> Vec<Subtitle> {
    let mut sorted = subtitles.to_vec();
    sorted.sort();

    let mut result = Vec::with_capacity(sorted.len());
    let mut next_idx = start_index;

    for mut sub in sorted {
        if skip && should_skip_sub(&sub) {
            continue;
        }
        sub.index = Some(next_idx);
        next_idx += 1;
        result.push(sub);
    }
    result
}

pub fn compose(
    subtitles: &[Subtitle],
    reindex: bool,
    start_index: usize,
    eol: Option<&str>,
) -> String {
    let subs: Vec<Subtitle> = if reindex {
        sort_and_reindex(subtitles, start_index, true)
    } else {
        subtitles.to_vec()
    };

    let mut out = String::new();
    for sub in &subs {
        out.push_str(&sub.to_srt(eol));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srt_composer_sorts_skips_invalid_and_formats_timestamps() {
        let subs = vec![
            Subtitle::new(
                Some(9),
                Duration::from_millis(3_661_250),
                Duration::from_millis(3_662_500),
                "Second cue",
            ),
            Subtitle::new(
                Some(2),
                Duration::from_millis(500),
                Duration::from_millis(500),
                "Zero duration cue to skip",
            ),
            Subtitle::new(
                Some(3),
                Duration::from_millis(100),
                Duration::from_millis(900),
                "   ",
            ),
            Subtitle::new(
                Some(1),
                Duration::from_millis(200),
                Duration::from_millis(800),
                "\nFirst\n\n\nline\n",
            ),
        ];

        let composed = compose(&subs, true, 1, None);
        assert!(composed.contains("1\n00:00:00,200 --> 00:00:00,800\nFirst\nline\n\n"));
        assert!(composed.contains("2\n01:01:01,250 --> 01:01:02,500\nSecond cue\n\n"));
        assert!(!composed.contains("Zero duration"));
    }
}
