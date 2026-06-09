use tokio::sync::mpsc;
use tokio::process::Command;
use std::path::Path;
use crate::workers::gemini_analysis::{annotate_captions, ShortSegment};
use crate::workers::captions::{
    build_ass, srt_to_interpolated_words, words_in_range, CaptionStyle, EmojiEvent, Word,
};

const FONTS_DIR: &str = "../fonts";
const EMOJI_CACHE_DIR: &str = "../downloads/emoji_cache";
// Twemoji is CC-BY 4.0; 72x72 color PNGs keyed by codepoint.
const TWEMOJI_BASE: &str = "https://cdn.jsdelivr.net/gh/jdecked/twemoji@15.1.0/assets/72x72";

pub struct CutterMessage {
    pub video_path: String,
    pub srt_content: String,
    pub segments: Vec<ShortSegment>,
    pub caption_style: Option<CaptionStyle>,
    pub words: Option<Vec<Word>>,
}

pub async fn spawn_worker_cutter(api_key: String) -> mpsc::Sender<CutterMessage> {
    let (tx, mut rx) = mpsc::channel::<CutterMessage>(100);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            cut_video(msg, &api_key).await;
        }
    });
    tx
}

pub async fn cut_video(msg: CutterMessage, api_key: &str) {
    let stem = Path::new(&msg.video_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video")
        .to_string();

    // Word timing: real (whisper path) or interpolated from the uploaded SRT.
    let words: Vec<Word> = match &msg.words {
        Some(w) => w.clone(),
        None if msg.caption_style.is_some() => srt_to_interpolated_words(&msg.srt_content),
        None => Vec::new(),
    };

    let dims = if msg.caption_style.is_some() {
        probe_dimensions(&msg.video_path).await
    } else {
        None
    };

    for (i, segment) in msg.segments.iter().enumerate() {
        let start = srt_to_ffmpeg_timestamp(&segment.timestamp_start);
        let start_ms = ms_from_ffmpeg(&start);
        let end_ms = ms_from_ffmpeg(&srt_to_ffmpeg_timestamp(&segment.timestamp_end));
        let duration = ((end_ms - start_ms).max(0) as f64) / 1000.0;
        let output_path = format!("../downloads/{}_short_{}.mp4", stem, i + 1);

        // Build per-clip captions (ASS text + color-emoji overlay events) when a
        // style was requested and we have word timing for this clip.
        let mut ass_path: Option<String> = None;
        let mut emoji_files: Vec<(String, EmojiEvent)> = Vec::new();
        let mut emoji_size = 0i32;
        if let (Some(style), Some((w, h))) = (msg.caption_style, dims) {
            let clip_words = words_in_range(&words, start_ms, end_ms);
            if !clip_words.is_empty() {
                let texts: Vec<String> = clip_words.iter().map(|w| w.text.clone()).collect();
                let annotation = annotate_captions(&texts, style, api_key).await;
                let (ass, events) = build_ass(&clip_words, &annotation, style, w, h);

                let path = format!("../downloads/{}_short_{}.ass", stem, i + 1);
                match tokio::fs::write(&path, ass).await {
                    Ok(()) => ass_path = Some(path),
                    Err(e) => eprintln!("[cutter] failed to write ass: {e}"),
                }
                emoji_size = (h as f32 * 0.085).round() as i32;
                for ev in events {
                    if let Some(file) = ensure_twemoji(&ev.codepoints).await {
                        emoji_files.push((file, ev));
                    }
                }
            }
        }

        println!(
            "[cutter] {} +{:.2}s -> {} | captions: {} | emojis: {}",
            start,
            duration,
            output_path,
            ass_path.is_some(),
            emoji_files.len(),
        );

        let mut cmd = Command::new("ffmpeg");
        // `-ss` before `-i` does fast input seeking and rebases output to 0.
        cmd.args(["-ss", &start, "-i", &msg.video_path]);

        match &ass_path {
            Some(ass) if !emoji_files.is_empty() => {
                for (file, _) in &emoji_files {
                    cmd.args(["-i", file]);
                }
                let fc = build_filter_complex(ass, &emoji_files, emoji_size);
                let last = format!("[v{}]", emoji_files.len());
                cmd.args(["-filter_complex", &fc, "-map", &last, "-map", "0:a?"]);
            }
            Some(ass) => {
                let filter =
                    format!("subtitles={}:fontsdir={}", escape_filter_path(ass), FONTS_DIR);
                cmd.args(["-vf", &filter]);
            }
            None => {}
        }
        // `-t` must follow every `-i` so it stays an OUTPUT option (clip length).
        // Placed before an emoji `-i` it would instead limit that image input.
        cmd.args(["-t", &format!("{duration}")]);
        cmd.args(["-c:v", "libx264", "-c:a", "aac", "-y", &output_path]);

        let output = cmd.output().await.expect("failed to run ffmpeg");
        if !output.status.success() {
            eprintln!(
                "[cutter] ffmpeg failed for short {}: {}",
                i + 1,
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

/// Compose the subtitles filter with one color-emoji overlay per event,
/// each gated to its cue's time window. Emoji sit centered, above the words.
fn build_filter_complex(ass: &str, emoji_files: &[(String, EmojiEvent)], size: i32) -> String {
    let mut fc = format!(
        "[0:v]subtitles={}:fontsdir={}[v0]",
        escape_filter_path(ass),
        FONTS_DIR
    );
    for (idx, (_, ev)) in emoji_files.iter().enumerate() {
        let input = idx + 1; // image inputs follow the video at index 0
        let s = ev.start_ms as f64 / 1000.0;
        let e = ev.end_ms as f64 / 1000.0;
        fc.push_str(&format!(
            ";[{input}:v]scale={size}:{size}[e{input}];\
             [v{prev}][e{input}]overlay=x=(W-w)/2:y=(H*0.33):enable='between(t,{s:.3},{e:.3})'[v{input}]",
            prev = idx,
        ));
    }
    fc
}

/// Fetch (and cache) the Twemoji color PNG for `codepoints`, returning its path.
/// Returns None if the asset can't be downloaded so captions degrade gracefully.
async fn ensure_twemoji(codepoints: &str) -> Option<String> {
    let path = format!("{EMOJI_CACHE_DIR}/{codepoints}.png");
    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Some(path);
    }
    if let Err(e) = tokio::fs::create_dir_all(EMOJI_CACHE_DIR).await {
        eprintln!("[cutter] emoji cache dir failed: {e}");
        return None;
    }
    let url = format!("{TWEMOJI_BASE}/{codepoints}.png");
    let resp = reqwest::Client::new().get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        eprintln!("[cutter] twemoji {codepoints} -> {}", resp.status());
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    tokio::fs::write(&path, &bytes).await.ok()?;
    Some(path)
}

/// Read the video's pixel dimensions via ffprobe.
async fn probe_dimensions(path: &str) -> Option<(u32, u32)> {
    let out = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height",
            "-of", "csv=s=x:p=0",
            path,
        ])
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (w, h) = text.trim().split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

fn srt_to_ffmpeg_timestamp(srt: &str) -> String {
    srt.replace(',', ".")
}

/// Parse an `HH:MM:SS.mmm` ffmpeg timestamp into milliseconds.
fn ms_from_ffmpeg(ts: &str) -> i64 {
    let (hms, frac) = ts.split_once('.').unwrap_or((ts, "0"));
    let parts: Vec<&str> = hms.split(':').collect();
    let (h, m, s) = match parts.as_slice() {
        [h, m, s] => (
            h.parse().unwrap_or(0),
            m.parse().unwrap_or(0),
            s.parse().unwrap_or(0),
        ),
        [m, s] => (0i64, m.parse().unwrap_or(0), s.parse().unwrap_or(0)),
        _ => (0, 0, 0),
    };
    let millis: i64 = format!("{frac:0<3}")[..3].parse().unwrap_or(0);
    ((h * 60 + m) * 60 + s) * 1000 + millis
}

/// Escape a path for use inside an ffmpeg filter argument.
fn escape_filter_path(path: &str) -> String {
    path.replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}
