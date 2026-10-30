use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::PathBuf;

use clap::Parser;
use pagina_core::tts::constants::DEFAULT_VOICE;
use pagina_core::tts::{
    list_voices, BoundaryType, Communicate, EdgeTtsError, ProgressBar, Result, SubMaker, TtsChunk,
};
use pagina_core::PaginaEngine;

#[derive(Parser, Debug)]
#[command(name = "edge-tts-rs", version)]
pub struct CliArgs {

    #[arg(short = 't', long = "text", group = "input")]
    pub text: Option<String>,

    #[arg(short = 'f', long = "file", group = "input")]
    pub file: Option<String>,

    #[arg(short = 'v', long = "voice", default_value = DEFAULT_VOICE)]
    pub voice: String,

    #[arg(short = 'l', long = "list-voices", group = "input")]
    pub list_voices: bool,

    #[arg(short = 'H', long = "health", group = "input")]
    pub health: bool,

    #[arg(long = "rate", default_value = "+0%")]
    pub rate: String,

    #[arg(long = "volume", default_value = "+0%")]
    pub volume: String,

    #[arg(long = "pitch", default_value = "+0Hz")]
    pub pitch: String,

    #[arg(long = "write-media")]
    pub write_media: Option<String>,

    #[arg(long = "write-subtitles")]
    pub write_subtitles: Option<String>,

    #[arg(long = "proxy")]
    pub proxy: Option<String>,

    #[arg(long = "no-progress")]
    pub no_progress: bool,
}

async fn print_voices(proxy: Option<&str>) -> Result<()> {
    let mut voices = list_voices(proxy).await?;
    voices.sort_by(|a, b| a.short_name.cmp(&b.short_name));

    println!(
        "{:<35} {:<10} {:<30} VoicePersonalities",
        "Name", "Gender", "ContentCategories"
    );
    println!("{}", "-".repeat(105));
    for v in voices {
        let categories = v.voice_tag.content_categories.join(", ");
        let personalities = v.voice_tag.voice_personalities.join(", ");
        println!(
            "{:<35} {:<10} {:<30} {}",
            v.short_name, v.gender, categories, personalities
        );
    }
    Ok(())
}

async fn run_epub_tts(args: &CliArgs, epub_path: &str) -> Result<()> {
    let chapters = PaginaEngine::load_epub(epub_path)?;
    eprintln!(
        "[EPUB] Loaded '{}' -> {} chapter(s) found",
        epub_path,
        chapters.len()
    );

    let mut all_audio = Vec::new();

    for (ch_idx, chapter) in chapters.iter().enumerate() {
        eprintln!(
            "\n=== Chapter {}: '{}' ({} paragraphs) ===",
            ch_idx + 1,
            chapter.title,
            chapter.paragraphs.len()
        );

        for para in &chapter.paragraphs {
            eprintln!(
                "  [Paragraph #{}] Text: \"{}\" ({} words)",
                para.id,
                para.text,
                para.words.len()
            );
            for w in &para.words {
                eprintln!(
                    "    WordSpan(id={}, text=\"{}\", chars={}..{})",
                    w.id, w.text, w.char_start, w.char_end
                );
            }

            let chunks = PaginaEngine::chunk_paragraph(para, 4);
            for sc in &chunks {
                eprintln!(
                    "  -> Synthesizing SentenceChunk #{} (chars={}..{}, words={:?}): \"{}\"",
                    sc.chunk_index, sc.char_start, sc.char_end, sc.word_ids, sc.text
                );
                let synth =
                    PaginaEngine::synthesize_sentence_chunk(para, sc, &args.voice, &args.rate)
                        .await?;
                eprintln!(
                    "     Received MP3 audio: {} bytes | Aligned WordAudioTimings: {}",
                    synth.audio_mp3.len(),
                    synth.word_timings.len()
                );
                for wt in &synth.word_timings {
                    eprintln!(
                        "       [WordAudioTiming] word_id={} \"{}\" (chars {}..{}) -> {}ms .. {}ms (duration: {}ms)",
                        wt.word_id,
                        wt.text,
                        wt.char_start,
                        wt.char_end,
                        wt.audio_offset_ms,
                        wt.audio_end_ms,
                        wt.audio_duration_ms
                    );
                }
                all_audio.extend_from_slice(&synth.audio_mp3);
            }
        }
    }

    if let Some(ref media_path) = args.write_media {
        fs::write(media_path, &all_audio)?;
        eprintln!(
            "\n[OK] Total MP3 audio saved to '{}' ({} bytes)",
            media_path,
            all_audio.len()
        );
    }

    Ok(())
}

async fn run_tts(args: &CliArgs, text: &str) -> Result<()> {
    let mut communicate = Communicate::with_options(
        text,
        &args.voice,
        &args.rate,
        &args.volume,
        &args.pitch,
        BoundaryType::WordBoundary,
    )?;
    let mut submaker = SubMaker::new();
    let show_progress = !args.no_progress && args.write_subtitles.as_deref() != Some("-");
    let mut progress = if show_progress {
        Some(ProgressBar::new(text.to_string(), None, 0.1))
    } else {
        None
    };

    let chunks = match communicate.stream().await {
        Ok(c) => c,
        Err(e) => {
            if let Some(ref mut p) = progress {
                p.close();
            }
            return Err(e);
        }
    };

    let mut audio_out: Box<dyn Write> = match args.write_media.as_deref() {
        Some(path) if path != "-" => Box::new(File::create(PathBuf::from(path))?),
        _ => Box::new(io::stdout()),
    };

    for chunk in &chunks {
        match chunk {
            TtsChunk::Audio { data } => {
                audio_out.write_all(data)?;
                if let Some(ref mut p) = progress {
                    p.add_audio_bytes(data.len() as u64);
                }
            }
            TtsChunk::WordBoundary {
                offset,
                duration,
                text: word_text,
            }
            | TtsChunk::SentenceBoundary {
                offset,
                duration,
                text: word_text,
            } => {
                submaker.feed(chunk)?;
                if let Some(ref mut p) = progress {
                    p.advance(word_text);
                    p.set_audio_timeline(offset + duration);
                }
            }
        }
        if let Some(ref mut p) = progress {
            p.render();
        }
    }

    audio_out.flush()?;
    if let Some(ref mut p) = progress {
        p.finish();
    }

    if let Some(ref sub_path) = args.write_subtitles {
        let srt = submaker.get_srt();
        if sub_path == "-" {
            io::stderr().write_all(srt.as_bytes())?;
        } else {
            fs::write(sub_path, srt.as_bytes())?;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = CliArgs::parse();

    if args.health {
        let report = PaginaEngine::check_health(5000).await;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    if args.list_voices {
        print_voices(args.proxy.as_deref()).await?;
        return Ok(());
    }

    if let Some(ref file_path) = args.file {
        if file_path.to_lowercase().ends_with(".epub") {
            return run_epub_tts(&args, file_path).await;
        }
    }

    let text = if let Some(ref t) = args.text {
        t.clone()
    } else if let Some(ref file_path) = args.file {
        if file_path == "-" || file_path == "/dev/stdin" {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            buf
        } else {
            fs::read_to_string(file_path)?
        }
    } else {
        return Err(EdgeTtsError::InvalidConfig(
            "One of --text, --file, or --list-voices is required.".to_string(),
        ));
    };

    run_tts(&args, &text).await
}
