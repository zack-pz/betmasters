use crate::coordinator::types::{CoordinatorCommand, WorkerMessage};
use crate::worker::types::Worker;
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

impl Worker {
    pub fn new(
        coordinator_url: String,
        max_iters: usize,
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
    ) -> Self {
        Self { coordinator_url, max_iters, x_min, x_max, y_min, y_max }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ws_url = build_ws_url(&self.coordinator_url);
        info!("Worker starting, connecting to {}", ws_url);

        loop {
            match connect_async(&ws_url).await {
                Ok((stream, _)) => {
                    info!("Connected to coordinator.");
                    self.run_session(stream).await;
                }
                Err(e) => error!("Connection error: {}. Retrying...", e),
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    async fn run_session(&self, mut stream: WsStream) {
        if send_hello(&mut stream).await.is_err() {
            error!("Failed to send Hello.");
            return;
        }

        loop {
            match stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    if self.handle_command(&mut stream, &text).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) => {
                    info!("Coordinator closed connection.");
                    break;
                }
                Some(Err(e)) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                None => break,
                Some(Ok(_)) => {} // ignorar frames Ping/Pong/Binary
            }
        }

        info!("Disconnected. Reconnecting...");
    }

    async fn handle_command(&self, stream: &mut WsStream, text: &str) -> Result<(), ()> {
        match serde_json::from_str::<CoordinatorCommand>(text) {
            Ok(CoordinatorCommand::Compute { task_id, width, height, start_row, end_row }) => {
                let data = self.compute_block(width, height, start_row, end_row);
                let result = WorkerMessage::ComputeResult { task_id, data };
                let json = serde_json::to_string(&result).map_err(|_| ())?;
                stream.send(Message::Text(json.into())).await.map_err(|_| ())
            }
            Ok(CoordinatorCommand::Wait) => Ok(()),
            Err(e) => {
                error!("Failed to parse command: {}", e);
                Ok(())
            }
        }
    }
}

fn build_ws_url(coordinator_url: &str) -> String {
    let url = coordinator_url
        .replace("https://", "wss://")
        .replace("http://", "ws://");
    format!("{}/ws", url)
}

async fn send_hello(stream: &mut WsStream) -> Result<(), Box<dyn std::error::Error>> {
    let hello = serde_json::to_string(&WorkerMessage::Hello)?;
    stream.send(Message::Text(hello.into())).await?;
    Ok(())
}
