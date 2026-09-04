//! Port of Chatry's EventEmitter bus (`message.ingress` / `egress.<platform>`).

use crate::umf::Envelope;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum BusMsg {
    Ingress(Envelope),
    Egress { target: String, envelope: Envelope },
}

pub type Bus = broadcast::Sender<BusMsg>;

pub fn new_bus() -> (Bus, broadcast::Receiver<BusMsg>) {
    broadcast::channel(1024)
}
