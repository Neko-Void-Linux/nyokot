//! Port of Chatry's `BaseAdapter`: one async trait for every platform.
//!
//! The shared ingress guard (`validate_envelope`) lives in the bus layer;
//! each adapter also validates before emitting, like `emitIngress`.

use crate::bus::Bus;
use crate::error::Result;
use crate::umf::{validate_envelope, Envelope};
use std::sync::Arc;

pub mod discord;
pub mod fluxer;
pub mod stoat;

pub use discord::DiscordAdapter;
pub use fluxer::FluxerAdapter;
pub use stoat::StoatAdapter;

/// Port of `BaseAdapter` (init/start/stop/processEgress/send/edit/delete).
/// `edit`/`delete` default to no-ops — most notifier traffic is send-only.
///
/// Methods take `&self` so one `Arc<A>` can serve the gateway task, the
/// egress loop and graceful shutdown at once.
#[allow(async_fn_in_trait)] // single-crate binary; Send-ness verified at spawn sites
pub trait Adapter: Send + Sync + 'static {
    fn platform_name(&self) -> &'static str;

    /// Connect and start listening (ingress). Skip quietly when unconfigured,
    /// mirroring Chatry's warn-and-skip on missing tokens.
    async fn start(&self) -> Result<()>;

    async fn stop(&self) -> Result<()>;

    /// Handle one validated egress envelope addressed to this platform.
    async fn process_egress(&self, envelope: &Envelope) -> Result<()>;

    /// Low-level send. Returns platform message ids.
    async fn send_message(&self, envelope: &Envelope) -> Result<Vec<String>>;

    async fn edit_message(&self, _envelope: &Envelope, _ids: &[String]) -> Result<Vec<String>> {
        Ok(vec![])
    }

    async fn delete_message(&self, _channel: &str, _ids: &[String]) -> Result<()> {
        Ok(())
    }
}

/// Shared ingress guard — port of `emitIngress`.
pub fn check_ingress(platform: &str, envelope: &Envelope) -> bool {
    if !validate_envelope(envelope) {
        tracing::warn!("[{platform}] invalid UMF envelope dropped on ingress");
        return false;
    }
    true
}

/// Per-adapter egress consumer: filters `Egress{target == platform}`,
/// re-validates (port of the `init` guard in `BaseAdapter`), and dispatches.
pub async fn egress_loop<A: Adapter>(adapter: Arc<A>, bus: Bus) {
    let mut rx = bus.subscribe();
    while let Ok(msg) = rx.recv().await {
        if let crate::bus::BusMsg::Egress { target, envelope } = msg {
            if target != adapter.platform_name() {
                continue;
            }
            if !validate_envelope(&envelope) {
                tracing::error!(
                    "[{}] invalid UMF envelope dropped on egress",
                    adapter.platform_name()
                );
                continue;
            }
            if let Err(e) = adapter.process_egress(&envelope).await {
                tracing::error!("[{}] process_egress failed: {e}", adapter.platform_name());
            }
        }
    }
}
