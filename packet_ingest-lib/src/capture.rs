use pcap::{Capture, Device, Error as PcapError};
use log::{info, error};
use std::time::{Instant, Duration};
use std::thread::sleep;
use crate::db::InfluxWriter;

/// Blocking packet capture loop on `iface`, batching into `influx`.
pub fn run_capture_blocking(
    influx: InfluxWriter,
    iface: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. List and log all interfaces
    let devices = Device::list()?;
    info!(
        "Available interfaces: {:?}",
        devices.iter().map(|d| &d.name).collect::<Vec<_>>()
    );

    // 2. Select the named device
    let device = devices
        .into_iter()
        .find(|d| d.name == iface)
        .ok_or_else(|| format!("Interface '{}' not found", iface))?;

    // 3. Open with immediate mode, 100 ms timeout, 16 MiB buffer
    let mut cap = Capture::from_device(device)?
        .promisc(true)
        .snaplen(65535)
        .timeout(100)
        .immediate_mode(true)
        .buffer_size(16 * 1024 * 1024)
        .open()?;

    cap.filter("tcp or udp", true)?;

    // 4. Prepare batching with larger capacity for higher throughput
    let mut buffer = Vec::with_capacity(10_000);
    let mut last_flush = Instant::now();

    loop {
        match cap.next_packet() {
            Ok(pkt) => {
                // Determine protocol tag
                let proto = match pkt.data.get(23) {
                    Some(6)  => "TCP",
                    Some(17) => "UDP",
                    _        => "OTHER",
                };
                // Convert header.ts (timeval) to seconds+microseconds
                let ts = pkt.header.ts;
                let point = influx.make_point(
                    proto,
                    pkt.header.len,
                    ts.tv_sec as i64,
                    ts.tv_usec as i64,
                )?;
                buffer.push(point);

                // Flush when buffer is full or interval elapsed
                if buffer.len() >= 10_000 || last_flush.elapsed() > Duration::from_millis(100) {
                    // Drain batch and record its size before moving
                    let batch = buffer.drain(..).collect::<Vec<_>>();
                    let count = batch.len();

                    // Spawn async write in parallel
                    let influx_clone = influx.clone();
                    tokio::runtime::Handle::current().spawn(async move {
                        if let Err(e) = influx_clone.write_batch(batch).await {
                            error!("Batch write failed: {}", e);
                        }
                    });

                    info!("Spawned async write of {} points", count);
                    last_flush = Instant::now();
                }
            }
            Err(PcapError::TimeoutExpired) => {
                //info!("No packets in interval");
                sleep(Duration::from_millis(10));
            }
            Err(e) => {
                error!("Capture error: {:?}", e);
                sleep(Duration::from_millis(50));
            }
        }
    }
}

