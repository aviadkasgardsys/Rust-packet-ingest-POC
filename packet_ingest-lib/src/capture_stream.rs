use std::time::{Duration, Instant};
use pcap::{Capture, Device, Error as PcapError};
use log::{info, error};
use tokio::sync::broadcast::Sender;
use crate::message::{Message, PacketData};
use chrono::Utc;

/// Runs packet capture and streams batches of packet data over the broadcast channel
pub fn run_capture_and_stream(
    tx: Sender<Message>,
    iface: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Find the device
    let devices = Device::list()?;
    let device = devices
        .into_iter()
        .find(|d| d.name == iface)
        .ok_or_else(|| format!("Interface '{}' not found", iface))?;

    let mut cap = Capture::from_device(device)?
        .promisc(true)
        .snaplen(65535)
        .timeout(100)
        .immediate_mode(true)
        .open()?;
    info!("Started packet capture on interface '{}'", iface);

    // 2. Prepare batch buffering
    let mut buffer: Vec<PacketData> = Vec::with_capacity(128);
    let batch_size     = 64;                       // flush when we have 64 entries
    let batch_interval = Duration::from_millis(200); // or every 200ms
    let mut last_flush  = Instant::now();

    loop {
        // 3. Capture one packet
        match cap.next_packet() {
            Ok(packet) => {
                let len = packet.header.len as u32;
                let ts  = Utc::now()
                    .timestamp_nanos_opt()
                    .expect("chrono returned no timestamp");

                // 4. Accumulate into our buffer
                buffer.push(PacketData { timestamp: ts, value: len });

                // 5a. Flush on size
                if buffer.len() >= batch_size {
                    let batch = std::mem::take(&mut buffer);
                    if let Err(e) = tx.send(Message::Batch { readings: batch }) {
                        error!("Failed to send batch: {}", e);
                    }
                    last_flush = Instant::now();
                }
            }
            Err(PcapError::TimeoutExpired) => {
                // no packet this interval
            }
            Err(e) => {
                error!("pcap error: {:?}", e);
                std::thread::sleep(Duration::from_millis(50));
            }
        }

        // 5b. Flush on time
        if last_flush.elapsed() >= batch_interval && !buffer.is_empty() {
            let batch = std::mem::take(&mut buffer);
            if let Err(e) = tx.send(Message::Batch { readings: batch }) {
                error!("Failed to send batch: {}", e);
            }
            last_flush = Instant::now();
        }
    }
}