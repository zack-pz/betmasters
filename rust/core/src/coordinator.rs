use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    routing::get,
    Router,
};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::sync::mpsc;

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerMessage {
    Hello(String),
    ComputeResult(Vec<Vec<f64>>),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CoordinatorCommand {
    Welcome(String),
    Compute { width: u32, height: u32 },
}

pub enum InternalMessage {
    WorkerConnected(mpsc::Sender<CoordinatorCommand>),
    WorkerSaidHello(String),
    WorkerFinished(Vec<Vec<f64>>),
}

pub struct Coordinator {
    pub storage: Vec<Vec<Vec<f64>>>,
}

impl Coordinator {
    pub fn new() -> Self {
        Self {
            storage: Vec::new(),
        }
    }

    pub async fn run(mut self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, mut rx) = mpsc::channel::<InternalMessage>(100);
        let app_state = tx.clone();

        let app = Router::new().route(
            "/ws",
            get(move |ws: WebSocketUpgrade| {
                let state = app_state.clone();
                async move { ws.on_upgrade(move |socket| handle_socket(socket, state)) }
            }),
        );

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = tokio::net::TcpListener::bind(addr).await?;

        info!("Coordinator listening on {}", addr);

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!("Axum server error: {}", e);
            }
        });

        // The coordinator waits for the workers to connect.
        // It is a simple implementation to demonstrate a 'distributed hello world'.
        while let Some(msg) = rx.recv().await {
            match msg {
                InternalMessage::WorkerConnected(worker_tx) => {
                    info!("New worker connected. Sending welcome...");
                    let _ = worker_tx
                        .send(CoordinatorCommand::Welcome("Hello distributed".to_string()))
                        .await;
                    
                    // Send a test computation task
                    info!("Sending test compute task (width: 100, height: 100)...");
                    let _ = worker_tx
                        .send(CoordinatorCommand::Compute { width: 100, height: 100 })
                        .await;
                }
                InternalMessage::WorkerSaidHello(greeting) => {
                    info!("Worker says: {}", greeting);
                }
                InternalMessage::WorkerFinished(result) => {
                    info!("Worker finished computation. Data received: {} rows", result.len());
                    self.storage.push(result);
                    info!("Total datasets registered in coordinator: {}", self.storage.len());
                }
            }
        }

        Ok(())
    }
}

async fn handle_socket(mut socket: WebSocket, tx: mpsc::Sender<InternalMessage>) {
    let (worker_tx, mut worker_rx) = mpsc::channel::<CoordinatorCommand>(10);

    if tx
        .send(InternalMessage::WorkerConnected(worker_tx.clone()))
        .await
        .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            // Implement the communication between the coodinator and worker
            Some(cmd) = worker_rx.recv() => {
                let json = serde_json::to_string(&cmd).unwrap();
                if socket.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
            // The coordinator awaits worker data
            Some(result) = socket.recv() => {
                match result {
                    Ok(Message::Text(text)) => {
                        if let Ok(msg) = serde_json::from_str::<WorkerMessage>(&text) {
                            match msg {
                                WorkerMessage::Hello(greeting) => {
                                    let _ = tx.send(InternalMessage::WorkerSaidHello(greeting)).await;
                                }
                                WorkerMessage::ComputeResult(result) => {
                                    let _ = tx.send(InternalMessage::WorkerFinished(result)).await;
                                }
                            }
                        }
                    }
                    _ => break,
                }
            }
        }
    }
}
