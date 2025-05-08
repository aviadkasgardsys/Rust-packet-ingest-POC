// src/main.rs

mod signal;
mod signal_ws;
use dotenv::dotenv;
use std::{env, error::Error};
use log::{info, error};
use warp::Filter;
use packet_ingest_lib::{
    Context,                  // broadcast context
    run_capture_and_stream,   // capture → SSE
    run_capture,              // capture → InfluxDB
    InfluxWriter,
};
use tokio::task;

/* sudo RUST_LOG=packet_ingest_lib=trace,packet_ingest_server=info \
cargo run -p packet_ingest-server */

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<(), Box<dyn Error>> {
    // ──────── ① Load .env or fail ────────
    dotenv().map_err(|e| format!("Failed to read .env file: {}", e))?;

    // initialize logger and panic hook
    env_logger::init();
    std::panic::set_hook(Box::new(|info| {
        error!("Thread panic: {:?}", info);
    }));

    // ──────── ② Require CAPTURE_IFACE & INFLUX_TOKEN ────────
    let iface = env::var("CAPTURE_IFACE")
        .map_err(|_| "Environment variable CAPTURE_IFACE must be set")?;
    let influx_token = env::var("INFLUX_TOKEN")
        .map_err(|_| "Environment variable INFLUX_TOKEN must be set")?;

    info!("Configured capture interface: {}", iface);

    // build shared broadcast context (holds a broadcast::Sender<_>)
    let ctx = Context::new(1024);

    // ──────── 1) Signaling server on 3031 ────────
    let signal_tx = ctx.tx.clone();
tokio::spawn(async move {
    signal::serve_signaling(signal_tx).await;
});

    // clone iface for blocking tasks
    let iface_for_sse   = iface.clone();
    let iface_for_influx = iface.clone();

    
        let tx_ws = ctx.tx.clone();
        let ws_routes = signal_ws::ws_routes(tx_ws);
        tokio::spawn(async move {
            info!("WebSocket : 0.0.0.0:3032/signal");
            warp::serve(ws_routes).run(([0, 0, 0, 0], 3032)).await;
        });
    

    // ──────── 2) Packet → SSE (PPS stream) ────────
    let capture_tx = ctx.tx.clone();
    task::spawn_blocking(move || {
        run_capture_and_stream(capture_tx, &iface_for_sse)
            .unwrap_or_else(|e| error!("PPS stream failed: {}", e));
    });

    // ──────── 3) Packet → InfluxDB ────────
    let influx = InfluxWriter::new(
        "http://localhost:8086".into(),
        "Asgard".into(),
        "factory_data".into(),
        influx_token,
    )?;
    let influx_clone = influx.clone();
    task::spawn_blocking(move || {
        if let Err(e) = run_capture(influx_clone, &iface_for_influx) {
            error!("Influx capture failed: {}", e);
        }
    });

    // ──────── 4) HTTP (health + static) on 3030 ────────
    let health       = warp::path!("health").map(|| "OK");
    let static_files = warp::path("static")
    .and(warp::fs::dir("static"))
    .with(warp::reply::with::header("cache-control", "no-cache, no-store"));
    let routes       = health.or(static_files);

    info!("HTTP  : 0.0.0.0:3030 (health + static files)");
    info!("SSE   : 0.0.0.0:3031/signal");
    info!("Influx: bucket=factory_data, iface={}", iface);

    warp::serve(routes).run(([0, 0, 0, 0], 3030)).await;
    Ok(())
}