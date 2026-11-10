//! Notification service implementing org.freedesktop.Notifications.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use futures_signals::signal::Mutable;
use tokio::sync::watch;
use tracing::{debug, warn};
use zbus::{
    Connection,
    fdo::{DBusProxy, RequestNameFlags, RequestNameReply},
    interface,
    message::Header,
    names::{BusName, UniqueName, WellKnownName},
    object_server::SignalEmitter,
    proxy,
    zvariant::{OwnedValue, Structure},
};

use crate::ServiceStatus;
use crate::applications::icons::lookup_icon;
use crate::lifecycle::{Lifecycle, ManagedService};

const NAME: WellKnownName =
    WellKnownName::from_static_str_unchecked("org.freedesktop.Notifications");
const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
const DEFAULT_TIMEOUT_MS: i32 = 5000;
const DEFAULT_ACTION_KEY: &str = "default";
const MAX_NOTIFICATION_HISTORY: usize = 200;

/// Held while claiming or releasing the bus name and object path, so a stop
/// followed quickly by a start cannot interleave and let the stop's late
/// release tear down the new server.
static REGISTRATION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A single desktop notification.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub app_icon_path: Option<PathBuf>,
    pub image_path: Option<PathBuf>,
    pub summary: String,
    pub body: String,
    pub urgency: u8,
    pub timeout_ms: i32,
    pub timestamp_ms: i64,
    /// When the popup goes on its own, in unix ms; `None` keeps it until
    /// dismissed. Moves forward by however long the popups were held.
    pub expires_at_ms: Option<i64>,
    pub actions: Vec<(String, String)>,
    pub read: bool,
    /// `desktop-entry` hint, without `.desktop`.
    pub desktop_entry: Option<String>,
    /// Pid of the D-Bus client that sent it (the portal's for sandboxed apps).
    pub sender_pid: Option<u32>,
}

impl Notification {
    pub fn has_default_action(&self) -> bool {
        self.actions
            .iter()
            .any(|(key, _)| key == DEFAULT_ACTION_KEY)
    }

    /// App ids or window classes the sender's windows likely carry.
    pub fn app_ids(&self) -> Vec<String> {
        self.desktop_entry
            .iter()
            .chain(Some(&self.app_name).filter(|name| !name.is_empty()))
            .cloned()
            .collect()
    }
}

/// Notification center state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotificationData {
    /// Shared so a write hands each subscriber pointers, not copies.
    pub notifications: Vec<Arc<Notification>>,
    pub popup_ids: Vec<u32>,
    /// Set while the pointer holds the popups: their countdowns stand still
    /// at this instant.
    pub popups_held_since_ms: Option<i64>,
    pub dnd: bool,
    pub unread_count: usize,
}

impl NotificationData {
    fn recompute_unread(&mut self) {
        self.unread_count = self.notifications.iter().filter(|n| !n.read).count();
    }

    fn latest_popup(&self) -> Option<Arc<Notification>> {
        let id = self.popup_ids.first().copied()?;
        self.notifications.iter().find(|n| n.id == id).cloned()
    }
}

/// A notification the shell posts about itself, one per slot: posting the same
/// slot again replaces its card instead of stacking a second one, so a config
/// error that re-fires on every bad save stays a single card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalSlot {
    ConfigError,
    ThemeError,
    ConfigWriteError,
}

impl LocalSlot {
    /// Slot ids count down from the top of the range and the daemon's count up
    /// from 1, so the two can never meet.
    fn id(self) -> u32 {
        u32::MAX - self as u32
    }
}

/// Commands for the notification service.
#[derive(Debug, Clone)]
pub enum NotificationCommand {
    Dismiss(u32),
    DismissLatest,
    DismissAll,
    SetDnd(bool),
    MarkAllRead,
    InvokeAction(u32, String),
    /// A click on the notification: invoke its default action, then dismiss.
    Activate(u32),
    /// Pause popup expiry (pointer over the popups).
    HoldPopups(bool),
}

