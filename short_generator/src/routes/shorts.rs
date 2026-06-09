use axum::{http::StatusCode, Json, extract::{State}, routing::{post}, Router};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use uuid::Uuid;
use crate::AppState;
use crate::workers::video_download::{DownloadMessage, DownloadDestination, VideoToDownload};
use crate::workers::captions::CaptionStyle;


#[derive(Serialize, Deserialize, Clone)]
pub struct VideoTask {
    pub link: String,
    pub number_of_videos: i32,
    pub max_short_length: i32,
    #[serde(default)]
    pub caption_style: Option<CaptionStyle>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SrtTask {
    pub link: String,
    pub video_link: Option<String>,
    pub number_of_videos: i32,
    pub max_short_length: i32,
    #[serde(default)]
    pub caption_style: Option<CaptionStyle>,
}

pub fn shorts_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/video", post(post_video))
        .route("/srt", post(post_srt))
}

pub async fn post_video(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<VideoTask>,
) -> StatusCode {
    let message = DownloadMessage {
        link: payload.link,
        output_path: generate_output_path("mp4"),
        destination: DownloadDestination::VideoStorage {
            number_of_videos: payload.number_of_videos,
            max_short_length: payload.max_short_length,
            caption_style: payload.caption_style,
        },
    };
    state.tx_video_download.send(message).await.unwrap();
    StatusCode::CREATED
}

pub async fn post_srt(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SrtTask>,
) -> StatusCode {
    let video = payload.video_link.map(|link| VideoToDownload {
        link,
        output_path: generate_output_path("mp4"),
    });

    let message = DownloadMessage {
        link: payload.link,
        output_path: generate_output_path("srt"),
        destination: DownloadDestination::GeminiSrt {
            number_of_videos: payload.number_of_videos,
            max_short_length: payload.max_short_length,
            video,
            caption_style: payload.caption_style,
        },
    };
    state.tx_video_download.send(message).await.unwrap();
    StatusCode::CREATED
}

pub fn generate_output_path(extension: &str) -> String {
    format!("../downloads/{}.{}", Uuid::new_v4(), extension)
}
