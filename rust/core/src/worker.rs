use crate::coordinator::{CoordinatorCommand, WorkerMessage};
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

pub struct Worker {
    coordinator_url: String,
}

impl Worker {
    pub fn new(coordinator_url: String) -> Self {
        Self { coordinator_url }
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let parallelism = std::thread::available_parallelism()?.get();
        info!("Worker redy. Parallelism available: {} threads", parallelism);

        let url = self.coordinator_url.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = Self::connect_and_greet(url).await {
                error!("Worker crashed on startup: {}", e);
            }

        });
        handle.await?;
        Ok(())
    }

    async fn connect_and_greet(url: String) -> Result<(), Box<dyn std::error::Error>> {
        info!("Connecting to coordinator at {}...", url);
        
        let (ws_stream, _) = connect_async(url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Send handshake to coordinator
        let greeting = WorkerMessage::Hello("¡Hello Coordinator!".to_string());
        let json = serde_json::to_string(&greeting)?;
        write.send(Message::Text(json.into())).await?;

        // Worker awaits coordinators commands
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(cmd) = serde_json::from_str::<CoordinatorCommand>(&text) {
                        match cmd {
                            CoordinatorCommand::Welcome(welcome_msg) => {
                                info!("Send by coordinator: {}", welcome_msg);
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("Coordinator closed the connection.");
                    break;
                }
                Err(e) => {
                    error!("WebSocket stream error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }
}
