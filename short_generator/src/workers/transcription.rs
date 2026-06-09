use tokio::sync::mpsc;
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};
use std::sync::Arc;
use std::path::Path;
use hound;
use crate::workers::gemini_analysis::AnalysisMessage;
use crate::workers::captions::{CaptionStyle, Word};


pub struct WorkerMessage {
    pub audio_path: String,
    pub video_path: String,
    pub number_of_videos: i32,
    pub max_short_length: i32,
    pub caption_style: Option<CaptionStyle>,
}

pub async fn spawn_worker_transcription(
    ctx: Arc<WhisperContext>,
    tx_gemini: mpsc::Sender<AnalysisMessage>,
) -> mpsc::Sender<WorkerMessage> {
    let (tx, mut rx) = mpsc::channel::<WorkerMessage>(100);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            println!("Processing: {}", msg.audio_path);
            process_message(&ctx, msg, &tx_gemini).await;
        }
    });
    tx
}

pub async fn process_message(
    ctx: &WhisperContext,
    msg: WorkerMessage,
    tx_gemini: &mpsc::Sender<AnalysisMessage>,
) {
    let mut state = ctx.create_state().expect("failed to create whisper state");

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("sr"));
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_token_timestamps(true);
    params.set_split_on_word(true);


    let reader = hound::WavReader::open(&msg.audio_path).expect("failed to open wav");
    let audio_data: Vec<f32> = reader
        .into_samples::<i16>()
        .map(|s| s.unwrap() as f32 / 32768.0)
        .collect();

    state.full(params, &audio_data).expect("failed to transcribe");

    let num_segments = state.full_n_segments();
    let mut srt = String::new();
    let mut words: Vec<Word> = Vec::new();
    for i in 0..num_segments {
        let segment = state.get_segment(i).unwrap();
        let text = segment.to_str().unwrap();
        let t0 = segment.start_timestamp();
        let t1 = segment.end_timestamp();
        srt.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            format_srt_timestamp(t0),
            format_srt_timestamp(t1),
            text.trim(),
        ));
        collect_words(&segment, &mut words);
    }

    let out_path = Path::new(&msg.audio_path).with_extension("srt");
    std::fs::write(&out_path, &srt).expect("failed to write transcript");
    println!("transcript saved to {} ({} words)", out_path.display(), words.len());

    let _ = tx_gemini.send(AnalysisMessage {
        srt_content: srt,
        video_path: Some(msg.video_path),
        number_of_videos: msg.number_of_videos,
        max_short_length: msg.max_short_length,
        caption_style: msg.caption_style,
        words: Some(words),
    }).await;
}

/// Merge a segment's tokens into whole words. With `split_on_word` enabled,
/// whisper starts a new word at a leading space; special `[_...]` tokens are
/// skipped. Token timestamps are centiseconds.
fn collect_words(segment: &whisper_rs::WhisperSegment, words: &mut Vec<Word>) {
    for t in 0..segment.n_tokens() {
        let Some(token) = segment.get_token(t) else { continue };
        let Ok(text) = token.to_str() else { continue };
        if text.starts_with("[_") || text.is_empty() {
            continue;
        }
        let data = token.token_data();
        let start_ms = data.t0 * 10;
        let end_ms = data.t1 * 10;

        let starts_word = text.starts_with(' ') || words.is_empty();
        let piece = text.trim();
        if piece.is_empty() {
            continue;
        }
        if starts_word {
            words.push(Word {
                text: piece.to_string(),
                start_ms,
                end_ms,
            });
        } else if let Some(last) = words.last_mut() {
            last.text.push_str(piece);
            last.end_ms = end_ms;
        }
    }
}

fn format_srt_timestamp(cs: i64) -> String {
    let ms = cs * 10;
    let h = ms / 3_600_000;
    let m = (ms / 60_000) % 60;
    let s = (ms / 1_000) % 60;
    let ms = ms % 1_000;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}

pub fn init_whisper(model_path: &str) -> Arc<WhisperContext> {
    let mut params = WhisperContextParameters::default();
    params.use_gpu(true);
    Arc::new(
        WhisperContext::new_with_params(model_path, params).expect("Failed to load whisper model...")
    )
}