/// Event-driven notification service.
#[derive(Debug, Clone, Default)]
pub struct NotificationSubscriber {
    core: crate::ServiceCore<NotificationData, NotificationCommand>,
    /// Expiry timers wait while set.
    popups_held: Arc<watch::Sender<bool>>,
}

impl std::ops::Deref for NotificationSubscriber {
    type Target = crate::ServiceCore<NotificationData, NotificationCommand>;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

impl NotificationSubscriber {
    /// Create the notification subscriber. The D-Bus daemon is claimed by
    /// [`ManagedService::start`].
    pub fn new() -> Self {
        Self::default()
    }

    pub fn latest_popup(&self) -> Option<Arc<Notification>> {
        self.data.lock_ref().latest_popup()
    }

    pub fn popup_notifications(&self, limit: usize) -> Vec<Arc<Notification>> {
        let data = self.data.lock_ref();
        data.popup_ids
            .iter()
            .filter_map(|id| data.notifications.iter().find(|n| n.id == *id).cloned())
            .take(limit)
            .collect()
    }

    /// The instant the popups' countdowns froze at, while they are held.
    pub fn popups_held_since_ms(&self) -> Option<i64> {
        self.data.lock_ref().popups_held_since_ms
    }

    /// Post a notification from the shell itself. We *are* the notification
    /// daemon, so it goes straight into the state rather than out over D-Bus -
    /// which also means it still shows when the bus name went to someone else
    /// or the service is off.
    ///
    /// It stays until dismissed: an error the user has not read yet is worth
    /// more than a tidy popup stack.
    pub fn post_local(&self, slot: LocalSlot, summary: String, body: String) {
        let id = slot.id();
        let notification = Notification {
            id,
            app_name: "GPUi Shell".to_string(),
            summary,
            body,
            urgency: 2,
            timeout_ms: 0,
            timestamp_ms: Utc::now().timestamp_millis(),
            ..Notification::default()
        };

        let mut data = self.data.lock_mut();
        data.notifications.retain(|n| n.id != id);
        data.popup_ids.retain(|n| *n != id);
        data.notifications.insert(0, Arc::new(notification));
        // Do-not-disturb still applies: it lands in the center, unread.
        if !data.dnd {
            data.popup_ids.insert(0, id);
        }
        data.recompute_unread();
    }

    /// Drop a slot's notification, once whatever it complained about is fixed.
    /// Only that card goes - nothing else can hold a slot id - and when the
    /// slot is empty nothing is touched at all: this runs on every good save,
    /// and every write lock wakes every subscriber.
    pub fn clear_local(&self, slot: LocalSlot) {
        let id = slot.id();
        let posted = self
            .data
            .lock_ref()
            .notifications
            .iter()
            .any(|n| n.id == id);
        if posted {
            remove_notification(&self.data, id);
        }
    }

    /// Apply a command. State-only commands take effect immediately, and so
    /// does everything while the daemon is down - the history is ours either
    /// way. Commands that have to tell a client go through the queue.
    pub fn dispatch(&self, command: NotificationCommand) {
        match command {
            NotificationCommand::SetDnd(enabled) => {
                let mut data = self.data.lock_mut();
                data.dnd = enabled;
                if enabled {
                    data.popup_ids.clear();
                }
            }
            NotificationCommand::MarkAllRead => {
                let mut data = self.data.lock_mut();
                for item in data.notifications.iter_mut().filter(|n| !n.read) {
                    Arc::make_mut(item).read = true;
                }
                data.recompute_unread();
            }
            NotificationCommand::HoldPopups(held) => {
                if *self.popups_held.borrow() == held {
                    return;
                }
                {
                    let mut data = self.data.lock_mut();
                    if held {
                        data.popups_held_since_ms = Some(now_ms());
                    } else if let Some(since) = data.popups_held_since_ms.take() {
                        // Deadlines move before the timers wake, so none fires on a stale one.
                        let shift = now_ms().saturating_sub(since);
                        let popups = data.popup_ids.clone();
                        for item in data.notifications.iter_mut() {
                            if let Some(at) = item.expires_at_ms
                                && popups.contains(&item.id)
                            {
                                Arc::make_mut(item).expires_at_ms = Some(at + shift);
                            }
                        }
                    }
                }
                self.popups_held.send_replace(held);
            }
            command if !self.lifecycle.is_up() => {
                for id in self.target_ids(&command) {
                    remove_notification(&self.data, id);
                }
            }
            command => self.jobs.send(command),
        }
    }

