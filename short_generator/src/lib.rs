mod routes;
use axum::{routing::get, Router};
use tokio::sync::mpsc;
use std::sync::Arc;
use routes::transcription::{WorkerMessage, spawn_worker};
use routes::shorts::shorts_routes;

pub struct AppState{
    tx: mpsc::Sender<WorkerMessage>,
}

async fn init_app_state() -> Arc<AppState> {    
    // create app state
    Arc::new(AppState{
        tx: spawn_worker().await
    })
}


pub async fn create_app() -> Router {

    let app_state = init_app_state().await;

    Router::new().nest("/shorts", shorts_routes().with_state(app_state))
}