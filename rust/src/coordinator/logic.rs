use axum::{routing::get, Router};
use log::{error, info, warn};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::interval;

use crate::coordinator::handlers::{root_hello, ws_handler};
use crate::coordinator::types::{CoordinatorCommand, InternalMessage, Task};

pub struct Coordinator {
    pub storage: Vec<u32>,
    width: u32,
    height: u32,
    tasks: VecDeque<Task>,
    assigned_tasks: HashMap<u32, (Task, Instant)>,
    // worker_id -> task_id asignado actualmente
    worker_tasks: HashMap<String, u32>,
    // worker_id -> canal para enviarle comandos
    connected_workers: HashMap<String, mpsc::UnboundedSender<CoordinatorCommand>>,
    // workers disponibles sin tarea
    idle_workers: VecDeque<String>,
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
            tasks.push_back(Task { id: task_id, start_row, end_row });
            task_id += 1;
        }

        Self {
            storage: vec![0; (width * height) as usize],
            width,
            height,
            tasks,
            assigned_tasks: HashMap::new(),
            worker_tasks: HashMap::new(),
            connected_workers: HashMap::new(),
            idle_workers: VecDeque::new(),
            started: false,
            workers_count: 0,
        }
    }

    /// Asigna tareas pendientes a workers libres (push-based).
    fn assign_tasks_to_idle_workers(&mut self) {
        while !self.idle_workers.is_empty() && !self.tasks.is_empty() {
            let worker_id = self.idle_workers.pop_front().unwrap();
            let task = self.tasks.pop_front().unwrap();

            let sent = if let Some(tx) = self.connected_workers.get(&worker_id) {
                let cmd = CoordinatorCommand::Compute {
                    task_id: task.id,
                    width: self.width,
                    height: self.height,
                    start_row: task.start_row,
                    end_row: task.end_row,
                };
                tx.send(cmd).is_ok()
            } else {
                false
            };

            if sent {
                info!(
                    "Task {} (rows {}-{}) → worker {}",
                    task.id, task.start_row, task.end_row, worker_id
                );
                self.worker_tasks.insert(worker_id, task.id);
                self.assigned_tasks.insert(task.id, (task, Instant::now()));
            } else {
                // Canal cerrado: descartamos el worker y devolvemos la tarea
                self.connected_workers.remove(&worker_id);
                self.tasks.push_front(task);
            }
        }
    }

    /// Detecta tareas que excedieron el timeout y las reencola.
    fn check_and_requeue_timed_out_tasks(&mut self, now: Instant) {
        let mut timed_out = Vec::new();
        for (&id, (_, assigned_at)) in &self.assigned_tasks {
            if now.duration_since(*assigned_at) > Duration::from_secs(3) {
                timed_out.push(id);
            }
        }

        for id in timed_out {
            if let Some((task, _)) = self.assigned_tasks.remove(&id) {
                warn!("Task {} timed out. Re-queuing.", id);
                let stale_worker = self.worker_tasks.iter()
                    .find(|(_, &v)| v == id)
                    .map(|(k, _)| k.clone());
                if let Some(w) = stale_worker {
                    self.worker_tasks.remove(&w);
                    self.idle_workers.push_back(w);
                }
                self.tasks.push_back(task);
            }
        }

        if self.started {
            self.assign_tasks_to_idle_workers();
        }
    }

    /// Procesa un mensaje interno y actualiza el estado del coordinador.
    fn handle_internal_message(&mut self, msg: InternalMessage) {
        match msg {
            InternalMessage::StartExecution => {
                if !self.started {
                    self.started = true;
                    info!("--- STARTING EXECUTION --- ({} workers connected)", self.workers_count);
                    self.assign_tasks_to_idle_workers();
                }
            }

            InternalMessage::WorkerConnected { worker_id, tx: worker_tx } => {
                self.workers_count += 1;
                info!("Worker '{}' connected (total: {})", worker_id, self.workers_count);
                self.connected_workers.insert(worker_id.clone(), worker_tx);
                self.idle_workers.push_back(worker_id);
                if self.started {
                    self.assign_tasks_to_idle_workers();
                }
            }

            InternalMessage::WorkerDisconnected { worker_id } => {
                self.workers_count = self.workers_count.saturating_sub(1);
                warn!("Worker '{}' disconnected (total: {})", worker_id, self.workers_count);
                self.connected_workers.remove(&worker_id);
                self.idle_workers.retain(|id| id != &worker_id);

                if let Some(task_id) = self.worker_tasks.remove(&worker_id) {
                    if let Some((task, _)) = self.assigned_tasks.remove(&task_id) {
                        warn!("Re-queuing task {} from disconnected worker '{}'", task_id, worker_id);
                        self.tasks.push_back(task);
                    }
                }
            }

            InternalMessage::WorkerFinished { task_id, worker_id, data } => {
                if let Some((task, _)) = self.assigned_tasks.remove(&task_id) {
                    info!("Task {} done by worker '{}'. Merging data...", task_id, worker_id);
                    self.worker_tasks.remove(&worker_id);

                    let start_idx = (task.start_row * self.width) as usize;
                    let len = data.len();
                    if start_idx + len <= self.storage.len() {
                        self.storage[start_idx..start_idx + len].copy_from_slice(&data);
                    } else {
                        warn!("Task {} result exceeds storage bounds.", task_id);
                    }

                    if self.tasks.is_empty() && self.assigned_tasks.is_empty() {
                        info!("All tasks completed!");
                    } else {
                        self.idle_workers.push_back(worker_id);
                        self.assign_tasks_to_idle_workers();
                    }
                } else {
                    warn!("Received result for unknown task {}.", task_id);
                }
            }
        }
    }

    /// Inicia el servidor HTTP y listener para conexiones de workers.
    async fn start_server(&self, bind_addr: &str, port: u16, tx: &mpsc::Sender<InternalMessage>) -> Result<(), Box<dyn std::error::Error>> {
        let app = Router::new()
            .route("/", get(root_hello))
            .route("/ws", get(ws_handler))
            .with_state(tx.clone());

        let addr: SocketAddr = format!("{}:{}", bind_addr, port).parse()?;
        let listener = tokio::net::TcpListener::bind(addr).await?;

        let local_ip = std::net::UdpSocket::bind("0.0.0.0:0")
            .and_then(|s| { s.connect("8.8.8.8:80")?; s.local_addr() })
            .map(|a| a.ip().to_string())
            .unwrap_or_else(|_| "127.0.0.1".to_string());

        info!(
            "Coordinator listening on {} (Local IP: {}) with {} tasks pending",
            addr, local_ip, self.tasks.len()
        );
        info!("Waiting for workers to join... Execution will start automatically after 10 seconds without new connections.");

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!("Axum server error: {}", e);
            }
        });

        Ok(())
    }

    pub async fn run(mut self, bind_addr: &str, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, mut rx) = mpsc::channel::<InternalMessage>(100);

        self.start_server(bind_addr, port, &tx).await?;

        let mut check_interval = interval(Duration::from_secs(1));
        let mut last_worker_connection = Instant::now();
        let worker_registration_timeout = Duration::from_secs(10);

        loop {
            tokio::select! {
                _ = check_interval.tick() => {
                    self.check_and_requeue_timed_out_tasks(Instant::now());

                    // Auto-start if 10 seconds pass without new worker connections
                    if !self.started {
                        if Instant::now().duration_since(last_worker_connection) >= worker_registration_timeout {
                            self.started = true;
                            info!(
                                "--- STARTING EXECUTION (auto-start timeout) --- ({} workers connected)",
                                self.workers_count
                            );
                            self.assign_tasks_to_idle_workers();
                            last_worker_connection = Instant::now();
                        }
                    }
                }

                msg = rx.recv() => {
                    let Some(msg) = msg else { break Ok(()); };

                    // Reset auto-start timer when a worker connects
                    if matches!(msg, InternalMessage::WorkerConnected { .. }) {
                        last_worker_connection = Instant::now();
                    }

                    self.handle_internal_message(msg);
                }
            }
        }
    }
}
