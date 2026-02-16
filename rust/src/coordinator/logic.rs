use axum::{
    routing::{get, post},
    Router,
};
use log::{error, info, warn};
use std::collections::{VecDeque, HashMap};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::interval;

use crate::coordinator::types::{CoordinatorCommand, InternalMessage, Task};
use crate::coordinator::handlers::{get_task, submit_result, hello};

pub struct Coordinator {
    pub storage: Vec<Vec<f64>>,
    width: u32,
    height: u32,
    tasks: VecDeque<Task>,
    assigned_tasks: HashMap<u32, (Task, Instant)>,
    started: bool,
    workers_count: usize,
    last_worker_activity: Option<Instant>,
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
            assigned_tasks: HashMap::new(),
            started: false,
            workers_count: 0,
            last_worker_activity: None,
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

        let local_ip = std::net::UdpSocket::bind("0.0.0.0:0")
            .and_then(|socket| {
                socket.connect("8.8.8.8:80")?;
                socket.local_addr()
            })
            .map(|addr| addr.ip().to_string())
            .unwrap_or_else(|_| "127.0.0.1".to_string());

        info!("Coordinator listening on {} (Local IP: {}) with {} tasks pending", addr, local_ip, self.tasks.len());
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

        let mut check_interval = interval(Duration::from_secs(1));

        loop {
            tokio::select! {
                _ = check_interval.tick() => {
                    let now = Instant::now();
                    
                    // Check task timeouts
                    let mut timed_out = Vec::new();
                    for (&id, (_, assigned_at)) in &self.assigned_tasks {
                        if now.duration_since(*assigned_at) > Duration::from_secs(3) {
                            timed_out.push(id);
                        }
                    }

                    for id in timed_out {
                        if let Some((task, _)) = self.assigned_tasks.remove(&id) {
                            warn!("Task {} timed out. Re-queuing task.", id);
                            self.tasks.push_back(task);
                        }
                    }

                    // Check general worker activity
                    if self.started && !self.tasks.is_empty() {
                        if let Some(last_seen) = self.last_worker_activity {
                            if now.duration_since(last_seen) > Duration::from_secs(3) {
                                error!("No hay worker activo y no se pudo concretar la tarea.");
                                
                                // Reset to initial state
                                self.started = false;
                                self.last_worker_activity = None;
                                info!("Waiting for workers to join... Press ENTER to start execution once ready.");
                            }
                        }
                    }
                }
                msg = rx.recv() => {
                    let Some(msg) = msg else { break; };
                    match msg {
                        InternalMessage::StartExecution => {
                            if !self.started {
                                self.started = true;
                                info!("--- STARTING EXECUTION ---");
                                if self.workers_count > 0 {
                                    info!("Active workers: {}", self.workers_count);
                                } else {
                                    warn!("Waiting for workers to join...");
                                }
                                // Empezamos a contar la inactividad desde ahora
                                self.last_worker_activity = Some(Instant::now());
                            }
                        }
                        InternalMessage::GetTask(reply_tx) => {
                            self.last_worker_activity = Some(Instant::now());
                            if self.started {
                                if let Some(task) = self.tasks.pop_front() {
                                    info!("Assigning task {} (rows {} to {})", task.id, task.start_row, task.end_row);
                                    
                                    self.assigned_tasks.insert(task.id, (task.clone(), Instant::now()));

                                    let _ = reply_tx.send(Some(CoordinatorCommand::Compute {
                                        task_id: task.id,
                                        width: self.width,
                                        height: self.height,
                                        start_row: task.start_row,
                                        end_row: task.end_row,
                                    }));
                                } else if self.assigned_tasks.is_empty() {
                                    let _ = reply_tx.send(None);
                                } else {
                                    let _ = reply_tx.send(Some(CoordinatorCommand::Wait));
                                }
                            } else {
                                let _ = reply_tx.send(Some(CoordinatorCommand::Wait));
                            }
                        }
                        InternalMessage::WorkerSaidHello(greeting) => {
                            self.workers_count += 1;
                            self.last_worker_activity = Some(Instant::now());
                            info!("Worker {} joined. Greeting: {}", self.workers_count, greeting);
                        }
                        InternalMessage::WorkerFinished { task_id, data } => {
                            self.last_worker_activity = Some(Instant::now());
                            if self.assigned_tasks.remove(&task_id).is_some() {
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
                            } else {
                                warn!("Received result for task {} but it was not in assigned list.", task_id);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
