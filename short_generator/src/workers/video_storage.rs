use tokio::sync::mpsc;
use std::path::Path;
use tokio::process::Command;
use crate::workers::transcription::WorkerMessage;


pub struct VideoStorageMessage {
    pub video_path: String,
    pub number_of_videos: i32,
    pub max_short_length: i32,
}

pub async fn spawn_worker_video_storage(
    tx_transcription: mpsc::Sender<WorkerMessage>,
) -> mpsc::Sender<VideoStorageMessage> {
    let (tx, mut rx) = mpsc::channel::<VideoStorageMessage>(100);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let audio_path = video_to_audio(&msg.video_path).await;
            let _ = tx_transcription.send(WorkerMessage {
                audio_path,
                video_path: msg.video_path,
                number_of_videos: msg.number_of_videos,
                max_short_length: msg.max_short_length,
            }).await;
        }
    });
    tx
}

pub async fn video_to_audio(input_path: &str) -> String {
    let output_path = Path::new(input_path)
        .with_extension("wav")
        .to_str()
        .unwrap()
        .to_string();

    Command::new("ffmpeg")
        .args([
            "-i", input_path,
            "-ar", "16000",
            "-ac", "1",
            "-c:a", "pcm_s16le",
            "-y",
            &output_path,
        ])
        .output()
        .await
        .expect("failed to run ffmpeg");

    output_path
}
