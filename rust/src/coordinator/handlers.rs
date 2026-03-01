use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::Response,
};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use tokio::sync::{mpsc, oneshot};

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
    let (sender, mut receiver) = socket.split();

    let Some((alias, cmd_rx)) = register_worker(&mut receiver, &tx).await else {
        return;
    };

    let send_task = tokio::spawn(forward_commands(sender, cmd_rx));
    receive_results(&mut receiver, &tx, &alias).await;

    let _ = tx.send(InternalMessage::WorkerDisconnected { alias }).await;
    send_task.abort();
}

async fn register_worker(
    receiver: &mut SplitStream<WebSocket>,
    tx: &mpsc::Sender<InternalMessage>,
) -> Option<(String, mpsc::UnboundedReceiver<CoordinatorCommand>)> {
    let text = match receiver.next().await {
        Some(Ok(Message::Text(t))) => t,
        _ => return None,
    };

    let WorkerMessage::Hello = serde_json::from_str(&text).ok()? else {
        return None;
    };

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (alias_tx, alias_rx) = oneshot::channel();
    tx.send(InternalMessage::WorkerConnected {
        tx: cmd_tx,
        alias_tx,
    })
    .await
    .ok()?;

    let alias = alias_rx.await.ok()?;
    Some((alias, cmd_rx))
}

async fn forward_commands(
    mut sender: SplitSink<WebSocket, Message>,
    mut cmd_rx: mpsc::UnboundedReceiver<CoordinatorCommand>,
) {
    while let Some(cmd) = cmd_rx.recv().await {
        let Ok(text) = serde_json::to_string(&cmd) else { break };
        if sender.send(Message::Text(text.into())).await.is_err() {
            break;
        }
    }
}

async fn receive_results(
    receiver: &mut SplitStream<WebSocket>,
    tx: &mpsc::Sender<InternalMessage>,
    alias: &str,
) {
    while let Some(Ok(Message::Text(text))) = receiver.next().await {
        if let Ok(WorkerMessage::ComputeResult { task_id, data }) = serde_json::from_str(&text) {
            let _ = tx
                .send(InternalMessage::WorkerFinished {
                    task_id,
                    alias: alias.to_string(),
                    data,
                })
                .await;
        }
    }
}
