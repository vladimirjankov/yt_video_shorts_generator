use tokio::sync::mpsc;
use std::sync::Arc;
use google_drive3::{DriveHub, hyper_rustls, hyper_util, yup_oauth2};
use http_body_util::BodyExt;
use crate::workers::video_storage::VideoStorageMessage;
use crate::workers::gemini_analysis::AnalysisMessage;
use crate::workers::captions::CaptionStyle;


pub enum Provider {
    GoogleDrive,
    YouTube,
    Unknown,
}

pub struct VideoToDownload {
    pub link: String,
    pub output_path: String,
}

pub enum DownloadDestination {
    VideoStorage {
        number_of_videos: i32,
        max_short_length: i32,
        caption_style: Option<CaptionStyle>,
    },
    GeminiSrt {
        number_of_videos: i32,
        max_short_length: i32,
        video: Option<VideoToDownload>,
        caption_style: Option<CaptionStyle>,
    },
}

pub struct DownloadMessage {
    pub link: String,
    pub output_path: String,
    pub destination: DownloadDestination,
}

type HttpsConnector = hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>;
pub type GDriveHub = DriveHub<HttpsConnector>;

pub async fn spawn_worker_video_download(
    hub: Arc<GDriveHub>,
    tx_storage: mpsc::Sender<VideoStorageMessage>,
    tx_gemini: mpsc::Sender<AnalysisMessage>,
) -> mpsc::Sender<DownloadMessage> {
    let (tx, mut rx) = mpsc::channel::<DownloadMessage>(100);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match extract_provider(&msg.link) {
                Provider::GoogleDrive => {
                    download_from_gdrive(&hub, &msg.link, &msg.output_path).await;

                    match msg.destination {
                        DownloadDestination::VideoStorage { number_of_videos, max_short_length, caption_style } => {
                            let _ = tx_storage.send(VideoStorageMessage {
                                video_path: msg.output_path,
                                number_of_videos,
                                max_short_length,
                                caption_style,
                            }).await;
                        }
                        DownloadDestination::GeminiSrt { number_of_videos, max_short_length, video, caption_style } => {
                            let srt_content = tokio::fs::read_to_string(&msg.output_path)
                                .await
                                .expect("failed to read downloaded srt");

                            let video_path = if let Some(v) = video {
                                download_from_gdrive(&hub, &v.link, &v.output_path).await;
                                Some(v.output_path)
                            } else {
                                None
                            };

                            let _ = tx_gemini.send(AnalysisMessage {
                                srt_content,
                                video_path,
                                number_of_videos,
                                max_short_length,
                                caption_style,
                                words: None,
                            }).await;
                        }
                    }
                }
                _ => println!("Not a valid provider"),
            };
        }
    });
    tx
}

pub fn extract_provider(link: &str) -> Provider {
    match link {
        l if l.contains("drive.google.com") => Provider::GoogleDrive,
        l if l.contains("youtube.com") => Provider::YouTube,
        _ => Provider::Unknown,
    }
}

pub fn extract_gdrive_id(link: &str) -> Option<&str> {
    link.split("/file/d/")
        .nth(1)?
        .split('/')
        .next()
}

pub async fn download_from_gdrive(hub: &GDriveHub, link: &str, output_path: &str) {
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
