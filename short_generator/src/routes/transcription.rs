use tokio::sync::mpsc;

pub struct WorkerMessage {
    pub link: String,
    pub number_of_videos: i32,
    pub max_short_length: i32
}

pub async fn spawn_worker() -> mpsc::Sender<WorkerMessage> {
    let (tx, mut rx) = mpsc::channel::<WorkerMessage>(100);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            println!("Processing: {}", msg.link);
        }
    });
    tx
}