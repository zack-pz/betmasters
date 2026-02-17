use crate::coordinator::{CoordinatorCommand, WorkerMessage};
use log::{error, info};
use num_traits::Num;
use std::time::Duration;
use tokio::time::sleep;

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

    pub async fn run(self, bind_addr: &str, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        let parallelism = std::thread::available_parallelism()?.get();
        
        let addr: std::net::SocketAddr = format!("{}:{}", bind_addr, port).parse()?;
        let listener = tokio::net::TcpListener::bind(addr).await?;

        info!("Worker listening on {}. Parallelism available: {} threads", addr, parallelism);

        // Simple health check for the worker
        let app = axum::Router::new().route("/", axum::routing::get(|| async { "Worker is running" }));
        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!("Worker server error: {}", e);
            }
        });

        let client = reqwest::Client::new();
        let base_url = self.coordinator_url.trim_end_matches('/');

        // Greet
        let greeting = WorkerMessage::Hello(format!("¡Hello Coordinator from {}!", addr));
        if let Err(e) = client.post(format!("{}/hello", base_url))
            .json(&greeting)
            .send()
            .await {
            error!("Failed to greet coordinator: {}", e);
            return Err(e.into());
        }

        loop {
            // Request task
            let resp = client.get(format!("{}/get_task", base_url))
                .send()
                .await;

            match resp {
                Ok(response) => {
                    if response.status().is_success() {
                        let task_opt: Option<CoordinatorCommand> = response.json().await?;
                        
                        if let Some(cmd) = task_opt {
                            match cmd {
                                CoordinatorCommand::Welcome(msg) => {
                                    info!("Coordinator welcomed us: {}", msg);
                                }
                                CoordinatorCommand::Wait => {
                                    // info!("Waiting for coordinator to start execution...");
                                    sleep(Duration::from_secs(1)).await;
                                }
                                CoordinatorCommand::Compute { task_id, width, height, start_row, end_row } => {
                                    info!("Starting task {}: rows {} to {} (total {}x{})", task_id, start_row, end_row, width, height);
                                    
                                    let result = self.compute_block(width, height, start_row, end_row);
                                    
                                    info!("Task {} finished. Sending result ({} rows).", task_id, result.len());
                                    let result_msg = WorkerMessage::ComputeResult { task_id, data: result };
                                    
                                    if let Err(e) = client.post(format!("{}/submit_result", base_url))
                                        .json(&result_msg)
                                        .send()
                                        .await {
                                        error!("Failed to submit result for task {}: {}", task_id, e);
                                    }
                                }
                            }
                        } else {
                            info!("No more tasks available. Worker shutting down.");
                            break;
                        }
                    } else {
                        error!("Coordinator returned error status: {}", response.status());
                        sleep(Duration::from_secs(2)).await;
                    }
                }
                Err(e) => {
                    error!("Connection error: {}", e);
                    sleep(Duration::from_secs(5)).await;
                }
            }
        }
        
        Ok(())
    }
}
