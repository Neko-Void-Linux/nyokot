//! Port of Chatry's router in `index.ts`: `message.ingress` fans out to
//! `egress.<platform>` for every platform except the origin and any platform
//! already present in `trace_path` (loop prevention).

use crate::bus::{Bus, BusMsg};
use crate::umf::{validate_envelope, Envelope};

pub const PLATFORMS: [&str; 3] = ["discord", "stoat", "fluxer"];

/// Fan-out targets for an ingress envelope (pure — unit tested).
pub fn fanout_targets(origin: &str, trace_path: &[String]) -> Vec<&'static str> {
    PLATFORMS
        .iter()
        .copied()
        .filter(|p| *p != origin && !trace_path.iter().any(|t| t == p))
        .collect()
}

pub async fn run(bus: Bus, mut rx: tokio::sync::broadcast::Receiver<BusMsg>) {
    while let Ok(msg) = rx.recv().await {
        if let BusMsg::Ingress(env) = msg {
            if !validate_envelope(&env) {
                tracing::error!("[router] invalid UMF envelope dropped");
                continue;
            }
            let origin = env.head.source.platform.clone();
            tracing::info!(
                "[router] ingress from {origin}: {}",
                env.body.title.as_deref().unwrap_or("")
            );
            for target in fanout_targets(&origin, &env.head.trace_path) {
                let mut out: Envelope = clone_envelope(&env);
                out.head.trace_path.push(target.to_string());
                let _ = bus.send(BusMsg::Egress {
                    target: target.to_string(),
                    envelope: out,
                });
            }
        }
    }
}

fn clone_envelope(env: &Envelope) -> Envelope {
    // ponytail: JSON round-trip is the cheapest correct deep clone here.
    serde_json::from_value(serde_json::to_value(env).unwrap_or_default())
        .unwrap_or_else(|_| env.clone())
}
