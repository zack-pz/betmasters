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
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CoordinatorCommand {
    Welcome(String),
}

pub enum InternalMessage {
    WorkerConnected(mpsc::Sender<CoordinatorCommand>),
    WorkerSaidHello(String),
}

pub struct Coordinator {}

impl Coordinator {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn run(self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
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

        /// The coordinator waits for the workers to connect.
        /// It is a simple implementation to demonstrate a 'distributed hello world'.
        while let Some(msg) = rx.recv().await {
            match msg {
                InternalMessage::WorkerConnected(worker_tx) => {
                    info!("New worker connected. Sending welcome...");
                    let _ = worker_tx
                        .send(CoordinatorCommand::Welcome("Hello distributed".to_string()))
                        .await;
                }
                InternalMessage::WorkerSaidHello(greeting) => {
                    info!("Worker says: {}", greeting);
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
                            }
                        }
                    }
                    _ => break,
                }
            }
        }
    }
}