    /// The notifications a dismissing command acts on.
    fn target_ids(&self, command: &NotificationCommand) -> Vec<u32> {
        let data = self.data.lock_ref();
        match command {
            NotificationCommand::Dismiss(id)
            | NotificationCommand::InvokeAction(id, _)
            | NotificationCommand::Activate(id) => vec![*id],
            NotificationCommand::DismissLatest => data
                .notifications
                .first()
                .map(|n| n.id)
                .into_iter()
                .collect(),
            NotificationCommand::DismissAll => data.notifications.iter().map(|n| n.id).collect(),
            NotificationCommand::SetDnd(_)
            | NotificationCommand::MarkAllRead
            | NotificationCommand::HoldPopups(_) => Vec::new(),
        }
    }

    async fn execute(&self, command: NotificationCommand) -> anyhow::Result<()> {
        match &command {
            NotificationCommand::InvokeAction(id, action_key) => {
                self.emit_action_invoked(*id, action_key).await;
            }
            NotificationCommand::Activate(id) => {
                let has_default = self
                    .data
                    .lock_ref()
                    .notifications
                    .iter()
                    .any(|n| n.id == *id && n.has_default_action());
                if has_default {
                    self.emit_action_invoked(*id, DEFAULT_ACTION_KEY).await;
                }
            }
            _ => {}
        }
        for id in self.target_ids(&command) {
            self.dismiss_by_id(id).await?;
        }
        Ok(())
    }

    async fn emit_action_invoked(&self, id: u32, action_key: &str) {
        let Ok(conn) = crate::bus::session().await else {
            return;
        };
        if let Ok(iface) = conn
            .object_server()
            .interface::<_, NotificationServer>(OBJECT_PATH)
            .await
        {
            let ctx = iface.signal_emitter();
            let _ = NotificationServer::action_invoked(ctx, id, action_key).await;
        }
    }

    async fn dismiss_by_id(&self, id: u32) -> anyhow::Result<()> {
        if self.lifecycle.is_up() {
            let conn = crate::bus::session().await?;
            let proxy = NotificationsProxy::new(&conn).await?;
            let _ = proxy.close_notification(id).await;
        }
        remove_notification(&self.data, id);
        Ok(())
    }
}

impl ManagedService for NotificationSubscriber {
    fn name(&self) -> &'static str {
        "Notifications"
    }

    fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    /// Notification bodies and their action labels are kept in history, so
    /// they dominate what this service retains.
    fn memory_bytes(&self) -> usize {
        let data = self.data.lock_ref();
        size_of::<NotificationData>()
            + data.popup_ids.len() * size_of::<u32>()
            + data
                .notifications
                .iter()
                .map(|notification| {
                    size_of::<Notification>()
                        + notification.app_name.len()
                        + notification.app_icon.len()
                        + notification.summary.len()
                        + notification.body.len()
                        + notification
                            .app_icon_path
                            .as_ref()
                            .map_or(0, |path| path.as_os_str().len())
                        + notification
                            .image_path
                            .as_ref()
                            .map_or(0, |path| path.as_os_str().len())
                        + notification
                            .actions
                            .iter()
                            .map(|(key, label)| key.len() + label.len())
                            .sum::<usize>()
                })
                .sum::<usize>()
    }

