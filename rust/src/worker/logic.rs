use crate::coordinator::{CoordinatorCommand, WorkerMessage};
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use num_traits::Num;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::worker::types::Worker;

impl<T> Worker<T>
where
    T: Num + Clone + Default + PartialOrd + Send + Sync + TryFrom<f64> + TryFrom<u32>,
{
    pub fn new(coordinator_url: String) -> Self {
        Self {
            coordinator_url,
            x_min: vec![Self::from_f64(-2.0)],
            x_max: vec![Self::from_f64(1.0)],
            y_min: vec![Self::from_f64(-1.5)],
            y_max: vec![Self::from_f64(1.5)],
            max_iters: 1000,
        }
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let parallelism = std::thread::available_parallelism()?.get();
        info!("Worker ready. Parallelism available: {} threads", parallelism);

        let url = self.coordinator_url.clone();
        Self::connect_and_greet(url, self).await?;
        
        Ok(())
    }

    async fn connect_and_greet(url: String, worker: Worker<T>) -> Result<(), Box<dyn std::error::Error>> {
        info!("Connecting to coordinator at {}...", url);
        
        let (ws_stream, _) = connect_async(url).await?;
        let (mut write, mut read) = ws_stream.split();

        let greeting = WorkerMessage::Hello("¡Hello Coordinator!".to_string());
        let json = serde_json::to_string(&greeting)?;
        write.send(Message::Text(json.into())).await?;

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(cmd) = serde_json::from_str::<CoordinatorCommand>(&text) {
                        match cmd {
                            CoordinatorCommand::Welcome(welcome_msg) => {
                                info!("Sent by coordinator: {}", welcome_msg);
                            }
                            CoordinatorCommand::Compute { task_id, width, height, start_row, end_row } => {
                                info!("Starting task {}: rows {} to {} (total {}x{})", task_id, start_row, end_row, width, height);
                                #[cfg(feature = "rayon")]
                                {
                                    let result = worker.compute_block(width, height, start_row, end_row);
                                    
                                    info!("Task {} finished. Sending result ({} rows).", task_id, result.len());
                                    let result_msg = WorkerMessage::ComputeResult { task_id, data: result };
                                    let json = serde_json::to_string(&result_msg)?;
                                    write.send(Message::Text(json.into())).await?;
                                }
                                #[cfg(not(feature = "rayon"))]
                                {
                                    let _ = &worker; // Mark as used
                                    error!("Rayon feature not enabled, cannot compute!");
                                }
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
