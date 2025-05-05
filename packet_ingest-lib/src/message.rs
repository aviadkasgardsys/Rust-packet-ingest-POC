use serde::{Deserialize, Serialize};

/// Individual packets‐per‐second datapoint
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PacketData {
    pub timestamp: i64,
    pub value:     u32,
}

/// Events that can be broadcast:
/// - `Signal` for WebRTC SDP/ICE handshake  
/// - `Data`  for single readings (optional if you always batch)  
/// - `Batch` for a chunk of PacketData at once
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Message {
    Signal { sdp: String, candidate: Option<String> },
    Data   { timestamp: i64, value: u32 },          // you can keep this if you need per‐point too
    Batch  { readings: Vec<PacketData> },          // new variant
}