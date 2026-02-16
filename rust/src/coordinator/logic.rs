use axum::{
    routing::{get, post},
    Router,
};
use log::{error, info};
use std::collections::VecDeque;
use std::net::SocketAddr;
use tokio::sync::mpsc;

use crate::coordinator::types::{CoordinatorCommand, InternalMessage, Task};
use crate::coordinator::handlers::{get_task, submit_result, hello};

pub struct Coordinator {
    pub storage: Vec<Vec<f64>>,
    width: u32,
    height: u32,
    tasks: VecDeque<Task>,
    started: bool,
    workers_count: usize,
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
            started: false,
            workers_count: 0,
        }
    }

    pub async fn run(mut self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, mut rx) = mpsc::channel::<InternalMessage>(100);
        let app_state = tx.clone();

        let app = Router::new()
            .route("/hello", post(hello))
            .route("/get_task", get(get_task))
            .route("/submit_result", post(submit_result))
            .with_state(app_state);

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = tokio::net::TcpListener::bind(addr).await?;

        info!("Coordinator listening on {} with {} tasks pending", addr, self.tasks.len());
        info!("Waiting for workers to join... Press ENTER to start execution once ready.");

        let stdin_tx = tx.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
            let mut line = String::new();
            let _ = reader.read_line(&mut line).await;
            let _ = stdin_tx.send(InternalMessage::StartExecution).await;
        });

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!("Axum server error: {}", e);
            }
        });

        while let Some(msg) = rx.recv().await {
            match msg {
                InternalMessage::StartExecution => {
                    if !self.started {
                        self.started = true;
                        info!("--- STARTING EXECUTION WITH {} WORKERS ---", self.workers_count);
                    }
                }
                InternalMessage::GetTask(reply_tx) => {
                    if self.started {
                        if let Some(task) = self.tasks.pop_front() {
                            info!("Assigning task {} (rows {} to {})", task.id, task.start_row, task.end_row);
                            let _ = reply_tx.send(Some(CoordinatorCommand::Compute {
                                task_id: task.id,
                                width: self.width,
                                height: self.height,
                                start_row: task.start_row,
                                end_row: task.end_row,
                            }));
                        } else {
                            // All tasks finished
                            let _ = reply_tx.send(None);
                        }
                    } else {
                        // Execution not started yet, tell worker to wait
                        let _ = reply_tx.send(Some(CoordinatorCommand::Wait));
                    }
                }
                InternalMessage::WorkerSaidHello(greeting) => {
                    self.workers_count += 1;
                    info!("Worker {} joined. Greeting: {}", self.workers_count, greeting);
                }
                InternalMessage::WorkerFinished { task_id, data } => {
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
                }
            }
        }

        Ok(())
    }
}
