mod routes;
mod workers;
use axum::{Router};
use tokio::sync::mpsc;
use std::sync::Arc;
use workers::transcription::{WorkerMessage, spawn_worker_transcription, init_whisper};
use workers::video_storage::{spawn_worker_video_storage, StorageDownloadMessage, build_drive_hub};
use routes::shorts::shorts_routes;

pub struct AppState{
    tx_transcription: mpsc::Sender<WorkerMessage>,
    tx_video_storage: mpsc::Sender<StorageDownloadMessage>,
}

async fn init_app_state() -> Arc<AppState> {    
    
    let ctx = init_whisper("../models/ggml-large-v3-turbo.bin");
    let hub = Arc::new(build_drive_hub().await);

    let tx_t = spawn_worker_transcription(Arc::clone(&ctx)).await;
    let tx_vs = spawn_worker_video_storage(Arc::clone(&hub), tx_t.clone()).await;
    // create app state
    Arc::new(AppState{
        tx_transcription: tx_t,
        tx_video_storage: tx_vs
    })
}

pub async fn create_app() -> Router {

    let app_state = init_app_state().await;

    Router::new().nest("/shorts", shorts_routes().with_state(app_state))
}