use crate::{ChromeError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{RwLock, broadcast, mpsc};

use super::protocol::{Notification, Request, Response};

static CLIENT_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

pub type ClientId = u64;

pub struct ClientConnection {
    pub id: ClientId,
    tx: mpsc::Sender<String>,
}

impl ClientConnection {
    pub async fn send(&self, msg: &str) -> Result<()> {
        self.tx
            .send(msg.to_string())
            .await
            .map_err(|_| ChromeError::General("Client disconnected".to_string()))
    }

    pub async fn send_response(&self, response: &Response) -> Result<()> {
        let json = serde_json::to_string(response)?;
        self.send(&json).await
    }

    pub async fn send_notification(&self, notification: &Notification) -> Result<()> {
        let json = serde_json::to_string(notification)?;
        self.send(&json).await
    }
}

pub struct IpcServer {
    socket_path: PathBuf,
    clients: Arc<RwLock<HashMap<ClientId, ClientConnection>>>,
    shutdown_tx: broadcast::Sender<()>,
}

impl IpcServer {
    pub fn new(socket_path: PathBuf) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            socket_path,
            clients: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx,
        }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub async fn bind(&self) -> Result<UnixListener> {
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        UnixListener::bind(&self.socket_path)
            .map_err(|e| ChromeError::General(format!("Failed to bind socket: {}", e)))
    }

    pub async fn accept<F, Fut>(&self, listener: &UnixListener, on_request: F) -> Result<()>
    where
        F: Fn(ClientId, Request) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Response> + Send,
    {
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
                            let client_id = CLIENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
                            let clients = self.clients.clone();
                            let on_request = on_request.clone();

                            tokio::spawn(async move {
                                Self::handle_client(stream, client_id, clients, on_request).await;
                            });
                        }
                        Err(e) => {
                            tracing::error!("Accept error: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_client<F, Fut>(
        stream: UnixStream,
        client_id: ClientId,
        clients: Arc<RwLock<HashMap<ClientId, ClientConnection>>>,
        on_request: F,
    ) where
        F: Fn(ClientId, Request) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Response> + Send,
    {
        let (read_half, mut write_half) = stream.into_split();
        let (tx, mut rx) = mpsc::channel::<String>(256);

        let connection = ClientConnection { id: client_id, tx };
        clients.write().await.insert(client_id, connection);

        let write_task = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if write_half
                    .write_all(format!("{}\n", msg).as_bytes())
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        let reader = BufReader::new(read_half);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.is_empty() {
                continue;
            }

            match serde_json::from_str::<Request>(&line) {
                Ok(request) => {
                    let response = on_request(client_id, request).await;
                    if let Some(client) = clients.read().await.get(&client_id) {
                        client.send_response(&response).await.ok();
                    }
                }
                Err(e) => {
                    let response = Response::error(
                        0,
                        super::protocol::error_codes::PARSE_ERROR,
                        format!("Parse error: {}", e),
                    );
                    if let Some(client) = clients.read().await.get(&client_id) {
                        client.send_response(&response).await.ok();
                    }
                }
            }
        }

        clients.write().await.remove(&client_id);
        write_task.abort();
    }

    pub async fn broadcast(&self, notification: &Notification) -> Result<()> {
        let json = serde_json::to_string(notification)?;
        let clients = self.clients.read().await;

        for client in clients.values() {
            client.send(&json).await.ok();
        }

        Ok(())
    }

    pub async fn send_to(&self, client_id: ClientId, notification: &Notification) -> Result<()> {
        let clients = self.clients.read().await;
        if let Some(client) = clients.get(&client_id) {
            client.send_notification(notification).await?;
        }
        Ok(())
    }

    pub fn shutdown(&self) {
        self.shutdown_tx.send(()).ok();
    }

    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ipc_server_creation() {
        let server = IpcServer::new(PathBuf::from("/tmp/test-cdtcli.sock"));
        assert_eq!(server.socket_path(), Path::new("/tmp/test-cdtcli.sock"));
    }
}
