use axum::extract::ws::{Message, WebSocket};
use tokio::sync::mpsc;
use crate::coordinator::types::{CoordinatorCommand, InternalMessage, WorkerMessage};

pub(crate) async fn handle_socket(mut socket: WebSocket, tx: mpsc::Sender<InternalMessage>) {
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
