use axum::{
    extract::{State, Json},
    http::StatusCode,
};
use tokio::sync::{mpsc, oneshot};
use crate::coordinator::types::{CoordinatorCommand, InternalMessage, WorkerMessage};

pub async fn hello(
    State(tx): State<mpsc::Sender<InternalMessage>>,
    Json(msg): Json<WorkerMessage>,
) -> StatusCode {
    if let WorkerMessage::Hello(greeting) = msg {
        let _ = tx.send(InternalMessage::WorkerSaidHello(greeting)).await;
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    }
}

pub async fn get_task(
    State(tx): State<mpsc::Sender<InternalMessage>>,
) -> Result<Json<Option<CoordinatorCommand>>, StatusCode> {
    let (reply_tx, reply_rx) = oneshot::channel();
    
    if tx.send(InternalMessage::GetTask(reply_tx)).await.is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    match reply_rx.await {
        Ok(task) => Ok(Json(task)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn submit_result(
    State(tx): State<mpsc::Sender<InternalMessage>>,
    Json(msg): Json<WorkerMessage>,
) -> StatusCode {
    if let WorkerMessage::ComputeResult { task_id, data } = msg {
        if tx.send(InternalMessage::WorkerFinished { task_id, data }).await.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    }
}
