use axum::{routing::get, Router};
use log::{error, info, warn};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::spawn_blocking;
use tokio::time::interval;

use crate::coordinator::handlers::{root_hello, ws_handler};
use crate::coordinator::types::{CoordinatorCommand, InternalMessage, Task, WorkerState};

pub struct Coordinator {
    pub storage: Vec<u32>,
    width: u32,
    height: u32,
    max_iters: u32,
    tasks: VecDeque<Task>,
    assigned_tasks: HashMap<u32, Task>,
    workers: HashMap<String, WorkerState>,
    idle_workers: VecDeque<String>,
    started: bool,
    next_worker_num: usize,
}

impl Coordinator {
    pub fn new(width: u32, height: u32, block_size: u32, max_iters: u32) -> Self {
        let mut tasks = VecDeque::new();

        for (task_id, start_row) in (0..height).step_by(block_size as usize).enumerate() {
            let end_row = (start_row + block_size).min(height);
            tasks.push_back(Task { id: task_id as u32, start_row, end_row });
        }

        Self {
            storage: vec![0; (width * height) as usize],
            width,
            height,
            max_iters,
            tasks,
            assigned_tasks: HashMap::new(),
            workers: HashMap::new(),
            idle_workers: VecDeque::new(),
            started: false,
            next_worker_num: 0,
        }
    }

    /// Asigna tareas pendientes a workers libres (push-based).
    fn assign_tasks_to_idle_workers(&mut self) {
        while !self.idle_workers.is_empty() && !self.tasks.is_empty() {
            let alias = self.idle_workers.pop_front().unwrap();
            let task = self.tasks.pop_front().unwrap();

            let sent = if let Some(state) = self.workers.get(&alias) {
                let cmd = CoordinatorCommand::Compute {
                    task_id: task.id,
                    width: self.width,
                    height: self.height,
                    start_row: task.start_row,
                    end_row: task.end_row,
                };
                state.tx.send(cmd).is_ok()
            } else {
                false
            };

            if sent {
                info!(
                    "Task {} (rows {}-{}) → worker {}",
                    task.id, task.start_row, task.end_row, alias
                );
                if let Some(state) = self.workers.get_mut(&alias) {
                    state.current_task_id = Some(task.id);
                    state.assigned_at = Some(Instant::now());
                }
                self.assigned_tasks.insert(task.id, task);
            } else {
                // Canal cerrado: descartamos el worker y devolvemos la tarea
                self.workers.remove(&alias);
                self.tasks.push_front(task);
            }
        }
    }

