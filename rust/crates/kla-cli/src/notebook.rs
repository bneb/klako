use std::process::Command;
use std::time::Duration;

use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../../notebook-ui/"]
struct NotebookAssets;

#[derive(Clone)]
struct AppState {
    pub tx: broadcast::Sender<String>,
    pub ui_input_tx: tokio::sync::mpsc::Sender<String>,
    pub permission_tx: tokio::sync::mpsc::Sender<String>,
}

pub async fn start_notebook_server() -> Result<(broadcast::Sender<String>, tokio::sync::mpsc::Receiver<String>, tokio::sync::mpsc::Receiver<String>), Box<dyn std::error::Error>> {
    let (tx, _) = broadcast::channel(100);
    let tx_out = tx.clone();
    let (ui_input_tx, ui_input_rx) = tokio::sync::mpsc::channel(100);
    let (permission_tx, permission_rx) = tokio::sync::mpsc::channel(100);
    let app_state = AppState { tx, ui_input_tx, permission_tx };

    tokio::spawn(async move {
        let app = Router::new()
            .route("/stream", get(ws_handler))
            .fallback(static_handler)
            .layer(CorsLayer::permissive())
            .with_state(app_state.clone());

        let mut port = 3434;
        let mut listener = None;
        for p in 3434..=3444 {
            if let Ok(l) = tokio::net::TcpListener::bind(format!("127.0.0.1:{p}")).await {
                listener = Some(l);
                port = p;
                break;
            }
        }

        let listener = listener.expect("Could not bind to any port between 3434 and 3444");

        println!("\x1b[38;5;238m╭─\x1b[0m \x1b[1;38;5;45m[Klako Notebook Engine]\x1b[0m \x1b[38;5;238m────────────────────────────────────────────╮\x1b[0m");
        println!("\x1b[38;5;238m│\x1b[0m  Binding to http://localhost:{port:<37}\x1b[38;5;238m│\x1b[0m");
        println!("\x1b[38;5;238m│\x1b[0m  Serving sovereign embedded assets!                               \x1b[38;5;238m│\x1b[0m");
        println!("\x1b[38;5;238m╰────────────────────────────────────────────────────────────────────╯\x1b[0m");

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            #[cfg(target_os = "macos")]
            let _ = Command::new("open").arg(format!("http://localhost:{port}")).status();
            #[cfg(target_os = "linux")]
            let _ = Command::new("xdg-open").arg(format!("http://localhost:{}", port)).status();
        });

        let _ = axum::serve(listener, app).await;
    });

    Ok((tx_out, ui_input_rx, permission_rx))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    let ui_input_tx = state.ui_input_tx.clone();
    let permission_tx = state.permission_tx.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&text) {
                match payload["type"].as_str() {
                    Some("SubmitPrompt") => {
                        if let Some(prompt) = payload["text"].as_str() {
                            let _ = ui_input_tx.send(prompt.to_string()).await;
                        }
                    }
                    Some("PermissionResponse") => {
                        if let Some(response) = payload["response"].as_str() {
                            let _ = permission_tx.send(response.to_string()).await;
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();

    if path.is_empty() {
        path = "index.html".to_string();
    }

    match NotebookAssets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
