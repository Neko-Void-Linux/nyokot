//! Per-channel paced outbound queue (proactive rate-limit defense).
//!
//! Discord allows ~5 msgs / 5s per channel (50 req/s global); bursting past
//! it earns 429s or worse. Every adapter funnels egress through an `Outbox`
//! that enforces a minimum interval between sends on the same channel.

use crate::umf::Envelope;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub type Deliver = Arc<dyn Fn(Envelope) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

#[derive(Clone)]
pub struct Outbox {
    tx: mpsc::Sender<Envelope>,
}

impl Outbox {
    pub fn spawn(interval: Duration, deliver: Deliver) -> Self {
        let (tx, mut rx) = mpsc::channel::<Envelope>(512);
        tokio::spawn(async move {
            let mut last: HashMap<String, Instant> = HashMap::new();
            while let Some(env) = rx.recv().await {
                let ch = env.head.source.channel_id.clone();
                if let Some(t) = last.get(&ch) {
                    let wait = interval.saturating_sub(t.elapsed());
                    if !wait.is_zero() {
                        tokio::time::sleep(wait).await;
                    }
                }
                last.insert(ch, Instant::now());
                deliver(env).await;
            }
        });
        Self { tx }
    }

    pub async fn push(&self, env: Envelope) {
        if self.tx.send(env).await.is_err() {
            tracing::error!("[outbox] queue closed, dropping egress");
        }
    }
}
