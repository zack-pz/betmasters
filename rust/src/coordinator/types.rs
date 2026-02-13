use serde::{Deserialize, Serialize};
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
pub(crate) struct Task {
    pub id: u32,
    pub start_row: u32,
    pub end_row: u32,
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
