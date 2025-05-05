use std::convert::Infallible;
use futures_util::{StreamExt, SinkExt, future::select};
use tokio::sync::broadcast::{Sender, error::RecvError};
use warp::{Filter, Rejection, Reply, ws::{Message as WsMsg, WebSocket}};
use packet_ingest_lib::Message;
use log::info;

/// Cloneable filter for broadcasting
fn with_tx(
    tx: Sender<Message>,
) -> impl Filter<Extract = (Sender<Message>,), Error = Infallible> + Clone {
    warp::any().map(move || tx.clone())
}

/// Build the WebSocket route under `/signal`
pub fn ws_routes(
    tx: Sender<Message>,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    // allow CORS for WS handshake
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "OPTIONS"])
        .allow_headers(vec!["sec-websocket-protocol", "origin", "upgrade"]);

    warp::path("signal")
        .and(warp::ws())
        .and(with_tx(tx))
        .map(|ws: warp::ws::Ws, tx| {
            ws.on_upgrade(move |socket| handle_ws(socket, tx))
        })
        .with(cors)
}

async fn handle_ws(ws: WebSocket, tx: Sender<Message>) {
    let (mut ws_tx, mut ws_rx) = ws.split();
    let mut rx = tx.subscribe();

    // Incoming from client → broadcast
    let inbound = async {
        while let Some(result) = ws_rx.next().await {
            if let Ok(msg) = result {
                if let Ok(txt) = msg.to_str() {
                    if let Ok(parsed) = serde_json::from_str::<Message>(txt) {
                        if let Message::Signal { sdp, candidate } = parsed {
                            let _ = tx.send(Message::Signal { sdp, candidate });
                        }
                    }
                }
            }
        }
    };

   // Outgoing broadcast → client (BINARY frames)
   let outbound = async {
    loop {
        match rx.recv().await {
            // ←— handle a batch of packet data at once
            Ok(Message::Batch { readings }) => {
                // each reading is 12 bytes; pre-allocate for speed
                let mut buf = Vec::with_capacity(readings.len() * 12);
                for pkt in readings {
                    buf.extend_from_slice(&pkt.timestamp.to_le_bytes());
                    buf.extend_from_slice(&pkt.value.   to_le_bytes());
                }
                if ws_tx.send(WsMsg::binary(buf)).await.is_err() {
                    break; // client disconnected
                }
            }

            // still handle one-off signals if you need them
            Ok(Message::Signal { sdp, candidate }) => {
                let txt = serde_json::to_string(&Message::Signal { sdp, candidate })
                    .unwrap();
                if ws_tx.send(WsMsg::text(txt)).await.is_err() {
                    break;
                }
            }

            // optional: forward legacy single datapoints
            Ok(Message::Data { timestamp, value }) => {
                let mut buf = [0u8; 12];
                buf[0..8].copy_from_slice(&timestamp.to_le_bytes());
                buf[8..12].copy_from_slice(&value.   to_le_bytes());
                if ws_tx.send(WsMsg::binary(buf)).await.is_err() {
                    break;
                }
            }

            Err(RecvError::Lagged(skipped)) => {
                log::warn!("WS client lagged, dropped {} messages", skipped);
                // you could break here to drop very slow clients
            }
            Err(RecvError::Closed) => break,
        }
    }
};

// Run inbound and outbound until one finishes
select(Box::pin(inbound), Box::pin(outbound)).await;
info!("WebSocket client disconnected");
}