    /// Detecta tareas que excedieron el timeout y las reencola.
    fn check_and_requeue_timed_out_tasks(&mut self, now: Instant) {
        let timed_out: Vec<(String, u32)> = self.workers.iter()
            .filter_map(|(alias, state)| {
                let task_id = state.current_task_id?;
                let assigned_at = state.assigned_at?;
                if now.duration_since(assigned_at) > Duration::from_secs(3) {
                    Some((alias.clone(), task_id))
                } else {
                    None
                }
            })
            .collect();

        for (alias, task_id) in timed_out {
            if let Some(task) = self.assigned_tasks.remove(&task_id) {
                warn!("Task {} timed out. Re-queuing.", task_id);
                if let Some(state) = self.workers.get_mut(&alias) {
                    state.current_task_id = None;
                    state.assigned_at = None;
                }
                self.idle_workers.push_back(alias);
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
            InternalMessage::WorkerConnected { tx: worker_tx, alias_tx } => {
                self.next_worker_num += 1;
                let alias = format!("worker-{}", self.next_worker_num);
                self.workers.insert(alias.clone(), WorkerState {
                    tx: worker_tx,
                    current_task_id: None,
                    assigned_at: None,
                });
                self.idle_workers.push_back(alias.clone());
                info!("Worker '{}' connected (total: {})", alias, self.workers.len());
                let _ = alias_tx.send(alias);
                if self.started {
                    self.assign_tasks_to_idle_workers();
                }
            }

            InternalMessage::WorkerDisconnected { alias } => {
                let current_task_id = self.workers.get(&alias).and_then(|s| s.current_task_id);
                self.workers.remove(&alias);
                self.idle_workers.retain(|id| id != &alias);
                warn!("Worker '{}' disconnected (total: {})", alias, self.workers.len());

                if let Some(task_id) = current_task_id {
                    if let Some(task) = self.assigned_tasks.remove(&task_id) {
                        warn!("Re-queuing task {} from disconnected worker '{}'", task_id, alias);
                        self.tasks.push_back(task);
                    }
                }
            }

            InternalMessage::WorkerFinished { task_id, alias, data } => {
                if let Some(task) = self.assigned_tasks.remove(&task_id) {
                    info!("Task {} done by worker '{}'. Merging data...", task_id, alias);
                    if let Some(state) = self.workers.get_mut(&alias) {
                        state.current_task_id = None;
                        state.assigned_at = None;
                    }

                    let start_idx = (task.start_row * self.width) as usize;
                    let len = data.len();
                    if start_idx + len <= self.storage.len() {
                        self.storage[start_idx..start_idx + len].copy_from_slice(&data);
                    } else {
                        warn!("Task {} result exceeds storage bounds.", task_id);
                    }

                    if self.tasks.is_empty() && self.assigned_tasks.is_empty() {
                        info!("All tasks completed!");
                        let storage = self.storage.clone();
                        let width = self.width;
                        let height = self.height;
                        let max_iters = self.max_iters;
                        spawn_blocking(move || {
                            save_image_impl(width, height, max_iters, storage);
                        });
                    } else {
                        self.idle_workers.push_back(alias);
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
                    if !self.started
                        && Instant::now().duration_since(last_worker_connection) >= worker_registration_timeout {
                        self.started = true;
                        info!(
                            "--- STARTING EXECUTION (auto-start timeout) --- ({} workers connected)",
                            self.workers.len()
                        );
                        self.assign_tasks_to_idle_workers();
                        last_worker_connection = Instant::now();
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

/// Polinomios de Bernstein para mapeo de iteraciones a colores.
/// Basados en un gradiente suave que va de colores brillantes (rápida divergencia)
/// a negro (puntos dentro del conjunto de Mandelbrot).
const COLOR_COEFF_RED: f64 = 9.0;
const COLOR_COEFF_GREEN: f64 = 15.0;
const COLOR_COEFF_BLUE: f64 = 8.5;
const COLOR_MAX_INTENSITY: f64 = 255.0;

/// Mapea un conteo de iteraciones a un color RGB usando polinomios de Bernstein.
/// Puntos dentro del conjunto (iter == max_iters) se pintan de negro.
fn iter_to_color(iter: u32, max_iters: u32) -> image::Rgb<u8> {
    if iter == max_iters {
        return image::Rgb([0, 0, 0]);
    }
    let t = iter as f64 / max_iters as f64;
    let r = (COLOR_COEFF_RED * (1.0 - t) * t * t * t * COLOR_MAX_INTENSITY) as u8;
    let g = (COLOR_COEFF_GREEN * (1.0 - t) * (1.0 - t) * t * t * COLOR_MAX_INTENSITY) as u8;
    let b = (COLOR_COEFF_BLUE * (1.0 - t) * (1.0 - t) * (1.0 - t) * t * COLOR_MAX_INTENSITY) as u8;
    image::Rgb([r, g, b])
}

/// Convierte los datos de iteraciones almacenados en una imagen PNG y la guarda en disco.
/// Esta función está diseñada para ejecutarse en un thread pool separado via spawn_blocking()
/// para no bloquear el Tokio event loop durante operaciones CPU/IO intensivas.
fn save_image_impl(width: u32, height: u32, max_iters: u32, storage: Vec<u32>) {
    let path = std::env::var("OUTPUT_FILE").unwrap_or_else(|_| "fractal.png".to_string());

    let img = image::ImageBuffer::from_fn(width, height, |x, y| {
        let idx = (y * width + x) as usize;
        iter_to_color(storage[idx], max_iters)
    });

    match img.save(&path) {
        Ok(_) => info!("Fractal image saved to '{}'", path),
        Err(e) => error!("Failed to save fractal image: {}", e),
    }
}
