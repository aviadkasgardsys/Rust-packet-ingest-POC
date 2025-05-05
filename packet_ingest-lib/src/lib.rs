//! packet_ingest-lib: packet capture, signaled streaming, and InfluxDB writing

pub mod capture;
pub mod db;
pub mod capture_stream;

// newly added modules:
pub mod context;
pub mod message;

// re-exports for ergonomic imports:
pub use context::Context;
pub use message::Message;

// existing exports:
pub use capture::run_capture_blocking as run_capture;
pub use capture_stream::run_capture_and_stream;
pub use db::InfluxWriter;