use crate::coordinator::{CoordinatorCommand, WorkerMessage};
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use num_complex::Complex;
use num_traits::Num;

#[cfg(feature = "rayon")]
use rayon::prelude::*;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

pub struct Worker<T> {
    coordinator_url: String,
    pub set_storage: Vec<Vec<f64>>,
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
            set_storage: Vec::new(),
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

        // Send handshake to coordinator
        let greeting = WorkerMessage::Hello("¡Hello Coordinator!".to_string());
        let json = serde_json::to_string(&greeting)?;
        write.send(Message::Text(json.into())).await?;

        // Worker awaits coordinators commands
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(cmd) = serde_json::from_str::<CoordinatorCommand>(&text) {
                        match cmd {
                            CoordinatorCommand::Welcome(welcome_msg) => {
                                info!("Sent by coordinator: {}", welcome_msg);
                            }
                            CoordinatorCommand::Compute { width, height } => {
                                info!("Starting computation: {}x{}", width, height);
                                #[cfg(feature = "rayon")]
                                {
                                    worker.update_set(width, height);
                                    
                                    // Calculate average value as a simple result to show
                                    let sum: f64 = worker.set_storage.iter().flat_map(|row| row.iter()).sum();
                                    let count = (width * height) as f64;
                                    let avg = sum / count;
                                    
                                    info!("Computation finished. Sending result: {}", avg);
                                    let result_msg = WorkerMessage::ComputeResult(avg);
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
    pub fn update_set(&mut self, width: u32, height: u32) {
        self.set_storage = (0..width)
            .into_par_iter()
            .map(|u| {
                let x: T = Self::remap(
                    Self::from_u32(u),
                    Self::from_f64(0.0),
                    Self::from_u32(width),
                    self.x_min.last().unwrap().clone(),
                    self.x_max.last().unwrap().clone(),
                );
                (0..height)
                    .map(|v| {
                        let y: T = Self::remap(
                            Self::from_u32(v),
                            Self::from_f64(0.0),
                            Self::from_u32(height),
                            self.y_min.last().unwrap().clone(),
                            self.y_max.last().unwrap().clone(),
                        );
                        Self::evaluate(x.clone(), y, self.max_iters)
                    })
                    .collect()
            })
            .collect();
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
