use tokio::sync::mpsc;
use tokio::process::Command;
use std::path::Path;
use crate::workers::gemini_analysis::ShortSegment;


pub struct CutterMessage {
    pub video_path: String,
    pub srt_content: String,
    pub segments: Vec<ShortSegment>,
}

pub async fn spawn_worker_cutter() -> mpsc::Sender<CutterMessage> {
    let (tx, mut rx) = mpsc::channel::<CutterMessage>(100);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            cut_video(msg).await;
        }
    });
    tx
}

pub async fn cut_video(msg: CutterMessage) {
    let stem = Path::new(&msg.video_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video")
        .to_string();

    let _ = msg.srt_content; // TODO: burn subtitles onto each short

    for (i, segment) in msg.segments.iter().enumerate() {
        let start = srt_to_ffmpeg_timestamp(&segment.timestamp_start);
        let end = srt_to_ffmpeg_timestamp(&segment.timestamp_end);
        let output_path = format!("../downloads/{}_short_{}.mp4", stem, i + 1);

        println!("[cutter] {} -> {} | {}", start, end, output_path);

        let output = Command::new("ffmpeg")
            .args([
                "-i", &msg.video_path,
                "-ss", &start,
                "-to", &end,
                "-c:v", "libx264",
                "-c:a", "aac",
                "-y",
                &output_path,
            ])
            .output()
            .await
            .expect("failed to run ffmpeg");

        if !output.status.success() {
            eprintln!("[cutter] ffmpeg failed for short {}: {}",
                      i + 1,
                      String::from_utf8_lossy(&output.stderr));
        }
    }
}

fn srt_to_ffmpeg_timestamp(srt: &str) -> String {
    srt.replace(',', ".")
}
