use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::sync::mpsc;
use log::{info, debug, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub max_iterations: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerMessage {
    Ready,
    Progress(f32), // 0.0 to 1.0
    Result(Task, Vec<u8>),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CoordinatorCommand {
    AssignTask(Task),
    Finish,
}

pub enum InternalMessage {
    WorkerConnected(mpsc::Sender<CoordinatorCommand>),
    WorkerRequestTask(mpsc::Sender<CoordinatorCommand>),
    WorkerReportProgress(f32),
    WorkerSubmitResult(Task, Vec<u8>),
}

pub struct Coordinator {
    width: u32,
    height: u32,
    max_iterations: u32,
    tasks: Vec<Task>,
    image_buffer: Vec<u8>, // RGBA Buffer
}

impl Coordinator {
    pub fn new(width: u32, height: u32, max_iterations: u32) -> Self {
        let mut tasks = Vec::new();
        let block_size = 64;

        for y in (0..height).step_by(block_size as usize) {
            for x in (0..width).step_by(block_size as usize) {
                let w = if x + block_size > width { width - x } else { block_size };
                let h = if y + block_size > height { height - y } else { block_size };
                tasks.push(Task {
                    x,
                    y,
                    width: w,
                    height: h,
                    max_iterations,
                });
            }
        }

        info!("Coordinator initialized with {} tasks for {}x{}", tasks.len(), width, height);

        Coordinator {
            width,
            height,
            max_iterations,
            tasks,
            image_buffer: vec![0; (width * height * 4) as usize],
        }
    }

    pub async fn run(mut self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, mut rx) = mpsc::channel::<InternalMessage>(100);
        let app_state = tx.clone();

        let app = Router::new()
            .route("/ws", get(move |ws: WebSocketUpgrade| {
                let state = app_state.clone();
                async move {
                    ws.on_upgrade(move |socket| handle_socket(socket, state))
                }
            }));

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = tokio::net::TcpListener::bind(addr).await?;
        
        info!("Coordinator Axum server listening on {}", addr);
        
        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!("Axum server error: {}", e);
            }
        });

        // Bucle principal (Dueño del estado)
        while let Some(msg) = rx.recv().await {
            match msg {
                InternalMessage::WorkerConnected(_worker_tx) => {
                    info!("New worker connected via WebSocket");
                }
                InternalMessage::WorkerRequestTask(worker_tx) => {
                    if let Some(task) = self.tasks.pop() {
                        let _ = worker_tx.send(CoordinatorCommand::AssignTask(task)).await;
                    } else {
                        let _ = worker_tx.send(CoordinatorCommand::Finish).await;
                    }
                }
                InternalMessage::WorkerReportProgress(p) => {
                    debug!("Worker progress: {:.2}%", p * 100.0);
                }
                InternalMessage::WorkerSubmitResult(task, data) => {
                    self.process_result(task, data);
                }
            }
        }

        Ok(())
    }

    fn process_result(&mut self, task: Task, data: Vec<u8>) {
        let row_size = (task.width * 4) as usize;
        let global_width = (self.width * 4) as usize;
        
        for i in 0..task.height {
            let src_start = (i * task.width * 4) as usize;
            let dest_y = task.y + i;
            let dest_start = (dest_y as usize * global_width) + (task.x as usize * 4);

            if dest_start + row_size <= self.image_buffer.len() && src_start + row_size <= data.len() {
                self.image_buffer[dest_start..dest_start + row_size]
                    .copy_from_slice(&data[src_start..src_start + row_size]);
            }
        }
    }

    pub fn get_final_buffer(&self) -> Vec<u8> {
        self.image_buffer.clone()
    }
}

async fn handle_socket(mut socket: WebSocket, tx: mpsc::Sender<InternalMessage>) {
    let (worker_tx, mut worker_rx) = mpsc::channel::<CoordinatorCommand>(10);
    if tx.send(InternalMessage::WorkerConnected(worker_tx.clone())).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            // Desde el Coordinador al Worker
            Some(cmd) = worker_rx.recv() => {
                let msg = serde_json::to_string(&cmd).unwrap();
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            // Desde el Worker al Coordinador
            Some(result) = socket.recv() => {
                match result {
                    Ok(Message::Text(text)) => {
                        if let Ok(msg) = serde_json::from_str::<WorkerMessage>(&text) {
                            match msg {
                                WorkerMessage::Ready => {
                                    let _ = tx.send(InternalMessage::WorkerRequestTask(worker_tx.clone())).await;
                                }
                                WorkerMessage::Progress(p) => {
                                    let _ = tx.send(InternalMessage::WorkerReportProgress(p)).await;
                                }
                                WorkerMessage::Result(task, data) => {
                                    let _ = tx.send(InternalMessage::WorkerSubmitResult(task, data)).await;
                                    let _ = tx.send(InternalMessage::WorkerRequestTask(worker_tx.clone())).await;
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        }
    }
}