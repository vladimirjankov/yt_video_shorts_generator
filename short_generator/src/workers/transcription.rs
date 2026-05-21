use tokio::sync::mpsc;
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};
use std::sync::Arc;
use std::path::Path;
use hound;

pub struct WorkerMessage {
    pub audio_path: String,
    pub number_of_videos: i32,
    pub max_short_length: i32
}

pub async fn spawn_worker_transcription(ctx: Arc<WhisperContext>) -> mpsc::Sender<WorkerMessage> {
    let (tx, mut rx) = mpsc::channel::<WorkerMessage>(100);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            println!("Processing: {}", msg.audio_path);
            process_message(&ctx, msg);
        }
    });
    tx
}

pub fn process_message(ctx: &WhisperContext, msg: WorkerMessage){
    let mut state = ctx.create_state().expect("failed to create whisper state");

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("sr"));
    params.set_print_progress(false);
    params.set_print_realtime(false);


    let reader = hound::WavReader::open(&msg.audio_path).expect("failed to open wav");
    let audio_data: Vec<f32> = reader
        .into_samples::<i16>()
        .map(|s| s.unwrap() as f32 / 32768.0)
        .collect();

    state.full(params, &audio_data).expect("failed to transcribe");

    let num_segments = state.full_n_segments();
    let mut srt = String::new();
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
    }

    let out_path = Path::new(&msg.audio_path).with_extension("srt");
    std::fs::write(&out_path, &srt).expect("failed to write transcript");
    println!("transcript saved to {}", out_path.display());
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

