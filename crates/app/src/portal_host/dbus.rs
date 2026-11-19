//! The D-Bus side of the portal host: pure plumbing, no UI.

use conchy_portal_proto::access::{AccessRequest, AccessResult};
use conchy_portal_proto::cast::{CastPickRequest, CastPickResult};
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
    /// Show the ScreenCast source picker.
    PickCast {
        id: String,
        req: Box<CastPickRequest>,
        reply: oneshot::Sender<(u32, CastPickResult)>,
    },
    /// The frontend closed the request: dismiss the dialog and answer `CANCELLED`.
    Cancel { id: String },
}

pub struct ShellHost {
    tx: mpsc::UnboundedSender<HostEvent>,
}

/// Only the portal daemon may drive the dialogs: the caller must own the daemon's well-known
/// name. Otherwise any process in the session could pop up a "permission" prompt in the name
/// of another app, or dismiss a real one.
async fn from_daemon(conn: &zbus::Connection, header: &zbus::message::Header<'_>) -> bool {
    let Some(sender) = header.sender() else { return false };
    let Ok(bus) = zbus::fdo::DBusProxy::new(conn).await else { return false };
    let Ok(name) = zbus::names::BusName::try_from(conchy_portal_proto::DAEMON_BUS_NAME) else {
        return false;
    };
    bus.get_name_owner(name)
        .await
        .is_ok_and(|owner| owner.as_str() == sender.as_str())
}

#[interface(name = "org.conchy.Portal.ShellHost")]
impl ShellHost {
    async fn access(
        &self,
        id: String,
        req: AccessRequest,
        #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> (u32, AccessResult) {
        if !from_daemon(conn, &header).await {
            return (response::OTHER, AccessResult::default());
        }
        let (reply, rx) = oneshot::channel();
        let event = HostEvent::Access { id, req: Box::new(req), reply };
        if self.tx.unbounded_send(event).is_err() {
            return (response::OTHER, AccessResult::default());
        }
        rx.await
            .unwrap_or((response::OTHER, AccessResult::default()))
    }

    async fn pick_cast_sources(
        &self,
        id: String,
        req: CastPickRequest,
        #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> (u32, CastPickResult) {
        if !from_daemon(conn, &header).await {
            return (response::OTHER, CastPickResult::default());
        }
        let (reply, rx) = oneshot::channel();
        let event = HostEvent::PickCast { id, req: Box::new(req), reply };
        if self.tx.unbounded_send(event).is_err() {
            return (response::OTHER, CastPickResult::default());
        }
        rx.await
            .unwrap_or((response::OTHER, CastPickResult::default()))
    }

    async fn cancel(
        &self,
        id: String,
        #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) {
        if from_daemon(conn, &header).await {
            let _ = self.tx.unbounded_send(HostEvent::Cancel { id });
        }
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
