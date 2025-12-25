use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::Message;

const PORT: u16 = 45203;
const PING_TIMEOUT: Duration = Duration::from_secs(180);

fn timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind(format!("0.0.0.0:{PORT}")).await.unwrap();
    println!("WebSocket server listening on ws://0.0.0.0:{PORT}/ws");

    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(async move {
            let callback = |req: &Request, resp: Response| {
                if req.uri().path() == "/ws" {
                    Ok(resp)
                } else {
                    Err(Response::builder().status(404).body(None).unwrap())
                }
            };

            let ws = match tokio_tungstenite::accept_hdr_async(stream, callback).await {
                Ok(ws) => ws,
                Err(_) => return,
            };
            println!("{addr} connected");

            let (mut tx, mut rx) = ws.split();

            loop {
                match timeout(PING_TIMEOUT, rx.next()).await {
                    Ok(Some(Ok(msg))) => {
                        if let Message::Text(text) = msg {
                            if text.starts_with("PING ") {
                                let reply = format!("PONG {}", timestamp_ms());
                                if tx.send(Message::Text(reply.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(Some(Err(_))) | Ok(None) => break,
                    Err(_) => {
                        println!("{addr} timed out");
                        break;
                    }
                }
            }

            println!("{addr} disconnected");
        });
    }
}