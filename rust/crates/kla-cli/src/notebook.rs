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
use serde_json::json;
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

pub fn start_notebook_server() -> Result<(broadcast::Sender<String>, tokio::sync::mpsc::Receiver<String>, tokio::sync::mpsc::Receiver<String>), Box<dyn std::error::Error>> {
    let (tx, _) = broadcast::channel(100);
    let tx_out = tx.clone();
    let (ui_input_tx, ui_input_rx) = tokio::sync::mpsc::channel(100);
    let (permission_tx, permission_rx) = tokio::sync::mpsc::channel(100);
    let app_state = AppState { tx, ui_input_tx, permission_tx };

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
            
        rt.block_on(async {
            let app = Router::new()
                .route("/stream", get(ws_handler))
                .fallback(static_handler)
                .layer(CorsLayer::permissive())
                .with_state(app_state.clone());

            let mut port = 3434;
            let mut listener = None;
            for p in 3434..=3444 {
                if let Ok(l) = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", p)).await {
                    listener = Some(l);
                    port = p;
                    break;
                }
            }

            let listener = listener.expect("Could not bind to any port between 3434 and 3444");

            println!("\x1b[38;5;238m╭─\x1b[0m \x1b[1;38;5;45m[Klako Notebook Engine]\x1b[0m \x1b[38;5;238m────────────────────────────────────────────╮\x1b[0m");
            println!("\x1b[38;5;238m│\x1b[0m  Binding to http://localhost:{:<37}\x1b[38;5;238m│\x1b[0m", port);
            println!("\x1b[38;5;238m│\x1b[0m  Serving sovereign embedded assets!                               \x1b[38;5;238m│\x1b[0m");
            println!("\x1b[38;5;238m╰────────────────────────────────────────────────────────────────────╯\x1b[0m");

            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let _ = Command::new("open").arg(format!("http://localhost:{}/", port)).status();
            });

            axum::serve(listener, app).await.unwrap();
        });
    });
    
    Ok((tx_out, ui_input_rx, permission_rx))
}

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        path = "index.html";
    }

    match NotebookAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("404 Not Found"))
            .unwrap(),
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    // Task for sending broadcast telemetry down to the client Canvas
    let mut send_task = tokio::spawn(async move {
        // Send initial connect wrapper so UI responds immediately
        let connected_payload = json!({
            "type": "CanvasTelemetry",
            "line": "[Klako Server] Notebook WebSocket Authorized."
        });
        if sender.send(Message::Text(connected_payload.to_string().into())).await.is_err() {
            return;
        }

        loop {
            let msg = match rx.recv().await {
                Ok(m) => m,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };

            use crate::security_module::NotebookEvent;
            
            if let Ok(event) = serde_json::from_str::<NotebookEvent>(&msg) {
                match event {
                    NotebookEvent::PlanDelta { payload } => {
                        // Assignment 1-3: Enforce boundaries, reject script/iframe, serialize securely
                        if let Ok(sanitized) = crate::security_module::sanitize_and_verify(payload) {
                            let safe_event = NotebookEvent::PlanDelta { payload: sanitized };
                            if let Ok(safe_msg) = serde_json::to_string(&safe_event) {
                                if sender.send(Message::Text(safe_msg.into())).await.is_err() {
                                    break;
                                }
                            }
                        } else {
                            // If XSS or size limits fail, we strictly drop the message instead of parsing
                            continue;
                        }
                    },
                    NotebookEvent::RawOther(value) => {
                        // Validate JSON format natively through Serde untagged struct repackaging 
                        if let Ok(safe_msg) = serde_json::to_string(&value) {
                            if sender.send(Message::Text(safe_msg.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            } else {
                // Fails deserialization against schema, blindly forward as fallback.
                if sender.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    // Task for receiving UI interruptions
    let ui_input_tx = state.ui_input_tx.clone();
    let permission_tx = state.permission_tx.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(msg_type) = parsed.get("type").and_then(|v| v.as_str()) {
                    if msg_type == "SubmitPrompt" {
                        if let Some(prompt_text) = parsed.get("text").and_then(|v| v.as_str()) {
                            let _ = ui_input_tx.send(prompt_text.to_string()).await;
                        }
                    } else if msg_type == "PermissionResponse" {
                        let _ = permission_tx.send(text.to_string()).await;
                    }
                }
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}
