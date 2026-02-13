use crate::coordinator::{CoordinatorCommand, WorkerMessage};
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use num_complex::Complex;
use num_traits::Num;

#[cfg(feature = "rayon")]
use rayon::prelude::*;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

// Aquí puedes agregar submódulos futuros en la carpeta worker/
// pub mod submodule;

pub struct Worker<T> {
    coordinator_url: String,
    pub x_min: Vec<T>,
    pub x_max: Vec<T>,
    pub y_min: Vec<T>,
    pub y_max: Vec<T>,
    pub max_iters: usize,
}

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

    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        let parallelism = std::thread::available_parallelism()?.get();
        info!("Worker ready. Parallelism available: {} threads", parallelism);

        let url = self.coordinator_url.clone();
        Self::connect_and_greet(url, &mut self).await?;
        
        Ok(())
    }

    async fn connect_and_greet(url: String, worker: &mut Worker<T>) -> Result<(), Box<dyn std::error::Error>> {
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

    fn from_f64(v: f64) -> T {
        T::try_from(v)
            .ok()
            .ok_or("conversion failed")
            .expect("Failed to convert from f64")
    }

    fn from_u32(v: u32) -> T {
        T::try_from(v)
            .ok()
            .ok_or("conversion failed")
            .expect("Failed to convert from u32")
    }

    fn remap(value: T, start1: T, stop1: T, start2: T, stop2: T) -> T {
        start2.clone()
            + (value - start1.clone()) * (stop2 - start2) / (stop1 - start1)
    }

    #[cfg(feature = "rayon")]
    pub fn compute_block(&self, width: u32, height: u32, start_row: u32, end_row: u32) -> Vec<Vec<f64>> {
        // We compute row by row for the given range
        (start_row..end_row)
            .into_par_iter()
            .map(|v| {
                let y: T = Self::remap(
                    Self::from_u32(v),
                    Self::from_f64(0.0),
                    Self::from_u32(height),
                    self.y_min.last().unwrap().clone(),
                    self.y_max.last().unwrap().clone(),
                );
                (0..width)
                    .map(|u| {
                        let x: T = Self::remap(
                            Self::from_u32(u),
                            Self::from_f64(0.0),
                            Self::from_u32(width),
                            self.x_min.last().unwrap().clone(),
                            self.x_max.last().unwrap().clone(),
                        );
                        Self::evaluate(x.clone(), y.clone(), self.max_iters)
                    })
                    .collect()
            })
            .collect()
    }

    fn evaluate(x: T, y: T, max_iters: usize) -> f64 {
        let q = (x.clone() - Self::from_f64(0.25)) * (x.clone() - Self::from_f64(0.25)) + y.clone() * y.clone();
        if q.clone() * (q + (x.clone() - Self::from_f64(0.25))) <= y.clone() * y.clone() * Self::from_f64(0.25) {
            return max_iters as f64;
        }
        let mut z = Complex::<T>::default();
        let mut z_old = Complex::<T>::default();
        let c = Complex::<T>::new(x, y);
        for i in 0..max_iters {
            if z.norm_sqr() >= Self::from_f64(4.0) {
                return i as f64 - 1.0;
            }
            z = z.clone() * z + c.clone();
            if z == z_old {
                return max_iters as f64;
            }
            if i % 20 == 0 {
                z_old = z.clone();
            }
        }
        max_iters as f64
    }
}