    fn start(&self) {
        let token = self.lifecycle.begin();
        let data = self.data.clone();
        let popups_held = self.popups_held.clone();

        let subscriber = self.clone();
        self.jobs.run(token.clone(), move |command| {
            let subscriber = subscriber.clone();
            async move { subscriber.execute(command).await }
        });

        tokio::spawn(async move {
            let _registration = REGISTRATION.lock().await;
            let _ = tokio::task::spawn_blocking(prune_image_cache).await;
            if !token.alive() {
                return;
            }

            let conn = match crate::bus::session().await {
                Ok(conn) => conn,
                Err(e) => {
                    warn!("Failed to connect to the session bus: {e}");
                    token.error(e.to_string());
                    return;
                }
            };

            // A previous run may still own the object path.
            let _ = conn
                .object_server()
                .remove::<NotificationServer, _>(OBJECT_PATH)
                .await;

            let server = NotificationServer::new(data, conn.clone(), popups_held);
            if let Err(e) = conn.object_server().at(OBJECT_PATH, server).await {
                warn!("Failed to serve the notification interface: {e}");
                token.error(e.to_string());
                return;
            }

            let flags = RequestNameFlags::AllowReplacement;
            match DBusProxy::new(&conn).await {
                Ok(proxy) => match proxy.request_name(NAME, flags.into()).await {
                    Ok(RequestNameReply::InQueue) => {
                        warn!("Bus name '{NAME}' already owned, notifications unavailable");
                        token.set(ServiceStatus::Unavailable);
                    }
                    Ok(_) => token.active(),
                    Err(e) => {
                        warn!("Failed to request bus name '{NAME}': {e}");
                        token.error(e.to_string());
                    }
                },
                Err(e) => {
                    warn!("Failed to reach the session bus: {e}");
                    token.error(e.to_string());
                }
            }
        });
    }

    /// Releases the bus name as well: a stopped notification daemon must let
    /// another one take over.
    fn stop(&self) {
        self.lifecycle.stop();

        let lifecycle = self.lifecycle.clone();
        tokio::spawn(async move {
            let _registration = REGISTRATION.lock().await;
            // Started again before this ran: the new run owns the name now.
            if !lifecycle.is_stopped() {
                return;
            }
            let Ok(conn) = crate::bus::session().await else {
                return;
            };
            let _ = conn
                .object_server()
                .remove::<NotificationServer, _>(OBJECT_PATH)
                .await;
            if let Ok(proxy) = DBusProxy::new(&conn).await {
                let _ = proxy.release_name(NAME).await;
            }
        });
    }
}

#[derive(Debug)]
struct NotificationServer {
    data: Mutable<NotificationData>,
    conn: Connection,
    next_id: u32,
    next_timer_generation: u64,
    timer_generations: Arc<Mutex<HashMap<u32, u64>>>,
    popups_held: Arc<watch::Sender<bool>>,
}

impl NotificationServer {
    fn new(
        data: Mutable<NotificationData>,
        conn: Connection,
        popups_held: Arc<watch::Sender<bool>>,
    ) -> Self {
        let next_id = first_free_id(&data.lock_ref());
        Self {
            data,
            conn,
            next_id,
            next_timer_generation: 1,
            timer_generations: Arc::new(Mutex::new(HashMap::new())),
            popups_held,
        }
    }

    async fn sender_pid(&self, sender: &UniqueName<'_>) -> Option<u32> {
        DBusProxy::new(&self.conn)
            .await
            .ok()?
            .get_connection_unix_process_id(BusName::Unique(sender.clone()))
            .await
            .inspect_err(|e| debug!("No pid for notification sender {sender}: {e}"))
            .ok()
    }
}

#[interface(
    name = "org.freedesktop.Notifications",
    proxy(
        gen_blocking = false,
        default_service = "org.freedesktop.Notifications",
        default_path = "/org/freedesktop/Notifications",
    )
)]
impl NotificationServer {
    #[zbus(name = "GetCapabilities")]
    async fn get_capabilities(&self) -> Vec<String> {
        vec![
            "actions".to_string(),
            "body".to_string(),
            "body-markup".to_string(),
            "persistence".to_string(),
        ]
    }

