mod routes;
use axum::{routing::get, Router};
use tokio::sync::mpsc;
use std::sync::Arc;
use routes::transcription::{WorkerMessage, spawn_worker, init_whisper};
use routes::shorts::shorts_routes;

pub struct AppState{
    tx: mpsc::Sender<WorkerMessage>,
}

async fn init_app_state() -> Arc<AppState> {    
    
    let ctx = init_whisper("../models/ggml-large-v3-turbo.bin");
    // create app state
    Arc::new(AppState{
        tx: spawn_worker(Arc::clone(&ctx)).await
    })
}

pub async fn create_app() -> Router {

    let app_state = init_app_state().await;

    Router::new().nest("/shorts", shorts_routes().with_state(app_state))
}