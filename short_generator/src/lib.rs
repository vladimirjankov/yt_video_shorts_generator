mod routes;
mod workers;
use axum::{Router};
use tokio::sync::mpsc;
use std::sync::Arc;
use workers::transcription::{WorkerMessage, spawn_worker_transcription, init_whisper};
use workers::video_storage::{VideoStorageMessage, spawn_worker_video_storage};
use workers::video_download::{DownloadMessage, spawn_worker_video_download, build_drive_hub};
use workers::gemini_analysis::{AnalysisMessage, spawn_worker_gemini_analysis};
use workers::cutter::{CutterMessage, spawn_worker_cutter};
use routes::shorts::shorts_routes;

pub struct AppState{
    tx_transcription: mpsc::Sender<WorkerMessage>,
    tx_video_storage: mpsc::Sender<VideoStorageMessage>,
    tx_video_download: mpsc::Sender<DownloadMessage>,
    tx_gemini: mpsc::Sender<AnalysisMessage>,
    tx_cutter: mpsc::Sender<CutterMessage>,
}

async fn init_app_state() -> Arc<AppState> {
    let api_key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY not set");
    let ctx = init_whisper("../models/ggml-large-v3-turbo.bin");
    let hub = Arc::new(build_drive_hub().await);

    let tx_c  = spawn_worker_cutter().await;
    let tx_g  = spawn_worker_gemini_analysis(api_key, tx_c.clone()).await;
    let tx_t  = spawn_worker_transcription(Arc::clone(&ctx), tx_g.clone()).await;
    let tx_vs = spawn_worker_video_storage(tx_t.clone()).await;
    let tx_vd = spawn_worker_video_download(Arc::clone(&hub), tx_vs.clone(), tx_g.clone()).await;

    Arc::new(AppState{
        tx_transcription: tx_t,
        tx_video_storage: tx_vs,
        tx_video_download: tx_vd,
        tx_gemini: tx_g,
        tx_cutter: tx_c,
    })
}

pub async fn create_app() -> Router {

    let app_state = init_app_state().await;

    Router::new().nest("/shorts", shorts_routes().with_state(app_state))
}