    #[zbus(name = "GetServerInformation")]
    async fn get_server_information(&self) -> (String, String, String, String) {
        (
            "GPUi Shell".to_string(),
            "gpuishell".to_string(),
            "0.1.0".to_string(),
            "1.2".to_string(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[zbus(name = "Notify")]
    async fn notify(
        &mut self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
        #[zbus(header)] header: Header<'_>,
    ) -> u32 {
        let sender_pid = match header.sender() {
            Some(sender) => self.sender_pid(sender).await,
            None => None,
        };

        let id = {
            let existing = self
                .data
                .lock_ref()
                .notifications
                .iter()
                .any(|n| n.id == replaces_id);
            if replaces_id != 0 && existing {
                replaces_id
            } else {
                let id = self.next_id;
                self.next_id = self.next_id.saturating_add(1);
                id
            }
        };

        let urgency = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v.clone()).ok())
            .unwrap_or(1);
        // Some senders (e.g. Chromium) write the icon/image to a temp file and
        // unlink it shortly after sending the notification, so only keep paths
        // that still exist by the time we parse the hints - otherwise gpui's
        // asset cache logs a load error for a file that's already gone.
        // Image priority follows the spec: `image-data`, `image-path`, then an
        // `app_icon` that is a file. A file `app_icon` beside a real image is
        // the app icon; one that became the image (a screenshot) is not shown
        // twice.
        let app_icon_file = is_image_source(app_icon).then(|| normalize_path(app_icon));
        let hinted_image = cache_image_data(&hints, id).or_else(|| {
            hint_string(&hints, &["image-path", "image_path"])
                .and_then(|p| cache_image_file(&normalize_path(&p), id))
        });
        let icon_is_image = hinted_image.is_none() && app_icon_file.is_some();
        let image_path = hinted_image.or_else(|| {
            let icon_file = app_icon_file.as_deref()?;
            cache_image_file(icon_file, id)
        });
        let app_icon_path = match &app_icon_file {
            Some(_) if icon_is_image => None,
            Some(file) => Some(file.clone()),
            None => hint_string(&hints, &["app_icon", "icon-path", "icon_path"])
                .filter(|value| is_image_source(value))
                .map(|p| normalize_path(&p)),
        }
        .and_then(|p| cache_image_file(&p, id));
        // Fallback: resolve named icon via XDG icon theme lookup
        let app_icon_path = app_icon_path.or_else(|| {
            if !app_icon.is_empty() && app_icon_file.is_none() {
                lookup_icon(app_icon)
            } else {
                None
            }
        });
        // Fallback: try desktop-entry hint for icon lookup
        let app_icon_path = app_icon_path.or_else(|| {
            hint_string(&hints, &["desktop-entry"]).and_then(|entry| lookup_icon(&entry))
        });
        // The spec: critical notifications don't expire on their own.
        let timeout_ms = if urgency == 2 {
            0
        } else if expire_timeout < 0 {
            DEFAULT_TIMEOUT_MS
        } else {
            expire_timeout
        };
        let desktop_entry = hint_string(&hints, &["desktop-entry"])
            .map(|entry| entry.strip_suffix(".desktop").unwrap_or(&entry).to_string());
        let parsed_actions = actions
            .chunks(2)
            .filter_map(|chunk| match chunk {
                [key, label] => Some((key.clone(), label.clone())),
                _ => None,
            })
            .collect();

        let timestamp_ms = now_ms();
        let notification = Notification {
            id,
            app_name: app_name.to_string(),
            app_icon: app_icon.to_string(),
            app_icon_path,
            image_path,
            summary: summary.to_string(),
            body: body.to_string(),
            urgency,
            timeout_ms,
            timestamp_ms,
            expires_at_ms: (timeout_ms > 0).then(|| timestamp_ms + i64::from(timeout_ms)),
            actions: parsed_actions,
            read: false,
            desktop_entry,
            sender_pid,
        };

        {
            let mut data = self.data.lock_mut();
            data.notifications.retain(|n| n.id != id);
            data.popup_ids.retain(|n| *n != id);
            data.notifications.insert(0, Arc::new(notification));
            if data.notifications.len() > MAX_NOTIFICATION_HISTORY {
                data.notifications.truncate(MAX_NOTIFICATION_HISTORY);
            }
            if !data.dnd {
                data.popup_ids.insert(0, id);
            }
            data.recompute_unread();
        }

        if timeout_ms > 0 {
            self.next_timer_generation = self.next_timer_generation.saturating_add(1);
            let generation = self.next_timer_generation;
            if let Ok(mut timers) = self.timer_generations.lock() {
                timers.insert(id, generation);
            }

            let conn = self.conn.clone();
            let data = self.data.clone();
            let timer_generations = self.timer_generations.clone();
            let popups_held = self.popups_held.clone();
            tokio::spawn(async move {
                let mut held = popups_held.subscribe();
                // The deadline moves while the popups are held, so sleep up
                // to it, then check again; one that is no longer a popup
                // (dismissed, do-not-disturb) still closes for its sender.
                while let Some(deadline) = popup_deadline(&data, id) {
                    let remaining = deadline - now_ms();
                    if remaining > 0 {
                        tokio::time::sleep(Duration::from_millis(remaining as u64)).await;
                    } else if *held.borrow() {
                        // Err only once the subscriber is gone; expire anyway.
                        if held.wait_for(|held| !held).await.is_err() {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                let should_close = timer_generations
                    .lock()
                    .ok()
                    .and_then(|map| map.get(&id).copied())
                    .map(|current_generation| current_generation == generation)
                    .unwrap_or(false);
                if !should_close {
                    return;
                }

                if let Ok(mut timers) = timer_generations.lock() {
                    timers.remove(&id);
                }

                if !deactivate_notification(&data, id) {
                    return;
                }

                // Emit NotificationClosed with reason 1 (expired)
                if let Ok(iface) = conn
                    .object_server()
                    .interface::<_, NotificationServer>(OBJECT_PATH)
                    .await
                {
                    let ctx = iface.signal_emitter();
                    let _ = NotificationServer::notification_closed(ctx, id, 1).await;
                }
            });
        }

        id
    }

    #[zbus(name = "CloseNotification")]
    async fn close_notification(
        &mut self,
        id: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        if let Ok(mut timers) = self.timer_generations.lock() {
            timers.remove(&id);
        }

        if deactivate_notification(&self.data, id) {
            let _ = NotificationServer::notification_closed(&emitter, id, 2).await;
        }
    }

    #[zbus(signal, name = "NotificationClosed")]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "ActionInvoked")]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

/// The id a new daemon run starts counting from.
///
/// History outlives a restart, and `Notify` replaces any entry with the id
/// it hands out, so counting from 1 again would quietly delete old entries.
/// Local slot ids live at the top of the range and are skipped.
fn first_free_id(data: &NotificationData) -> u32 {
    let lowest_slot = LocalSlot::ConfigError
        .id()
        .min(LocalSlot::ThemeError.id())
        .min(LocalSlot::ConfigWriteError.id());
    data.notifications
        .iter()
        .map(|n| n.id)
        .filter(|id| *id < lowest_slot)
        .max()
        .map_or(1, |id| id.saturating_add(1))
}

fn remove_notification(data: &Mutable<NotificationData>, id: u32) -> bool {
    let mut state = data.lock_mut();
    let len_before = state.notifications.len();
    state.notifications.retain(|n| n.id != id);
    state.popup_ids.retain(|x| *x != id);
    state.recompute_unread();
    len_before != state.notifications.len()
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// When `id`'s popup is due, while it is one and has a deadline.
fn popup_deadline(data: &Mutable<NotificationData>, id: u32) -> Option<i64> {
    let data = data.lock_ref();
    if !data.popup_ids.contains(&id) {
        return None;
    }
    data.notifications
        .iter()
        .find(|n| n.id == id)?
        .expires_at_ms
}

fn deactivate_notification(data: &Mutable<NotificationData>, id: u32) -> bool {
    let mut state = data.lock_mut();
    let had_popup = state.popup_ids.contains(&id);
    state.popup_ids.retain(|x| *x != id);
    had_popup || state.notifications.iter().any(|n| n.id == id)
}

fn hint_string(hints: &HashMap<String, OwnedValue>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        hints.get(*key).and_then(|v| {
            String::try_from(v.clone())
                .ok()
                .or_else(|| {
                    let raw = format!("{v:?}");
                    raw.strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                        .map(ToString::to_string)
                })
                .filter(|s| !s.trim().is_empty())
        })
    })
}

fn is_image_source(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with("file://")
        || value.starts_with("http://")
        || value.starts_with("https://")
}

fn normalize_path(value: &str) -> PathBuf {
    let path = value.strip_prefix("file://").unwrap_or(value);
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2]))
        {
            out.push((h << 4) | l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    PathBuf::from(String::from_utf8_lossy(&out).into_owned())
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications",
    gen_blocking = false
)]
trait Notifications {
    #[zbus(name = "CloseNotification")]
    fn close_notification(&self, id: u32) -> zbus::Result<()>;
}

/// Senders unlink temp images right after `Notify` and `image-data` only
/// lives in the message, so every image is copied here before it is shown.
fn image_cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
            PathBuf::from(home).join(".cache")
        });
    base.join("gpuishell").join("notifications")
}

