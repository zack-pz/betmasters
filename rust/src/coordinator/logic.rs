use axum::{
    extract::ws::WebSocketUpgrade,
    routing::get,
    Router,
};
use log::{error, info};
use std::collections::VecDeque;
use std::net::SocketAddr;
use tokio::sync::mpsc;

use crate::coordinator::types::{CoordinatorCommand, InternalMessage, Task};
use crate::coordinator::handlers::handle_socket;

pub struct Coordinator {
    pub storage: Vec<Vec<f64>>,
    width: u32,
    height: u32,
    tasks: VecDeque<Task>,
}

impl Coordinator {
    pub fn new(width: u32, height: u32) -> Self {
        let block_size = 10;
        let mut tasks = VecDeque::new();
        let mut task_id = 0;

        for start_row in (0..height).step_by(block_size as usize) {
            let end_row = (start_row + block_size).min(height);
            tasks.push_back(Task {
                id: task_id,
                start_row,
                end_row,
            });
            task_id += 1;
        }

        Self {
            storage: vec![vec![0.0; height as usize]; width as usize],
            width,
            height,
            tasks,
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

        info!("Coordinator listening on {} with {} tasks pending", addr, self.tasks.len());

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!("Axum server error: {}", e);
            }
        });

        while let Some(msg) = rx.recv().await {
            match msg {
                InternalMessage::WorkerConnected(worker_tx) => {
                    info!("New worker connected.");
                    if let Some(task) = self.tasks.pop_front() {
                        info!("Assigning task {} (rows {} to {})", task.id, task.start_row, task.end_row);
                        let _ = worker_tx.send(CoordinatorCommand::Compute {
                            task_id: task.id,
                            width: self.width,
                            height: self.height,
                            start_row: task.start_row,
                            end_row: task.end_row,
                        }).await;
                    }
                }
                InternalMessage::WorkerSaidHello(greeting) => {
                    info!("Worker says: {}", greeting);
                }
                InternalMessage::WorkerFinished { task_id, data, worker_tx } => {
                    let start_row = (task_id * 10) as usize;
                    
                    info!("Received result for task {}. Merging data...", task_id);
                    
                    for (i, row_data) in data.into_iter().enumerate() {
                        let target_row = start_row + i;
                        if target_row < self.height as usize {
                            for (col, &val) in row_data.iter().enumerate() {
                                if col < self.width as usize {
                                    self.storage[col][target_row] = val;
                                }
                            }
                        }
                    }

                    if let Some(task) = self.tasks.pop_front() {
                        info!("Assigning next task {} to worker", task.id);
                        let _ = worker_tx.send(CoordinatorCommand::Compute {
                            task_id: task.id,
                            width: self.width,
                            height: self.height,
                            start_row: task.start_row,
                            end_row: task.end_row,
                        }).await;
                    } else {
                        info!("No more tasks available for this worker.");
                    }
                }
            }
        }

        Ok(())
    }
}
