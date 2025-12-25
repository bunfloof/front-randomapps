use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_tungstenite::{accept_async, tungstenite::Message};

const IDLE_TIMEOUT: Duration = Duration::from_secs(180); // 3 minutes
const BASE_URL: &str = "https://fur1.foxomy.com/rustapps/ipgeolocation";
const VALID_APIS: &[&str] = &["extremeip", "ipinfo", "ipregistry", "ipstack", "nange", "nordvpn"];

#[derive(Deserialize)]
struct Request {
    api: String,
    ip: String,
    #[serde(default)]
    id: Option<String>,
}

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("0.0.0.0:45278").await.expect("Failed to bind");
    println!("WebSocket server running on ws://0.0.0.0:45278");

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client");

    loop {
        if let Ok((stream, addr)) = listener.accept().await {
            let client = client.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, client).await {
                    eprintln!("Connection error from {}: {}", addr, e);
                }
            });
        }
    }
}

async fn handle_connection(
    stream: TcpStream, client: Client,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ws_stream = accept_async(stream).await?;
    let (mut write, mut read) = ws_stream.split();

    loop {
        match timeout(IDLE_TIMEOUT, read.next()).await {
            Ok(Some(Ok(msg))) => match msg {
                Message::Text(text) => {
                    let response = if text.trim().eq_ignore_ascii_case("ping") {
                        "pong".to_string()
                    } else {
                        process_request(&client, &text).await
                    };
                    write.send(Message::Text(response.into())).await?;
                }
                Message::Ping(data) => {
                    write.send(Message::Pong(data)).await?;
                }
                Message::Close(_) => break,
                _ => {}
            },
            Ok(Some(Err(e))) => return Err(e.into()),
            Ok(None) => break, // Stream ended
            Err(_) => {
                let _ = write.send(Message::Close(None)).await; // Timeout - no activity for 3 minutes
                break;
            }
        }
    }

    Ok(())
}

async fn process_request(client: &Client, text: &str) -> String {
    let req: Request = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(_) => return r#"{"error":"Invalid JSON. Expected: {\"api\":\"...\",\"ip\":\"...\",\"id\":\"...(optional)\"}}"#.to_string(),
    };

    if !VALID_APIS.contains(&req.api.as_str()) {
        return format!(r#"{{"error":"Invalid API. Valid options: {}"}}"#, VALID_APIS.join(", "));
    }

    let url = format!("{}/{}?ip={}", BASE_URL, req.api, req.ip);

    let data: serde_json::Value = match client.get(&url).send().await {
        Ok(resp) => match resp.json().await {
            Ok(json) => json,
            Err(e) => serde_json::json!({"error": format!("Failed to parse response: {}", e)}),
        },
        Err(e) => serde_json::json!({"error": format!("Request failed: {}", e)}),
    };

    let mut response = serde_json::json!({
        "api": req.api,
        "data": data
    });

    if let Some(id) = req.id {
        response["id"] = serde_json::json!(id);
    }

    response.to_string()
}
