use tokio::sync::mpsc;
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};
use std::sync::Arc;

pub struct WorkerMessage {
    pub link: String,
    pub number_of_videos: i32,
    pub max_short_length: i32
}

pub async fn spawn_worker(ctx: Arc<WhisperContext>) -> mpsc::Sender<WorkerMessage> {
    let (tx, mut rx) = mpsc::channel::<WorkerMessage>(100);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            println!("Processing: {}", msg.link);
            process_message(&ctx, msg);
        }
    });
    tx
}

pub fn process_message(ctx: &WhisperContext, msg: WorkerMessage){
    println!("Heyyy")
}

pub fn init_whisper(model_path: &str) -> Arc<WhisperContext> {
    let mut params = WhisperContextParameters::default();
    params.use_gpu(true);
    Arc::new(
        WhisperContext::new_with_params(model_path, params).expect("Failed to load whisper model...")
    )
}

