use axum::{
    extract::{ws::{Message, WebSocket}, State, WebSocketUpgrade},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::coordinator::types::{CoordinatorCommand, InternalMessage, WorkerMessage};

pub async fn root_hello() -> &'static str {
    "Hello from Coordinator!"
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(tx): State<mpsc::Sender<InternalMessage>>,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, tx))
}

async fn handle_socket(socket: WebSocket, tx: mpsc::Sender<InternalMessage>) {
    let (mut sender, mut receiver) = socket.split();
    let (worker_cmd_tx, mut worker_cmd_rx) = mpsc::unbounded_channel::<CoordinatorCommand>();

    // El primer mensaje debe ser Hello con el ID del worker
    let worker_id = match receiver.next().await {
        Some(Ok(Message::Text(text))) => {
            match serde_json::from_str::<WorkerMessage>(&text) {
                Ok(WorkerMessage::Hello(id)) => {
                    let _ = tx
                        .send(InternalMessage::WorkerConnected {
                            worker_id: id.clone(),
                            tx: worker_cmd_tx,
                        })
                        .await;
                    id
                }
                _ => return,
            }
        }
        _ => return,
    };

    // Tarea para reenviar comandos del coordinator al worker via WS
    let send_task = tokio::spawn(async move {
        while let Some(cmd) = worker_cmd_rx.recv().await {
            let text = match serde_json::to_string(&cmd) {
                Ok(t) => t,
                Err(_) => break,
            };
            if sender.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    // Recibir resultados del worker
    while let Some(Ok(Message::Text(text))) = receiver.next().await {
        if let Ok(WorkerMessage::ComputeResult { task_id, data }) =
            serde_json::from_str::<WorkerMessage>(&text)
        {
            let _ = tx
                .send(InternalMessage::WorkerFinished {
                    task_id,
                    worker_id: worker_id.clone(),
                    data,
                })
                .await;
        }
    }

    // Worker desconectado
    let _ = tx
        .send(InternalMessage::WorkerDisconnected {
            worker_id,
        })
        .await;
    send_task.abort();
}
