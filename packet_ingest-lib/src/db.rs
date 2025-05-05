use influxdb2::{ClientBuilder, BuildError};
use influxdb2::models::data_point::{DataPoint, DataPointError};
use futures::stream;
use thiserror::Error;
use std::net::UdpSocket;

/// Errors returned by InfluxWriter operations.
#[derive(Debug, Error)]
pub enum DbError {
    #[error("InfluxDB write error: {0}")]
    Api(#[from] influxdb2::RequestError),

    #[error("Point build error: {0}")]
    BuildPoint(#[from] DataPointError),

    #[error("InfluxDB client build error: {0}")]
    ClientBuild(#[from] BuildError),

    #[error("Timestamp precision overflow converting to nanoseconds")]
    TimestampOverflow,

    #[error("UDP send error: {0}")]
    Udp(#[from] std::io::Error),
}

#[derive(Clone)]
pub struct InfluxWriter {
    client: influxdb2::Client,
    bucket: String,
}

impl InfluxWriter {
    /// Initialize with gzip-enabled client.
    pub fn new(
        host: String,
        org: String,
        bucket: String,
        token: String,
    ) -> Result<Self, DbError> {
        let client = ClientBuilder::new(host, org, token)
            .gzip(true)
            .build()?;
        Ok(Self { client, bucket })
    }

    /// Convert a packet header timeval into a DataPoint with sorted tags.
    pub fn make_point(
        &self,
        protocol: &str,
        length: u32,
        ts_sec: i64,
        ts_usec: i64,
    ) -> Result<DataPoint, DbError> {
        // Compute nanosecond timestamp
        let nanos = ts_sec
            .checked_mul(1_000_000_000)
            .and_then(|s| s.checked_add(ts_usec * 1_000))
            .ok_or(DbError::TimestampOverflow)?;

        // Collect tags and sort lexicographically by key
        let mut tags = vec![("protocol", protocol)];
        tags.sort_by(|a, b| a.0.cmp(b.0));

        // Build the DataPoint
        let mut builder = DataPoint::builder("packet_stats");
        for (k, v) in tags {
            builder = builder.tag(k, v);
        }
        builder
            .field("length", length as i64)
            .timestamp(nanos)
            .build()
            .map_err(DbError::BuildPoint)
    }

    /// Asynchronously write a batch of points via HTTP.
    pub async fn write_batch(&self, points: Vec<DataPoint>) -> Result<(), DbError> {
        self.client.write(&self.bucket, stream::iter(points)).await?;
        Ok(())
    }

    /// Fire-and-forget UDP write (needs [[udp]] listener enabled)
    pub fn write_udp(&self, lines: &str, udp_addr: &str) -> Result<(), DbError> {
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        sock.send_to(lines.as_bytes(), udp_addr)?;
        Ok(())
    }
}
