use tokio::sync::broadcast;
use crate::message::Message;

/// A small wrapper around a Tokio broadcast channel,  
/// used to fan-out packets-per-second (or signaling) events.
#[derive(Clone)]
pub struct Context {
    pub tx: broadcast::Sender<Message>,
}

impl Context {
    /// Create a new Context with a channel of the given capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }
}