use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum CoordinatorCommand {
    Wait,
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
    Hello,
    ComputeResult {
        task_id: u32,
        data: Vec<u32>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct Task {
    pub id: u32,
    pub start_row: u32,
    pub end_row: u32,
}

pub enum InternalMessage {
    WorkerConnected {
        tx: mpsc::UnboundedSender<CoordinatorCommand>,
        alias_tx: oneshot::Sender<String>,
    },
    WorkerDisconnected {
        alias: String,
    },
    WorkerFinished {
        task_id: u32,
        alias: String,
        data: Vec<u32>,
    },
}
