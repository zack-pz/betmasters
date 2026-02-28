use crate::worker::types::Worker;
use std::time::Duration;
use reqwest::Client;
use crate::coordinator::types::{CoordinatorCommand, WorkerMessage};

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
        let client = Client::new();
        println!("Worker started, connecting to {}", self.coordinator_url);

        loop {
            // Solicitar trabajo al coordinador
            let res = client.get(format!("{}/work", self.coordinator_url))
                .send()
                .await;

            match res {
                Ok(response) => {
                    if response.status().is_success() {
                        let command: CoordinatorCommand = response.json().await?;
                        
                        match command {
                            CoordinatorCommand::Compute { task_id, width, height, start_row, end_row } => {
                                // Realizar el cálculo
                                let data = self.compute_block(width, height, start_row, end_row);
                                
                                // Enviar resultados
                                let result = WorkerMessage::ComputeResult {
                                    task_id,
                                    data,
                                };

                                let _ = client.post(format!("{}/results", self.coordinator_url))
                                    .json(&result)
                                    .send()
                                    .await;
                            }
                            _ => {
                                // Si no hay trabajo, esperamos un poco
                                tokio::time::sleep(Duration::from_secs(1)).await;
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error connecting to coordinator: {}. Retrying in 2s...", e);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
            
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
