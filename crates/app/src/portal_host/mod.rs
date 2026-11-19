//! Host of the xdg-desktop-portal backend UI (`org.conchy.Portal.ShellHost`).
//!
//! `xdg-desktop-portal-conchy` is a headless router: dialogs that need the shell (permission
//! prompts, and later the ScreenCast source picker) are shown here, in the already-warm process.
//! The daemon calls `Access` as a long pending call and gets the answer when the user is done;
//! a crash here makes it answer `response = 2` (permission **not** granted).

mod access;
mod dbus;

use futures::StreamExt;
use gpui::App;

/// Start serving `org.conchy.Portal.ShellHost` and show the dialogs it asks for.
pub fn init(cx: &mut App) {
    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    tokio::spawn(async move {
        if let Err(err) = dbus::serve(tx).await {
            tracing::warn!("Portal host not available: {err}");
        }
    });
    cx.spawn(async move |cx| {
        while let Some(event) = rx.next().await {
            cx.update(|cx| access::handle(event, cx));
        }
    })
    .detach();
}
