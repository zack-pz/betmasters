use crate::coordinator::types::{CoordinatorCommand, WorkerMessage};
use crate::worker::types::Worker;
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

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
        // Convertir http:// → ws://  y  https:// → wss://
        let ws_url = self
            .coordinator_url
            .replace("https://", "wss://")
            .replace("http://", "ws://");
        let ws_url = format!("{}/ws", ws_url);

        let worker_id = format!("worker-{}", std::process::id());
        println!("Worker '{}' starting, connecting to {}", worker_id, ws_url);

        loop {
            match connect_async(&ws_url).await {
                Ok((mut stream, _)) => {
                    println!("Connected to coordinator.");

                    // Enviar Hello con el ID del worker
                    let hello = serde_json::to_string(&WorkerMessage::Hello(worker_id.clone()))?;
                    if stream.send(Message::Text(hello.into())).await.is_err() {
                        eprintln!("Failed to send Hello. Reconnecting in 2s...");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }

                    // Esperar y procesar comandos del coordinator
                    while let Some(msg) = stream.next().await {
                        match msg {
                            Ok(Message::Text(text)) => {
                                match serde_json::from_str::<CoordinatorCommand>(&text) {
                                    Ok(CoordinatorCommand::Compute {
                                        task_id,
                                        width,
                                        height,
                                        start_row,
                                        end_row,
                                    }) => {
                                        let data =
                                            self.compute_block(width, height, start_row, end_row);
                                        let result = WorkerMessage::ComputeResult { task_id, data };
                                        let text = serde_json::to_string(&result)?;
                                        if stream.send(Message::Text(text.into())).await.is_err() {
                                            eprintln!("Failed to send result. Reconnecting...");
                                            break;
                                        }
                                    }
                                    Ok(CoordinatorCommand::Welcome(msg)) => {
                                        println!("Coordinator: {}", msg);
                                    }
                                    Ok(CoordinatorCommand::Wait) => {
                                        // El coordinator nos dirá cuándo hay trabajo
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to parse command: {}", e);
                                    }
                                }
                            }
                            Ok(Message::Close(_)) => {
                                println!("Coordinator closed connection.");
                                break;
                            }
                            Err(e) => {
                                eprintln!("WebSocket error: {}", e);
                                break;
                            }
                            _ => {}
                        }
                    }

                    eprintln!("Disconnected. Reconnecting in 2s...");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                Err(e) => {
                    eprintln!("Connection error: {}. Retrying in 2s...", e);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }
}
