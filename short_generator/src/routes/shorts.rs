use axum::{http::StatusCode, Json, extract::{Path, Query, State}, routing::{post}, Router, response::IntoResponse};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use crate::AppState;
use crate::routes::transcription::{WorkerMessage};

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
    
    let message = WorkerMessage{
        link: payload.link,
        number_of_videos: payload.number_of_videos,
        max_short_length: payload.max_short_length
    };
    state.tx.send(message).await.unwrap();
    StatusCode::CREATED  
}