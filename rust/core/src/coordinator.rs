use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    routing::get,
    Router,
};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::net::SocketAddr;
use tokio::sync::mpsc;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum CoordinatorCommand {
    Welcome(String),
    Compute {
        task_id: u32,
        width: u32,
        height: u32,
        start_row: u32,
        end_row: u32,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerMessage {
    Hello(String),
    ComputeResult {
        task_id: u32,
        data: Vec<Vec<f64>>,
    },
}

#[derive(Debug, Clone)]
struct Task {
    id: u32,
    start_row: u32,
    end_row: u32,
}

pub enum InternalMessage {
    WorkerConnected(mpsc::Sender<CoordinatorCommand>),
    WorkerSaidHello(String),
    WorkerFinished {
        task_id: u32,
        data: Vec<Vec<f64>>,
        worker_tx: mpsc::Sender<CoordinatorCommand>,
    },
}

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
            Some(cmd) = worker_rx.recv() => {
                let json = serde_json::to_string(&cmd).unwrap();
                if socket.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
            Some(result) = socket.recv() => {
                match result {
                    Ok(Message::Text(text)) => {
                        if let Ok(msg) = serde_json::from_str::<WorkerMessage>(&text) {
                            match msg {
                                WorkerMessage::Hello(greeting) => {
                                    let _ = tx.send(InternalMessage::WorkerSaidHello(greeting)).await;
                                }
                                WorkerMessage::ComputeResult { task_id, data } => {
                                    let _ = tx.send(InternalMessage::WorkerFinished { 
                                        task_id, 
                                        data, 
                                        worker_tx: worker_tx.clone() 
                                    }).await;
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
