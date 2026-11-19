//! The D-Bus side of the portal host: pure plumbing, no UI.

use conchy_portal_proto::access::{AccessRequest, AccessResult};
use conchy_portal_proto::{HOST_OBJECT_PATH, SHELL_HOST_BUS_NAME, response};
use futures::channel::{mpsc, oneshot};
use zbus::interface;

/// What the daemon asked the shell to do.
pub enum HostEvent {
    /// Show a permission dialog; the answer goes to `reply`. A dropped sender is `OTHER`.
    Access {
        id: String,
        req: Box<AccessRequest>,
        reply: oneshot::Sender<(u32, AccessResult)>,
    },
    /// The frontend closed the request: dismiss the dialog and answer `CANCELLED`.
    Cancel { id: String },
}

pub struct ShellHost {
    tx: mpsc::UnboundedSender<HostEvent>,
}

#[interface(name = "org.conchy.Portal.ShellHost")]
impl ShellHost {
    async fn access(&self, id: String, req: AccessRequest) -> (u32, AccessResult) {
        let (reply, rx) = oneshot::channel();
        let event = HostEvent::Access { id, req: Box::new(req), reply };
        if self.tx.unbounded_send(event).is_err() {
            return (response::OTHER, AccessResult::default());
        }
        rx.await
            .unwrap_or((response::OTHER, AccessResult::default()))
    }

    async fn cancel(&self, id: String) {
        let _ = self.tx.unbounded_send(HostEvent::Cancel { id });
    }
}

/// Serve the host on the shared session connection and claim its well-known name.
pub async fn serve(tx: mpsc::UnboundedSender<HostEvent>) -> zbus::Result<()> {
    let conn = services::bus::session().await?;
    conn.object_server()
        .at(HOST_OBJECT_PATH, ShellHost { tx })
        .await?;
    conn.request_name(SHELL_HOST_BUS_NAME).await?;
    tracing::info!("Portal host serving {SHELL_HOST_BUS_NAME}");
    Ok(())
}
