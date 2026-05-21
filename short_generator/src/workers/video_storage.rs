use tokio::sync::mpsc;
use std::path::Path;
use std::sync::Arc;
use tokio::process::Command;
use google_drive3::{DriveHub, hyper_rustls, hyper_util, yup_oauth2};
use http_body_util::BodyExt;
use crate::workers::transcription::WorkerMessage;

pub enum Provider {
    GoogleDrive,
    YouTube,
    Unknown,
}

pub struct StorageDownloadMessage{
    pub link: String,
    pub output_path: String,
    pub number_of_videos: i32,
    pub max_short_length: i32
}

type HttpsConnector = hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>;
pub type GDriveHub = DriveHub<HttpsConnector>;

pub async fn spawn_worker_video_storage(hub:  Arc<GDriveHub>, 
                                        tx_transcription: mpsc::Sender<WorkerMessage>) -> mpsc::Sender<StorageDownloadMessage> {
    let (tx, mut rx) = mpsc::channel::<StorageDownloadMessage>(100);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let mut audio_output_path : String;
            match extract_provider(&msg.link) {
                Provider::GoogleDrive => {
                    download_from_gdrive(&hub, &msg.link, &msg.output_path).await;
                    audio_output_path = video_to_audio(&msg.output_path).await;
                    let message = WorkerMessage{
                        audio_path:  audio_output_path,
                        number_of_videos: msg.number_of_videos,
                        max_short_length: msg.max_short_length
                    };

                    tx_transcription.send(message).await;
                },
                _ => println!("Not a valid provider")
            };

        }
    });
    tx
}

pub fn extract_provider(link: &str) -> Provider{
    match link {
        l if l.contains("drive.google.com") => Provider::GoogleDrive,
        l if l.contains("youtube.com") => Provider::YouTube,
        _ => Provider::Unknown,
    }
}


pub fn extract_gdrive_id(link: &str) -> Option<&str> {
    // handles: https://drive.google.com/file/d/<ID>/view
    // https://drive.google.com/file/d/1tQxHzK3iTTr_amHW_kTdKWauth-A6m61/view?usp=sharing
    link.split("/file/d/")
        .nth(1)?
        .split('/')
        .next()
}


pub async fn download_from_gdrive(
    hub: &GDriveHub,
    link: &str,
    output_path: &str,
) {
    let file_id = extract_gdrive_id(link).expect("invalid google drive link");

    let (response, _) = hub
        .files()
        .get(file_id)
        .add_scope("https://www.googleapis.com/auth/drive.readonly")
        .param("alt", "media")
        .doit()
        .await
        .expect("failed to download file");

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    tokio::fs::write(output_path, bytes).await.unwrap();

    println!("Downloaded to {output_path}");
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

pub async fn build_drive_hub() -> GDriveHub {
    let secret = yup_oauth2::read_service_account_key("../service_account.json")
        .await
        .expect("failed to read service_account.json");

    let auth = yup_oauth2::ServiceAccountAuthenticator::builder(secret)
        .build()
        .await
        .expect("failed to build authenticator");

    let client = hyper_util::client::legacy::Client::builder(
        hyper_util::rt::TokioExecutor::new()
    )
    .build(
        hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .unwrap()
            .https_only()
            .enable_http1()
            .build(),
    );

    DriveHub::new(client, auth)
}