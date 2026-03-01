use crate::coordinator::types::{CoordinatorCommand, WorkerMessage};
use crate::worker::types::Worker;
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

impl Worker {
    pub fn new(coordinator_url: String) -> Self {
        Self {
            coordinator_url,
            max_iters: 1000,
            x_min: -2.0,
            x_max: 1.0,
            y_min: -1.5,
            y_max: 1.5,
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ws_url = build_ws_url(&self.coordinator_url);
        let worker_id = format!("worker-{}", std::process::id());
        println!("Worker '{}' starting, connecting to {}", worker_id, ws_url);

        loop {
            match connect_async(&ws_url).await {
                Ok((stream, _)) => {
                    println!("Connected to coordinator.");
                    self.run_session(stream, &worker_id).await;
                }
                Err(e) => eprintln!("Connection error: {}. Retrying...", e),
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    async fn run_session(&self, mut stream: WsStream, worker_id: &str) {
        if send_hello(&mut stream, worker_id).await.is_err() {
            eprintln!("Failed to send Hello.");
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
                    println!("Coordinator closed connection.");
                    break;
                }
                Some(Err(e)) => {
                    eprintln!("WebSocket error: {}", e);
                    break;
                }
                None => break,
                Some(Ok(_)) => {} // ignorar frames Ping/Pong/Binary
            }
        }

        eprintln!("Disconnected. Reconnecting...");
    }

    async fn handle_command(&self, stream: &mut WsStream, text: &str) -> Result<(), ()> {
        match serde_json::from_str::<CoordinatorCommand>(text) {
            Ok(CoordinatorCommand::Compute { task_id, width, height, start_row, end_row }) => {
                let data = self.compute_block(width, height, start_row, end_row);
                let result = WorkerMessage::ComputeResult { task_id, data };
                let json = serde_json::to_string(&result).map_err(|_| ())?;
                stream.send(Message::Text(json.into())).await.map_err(|_| ())
            }
            Ok(CoordinatorCommand::Welcome(msg)) => {
                println!("Coordinator: {}", msg);
                Ok(())
            }
            Ok(CoordinatorCommand::Wait) => Ok(()),
            Err(e) => {
                eprintln!("Failed to parse command: {}", e);
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

async fn send_hello(stream: &mut WsStream, worker_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let hello = serde_json::to_string(&WorkerMessage::Hello(worker_id.to_string()))?;
    stream.send(Message::Text(hello.into())).await?;
    Ok(())
}
