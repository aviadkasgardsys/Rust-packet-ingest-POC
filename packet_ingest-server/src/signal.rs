use std::convert::Infallible;
use async_stream::stream;
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
use packet_ingest_lib::Message;
use warp::{Filter, sse::{Event, reply, keep_alive}};
use log::info;

#[derive(Clone, Debug, Deserialize)]
pub struct SignalRequest {
    pub sdp: String,
    #[serde(default)]
    pub candidate: Option<String>,
}

pub async fn serve_signaling(tx: Sender<Message>) {
    // POST /signal
    let post = warp::post()
        .and(warp::path("signal"))
        .and(warp::body::json::<SignalRequest>())
        .and_then({
            let post_tx = tx.clone();
            move |req: SignalRequest| {
                let post_tx = post_tx.clone();
                async move {
                    info!("POST /signal: {:?}", req);
                    post_tx.send(Message::Signal { sdp: req.sdp, candidate: req.candidate })
                        .map_err(|_| warp::reject())?;
                    Ok::<_, warp::Rejection>(warp::reply())
                }
            }
        });

    // GET /signal → SSE
    let sse_route = warp::get()
        .and(warp::path("signal"))
        .map({
            // clone the Sender, not the Receiver
            let tx = tx.clone();
            move || {
                // subscribe inside the closure
                let mut rx = tx.subscribe();
                // build a fresh stream for each client
                let event_stream = stream! {
                    while let Ok(msg) = rx.recv().await {
                        let json = serde_json::to_string(&msg).unwrap();
                        yield Ok::<_, Infallible>(Event::default().data(json));
                    }
                };
                // reply sets up text/event-stream headers for you
                reply(keep_alive().stream(event_stream))
            }
        });

    // Build CORS once, apply to both routes
    let cors = warp::cors()
        .allow_any_origin()  
        .allow_methods(vec!["OPTIONS", "GET", "POST"])  
        .allow_headers(vec!["content-type", "accept", "last-event-id", "origin"]);

    // Serve combined
    warp::serve(post.or(sse_route).with(cors))
        .run(([0, 0, 0, 0], 3031))
        .await;

}