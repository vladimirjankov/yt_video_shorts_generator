use axum::{http::StatusCode, Json, extract::{State}, routing::{post}, Router};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use uuid::Uuid;
use crate::AppState;
use crate::workers::video_storage::{StorageDownloadMessage};


#[derive(Serialize, Deserialize, Clone)]
pub struct VideoTask{
    pub link: String,
    pub number_of_videos: i32,
    pub max_short_length: i32
}

pub fn shorts_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/video", post(post_video))
}


pub async fn post_video(State(state): State<Arc<AppState>>,
                        Json(payload): Json<VideoTask>) -> StatusCode{
    
    let message = StorageDownloadMessage{
        link: payload.link,
        number_of_videos: payload.number_of_videos,
        max_short_length: payload.max_short_length,
        output_path: generate_output_path("mp4")
    };
    state.tx_video_storage.send(message).await.unwrap();
    StatusCode::CREATED  
}



pub fn generate_output_path(extension: &str) -> String {
    format!("../downloads/{}.{}", Uuid::new_v4(), extension)
}