/// Drop cached images older than a day; the history never keeps one longer.
fn prune_image_cache() {
    let Ok(entries) = std::fs::read_dir(image_cache_dir()) else {
        return;
    };
    let cutoff = std::time::SystemTime::now() - Duration::from_secs(24 * 60 * 60);
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .map(|modified| modified < cutoff)
            .unwrap_or(false);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn cache_image_file(source: &Path, id: u32) -> Option<PathBuf> {
    let bytes = std::fs::read(source).ok()?;
    let ext = source
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("img");
    write_cached(id, ext, &bytes)
}

fn write_cached(id: u32, ext: &str, bytes: &[u8]) -> Option<PathBuf> {
    let dir = image_cache_dir();
    std::fs::create_dir_all(&dir).ok()?;
    // Unique per notification, so gpui's image cache never serves a replaced one.
    let path = dir.join(format!("{id}-{}.{ext}", Utc::now().timestamp_millis()));
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// `image-data` (`iibiiay`): width, height, row stride, has alpha, bits per
/// sample, channels, pixels. Written out as a PNG.
fn cache_image_data(hints: &HashMap<String, OwnedValue>, id: u32) -> Option<PathBuf> {
    let value = ["image-data", "image_data", "icon_data"]
        .iter()
        .find_map(|key| hints.get(*key))?;
    let structure = Structure::try_from(value.clone()).ok()?;
    let fields = structure.fields();
    if fields.len() != 7 {
        return None;
    }
    let width = i32::try_from(&fields[0]).ok()?;
    let height = i32::try_from(&fields[1]).ok()?;
    let stride = i32::try_from(&fields[2]).ok()?;
    let has_alpha = bool::try_from(&fields[3]).ok()?;
    let bits = i32::try_from(&fields[4]).ok()?;
    let channels = i32::try_from(&fields[5]).ok()?;
    let data = Vec::<u8>::try_from(fields[6].clone()).ok()?;
    if bits != 8 || width <= 0 || height <= 0 || channels < 3 || (has_alpha && channels < 4) {
        return None;
    }
    let (width, height, stride, channels) = (
        width as usize,
        height as usize,
        stride as usize,
        channels as usize,
    );
    let mut rgba = image::RgbaImage::new(width as u32, height as u32);
    for y in 0..height {
        for x in 0..width {
            let start = y * stride + x * channels;
            let px = data.get(start..start + channels)?;
            let alpha = if has_alpha { px[3] } else { 255 };
            rgba.put_pixel(
                x as u32,
                y as u32,
                image::Rgba([px[0], px[1], px[2], alpha]),
            );
        }
    }
    let mut png = Vec::new();
    rgba.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    write_cached(id, "png", &png)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config error has to reach the center as one sticky card, and the next
    /// one about the same file must replace it rather than stack - a bad file
    /// gets saved repeatedly while it is being fixed.
    #[test]
    fn a_local_slot_holds_a_single_sticky_notification() {
        let notifications = NotificationSubscriber::new();

        notifications.post_local(LocalSlot::ConfigError, "first".into(), "body".into());
        notifications.post_local(LocalSlot::ConfigError, "second".into(), "body".into());

        let data = notifications.get();
        assert_eq!(data.notifications.len(), 1);
        assert_eq!(data.notifications[0].summary, "second");
        assert_eq!(data.unread_count, 1);
        // Never expires on its own, and sits above the daemon's ids.
        assert_eq!(data.notifications[0].timeout_ms, 0);
        assert_eq!(data.popup_ids, vec![u32::MAX]);

        notifications.clear_local(LocalSlot::ConfigError);
        assert!(notifications.get().notifications.is_empty());
    }

    /// A file that parses again clears its own card and nothing else: neither
    /// the other file's, nor anything that came in over the bus.
    #[test]
    fn clearing_a_slot_leaves_every_other_notification_alone() {
        let notifications = NotificationSubscriber::new();
        notifications.post_local(LocalSlot::ConfigError, "config".into(), String::new());
        notifications.post_local(LocalSlot::ThemeError, "theme".into(), String::new());
        notifications
            .data
            .lock_mut()
            .notifications
            .push(Arc::new(Notification {
                id: 7,
                summary: "from an app on the bus".to_string(),
                ..Notification::default()
            }));

        notifications.clear_local(LocalSlot::ConfigError);
        // The second call has nothing left to do, and must not take the
        // neighbours with it.
        notifications.clear_local(LocalSlot::ConfigError);

        let surviving: Vec<u32> = notifications
            .get()
            .notifications
            .iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(surviving, vec![LocalSlot::ThemeError.id(), 7]);
    }

    #[test]
    fn plain_path_unchanged() {
        assert_eq!(
            normalize_path("/home/user/pic.png"),
            PathBuf::from("/home/user/pic.png")
        );
    }

    #[test]
    fn strips_file_scheme() {
        assert_eq!(
            normalize_path("file:///home/user/pic.png"),
            PathBuf::from("/home/user/pic.png")
        );
    }

    #[test]
    fn decodes_percent_space() {
        assert_eq!(
            normalize_path("file:///home/user/My%20Pictures/test.png"),
            PathBuf::from("/home/user/My Pictures/test.png"),
        );
    }

    #[test]
    fn decodes_utf8_sequence() {
        assert_eq!(
            normalize_path("/tmp/%C3%A9.png"),
            PathBuf::from("/tmp/é.png")
        );
    }

    #[test]
    fn decodes_lowercase_hex() {
        assert_eq!(normalize_path("/a/b%2fc"), PathBuf::from("/a/b/c"));
    }

    #[test]
    fn keeps_invalid_hex_literal() {
        assert_eq!(normalize_path("/a/%ZZ/b"), PathBuf::from("/a/%ZZ/b"));
    }

    #[test]
    fn keeps_trailing_short_percent() {
        assert_eq!(normalize_path("/a/%2"), PathBuf::from("/a/%2"));
        assert_eq!(normalize_path("/a/%"), PathBuf::from("/a/%"));
    }

    #[test]
    fn mixed_encoding() {
        assert_eq!(
            normalize_path("file:///tmp/My%20%C3%A9.png"),
            PathBuf::from("/tmp/My é.png"),
        );
    }

    /// A restarted daemon must not hand out ids still in the history: `Notify`
    /// replaces whatever holds the id, which deleted old entries.
    #[test]
    fn a_restart_keeps_counting_past_the_history() {
        let notifications = NotificationSubscriber::new();
        assert_eq!(first_free_id(&notifications.get()), 1);

        notifications.post_local(LocalSlot::ConfigError, "bad".into(), String::new());
        {
            let mut data = notifications.data.lock_mut();
            for id in [3, 7] {
                data.notifications.push(Arc::new(Notification {
                    id,
                    ..Notification::default()
                }));
            }
        }
        assert_eq!(first_free_id(&notifications.get()), 8);
    }
}
