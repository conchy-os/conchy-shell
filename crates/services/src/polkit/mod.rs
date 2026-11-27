//! The polkit authentication agent: the shell is the session's agent, so
//! `pkexec`, privileged settings and services such as vpnx's TUN mode ask
//! for the password in a shell dialog instead of a GTK or Qt one.
//!
//! The agent object (`org.freedesktop.PolicyKit1.AuthenticationAgent`) lives
//! on the system bus and is registered with polkitd for this login session.
//! polkitd calls `BeginAuthentication`; the call stays pending until the user
//! authenticates or gives up. The password itself is checked by
//! `polkit-agent-helper-1` (see [`helper`]), never by the shell.
//!
//! This module is plumbing only: requests go to the UI as [`AgentEvent`]s.

pub mod helper;

use std::collections::HashMap;

use futures::channel::{mpsc, oneshot};
use zbus::zvariant::{OwnedValue, Value};
use zbus::{fdo, interface};

pub use helper::{HelperMsg, HelperSession};

/// Where the agent object is served.
pub const AGENT_PATH: &str = "/org/conchy/PolicyKit1/AuthenticationAgent";
const POLKIT_NAME: &str = "org.freedesktop.PolicyKit1";
const AUTHORITY_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";
const AUTHORITY_IFACE: &str = "org.freedesktop.PolicyKit1.Authority";

/// A user the request may be authenticated as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub uid: u32,
    pub name: String,
}

/// What polkitd asks the user to confirm.
#[derive(Debug, Clone)]
pub struct AuthRequest {
    pub cookie: String,
    pub action_id: String,
    /// Already translated by polkitd for the agent's locale.
    pub message: String,
    pub icon_name: String,
    /// Action details, e.g. `program` and `command_line` from pkexec.
    pub details: HashMap<String, String>,
    /// Users that may authenticate, groups expanded to their members.
    pub identities: Vec<Identity>,
}

impl AuthRequest {
    /// The command pkexec is about to run, if this is a pkexec request.
    pub fn command(&self) -> Option<&str> {
        self.details
            .get("command_line")
            .or_else(|| self.details.get("program"))
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }

    /// Who to authenticate as first: the user running the shell when polkit
    /// allows it (the usual sudo-like case), else the first one offered.
    pub fn preferred_identity(&self) -> usize {
        // SAFETY: getuid cannot fail.
        let me = unsafe { libc::getuid() };
        self.identities
            .iter()
            .position(|i| i.uid == me)
            .unwrap_or(0)
    }
}

/// How a request ended for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthOutcome {
    /// The helper reported success to polkitd.
    Authenticated,
    /// Dismissed, or the request was withdrawn.
    Cancelled,
}

/// Requests for the UI.
pub enum AgentEvent {
    /// Show a dialog for `req`; answer on `reply`. A dropped sender counts as
    /// cancelled.
    Begin {
        req: Box<AuthRequest>,
        reply: oneshot::Sender<AuthOutcome>,
    },
    /// polkitd withdrew the request with this cookie (e.g. pkexec was killed).
    Cancel { cookie: String },
}

/// Errors as polkitd expects them.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.PolicyKit1.Error")]
pub enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Failed(String),
    Cancelled(String),
}

struct Agent {
    tx: mpsc::UnboundedSender<AgentEvent>,
}

/// Only polkitd may drive the agent: anything else on the system bus could
/// otherwise pop up password dialogs in its name.
async fn from_polkitd(conn: &zbus::Connection, header: &zbus::message::Header<'_>) -> bool {
    let Some(sender) = header.sender() else {
        return false;
    };
    let Ok(bus) = fdo::DBusProxy::new(conn).await else {
        return false;
    };
    let Ok(name) = zbus::names::BusName::try_from(POLKIT_NAME) else {
        return false;
    };
    bus.get_name_owner(name)
        .await
        .is_ok_and(|owner| owner.as_str() == sender.as_str())
}

#[interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
    #[allow(clippy::too_many_arguments)]
    async fn begin_authentication(
        &self,
        action_id: String,
        message: String,
        icon_name: String,
        details: HashMap<String, String>,
        cookie: String,
        identities: Vec<(String, HashMap<String, OwnedValue>)>,
        #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> Result<(), AgentError> {
        if !from_polkitd(conn, &header).await {
            return Err(AgentError::Failed("not polkitd".into()));
        }
        let identities = expand_identities(&identities);
        if identities.is_empty() {
            return Err(AgentError::Failed("no user to authenticate as".into()));
        }
        let (reply, rx) = oneshot::channel();
        let req = AuthRequest {
            cookie,
            action_id,
            message,
            icon_name,
            details,
            identities,
        };
        let event = AgentEvent::Begin {
            req: Box::new(req),
            reply,
        };
        if self.tx.unbounded_send(event).is_err() {
            return Err(AgentError::Failed("the shell is shutting down".into()));
        }
        match rx.await {
            Ok(AuthOutcome::Authenticated) => Ok(()),
            _ => Err(AgentError::Cancelled(
                "Authentication dialog was dismissed".into(),
            )),
        }
    }

    async fn cancel_authentication(
        &self,
        cookie: String,
        #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> Result<(), AgentError> {
        if !from_polkitd(conn, &header).await {
            return Err(AgentError::Failed("not polkitd".into()));
        }
        let _ = self.tx.unbounded_send(AgentEvent::Cancel { cookie });
        Ok(())
    }
}

/// Serve the agent on the shared system-bus connection and register it with
/// polkitd for this session. Fails if another agent already holds the
/// session (polkitd allows one).
pub async fn serve(tx: mpsc::UnboundedSender<AgentEvent>) -> anyhow::Result<()> {
    let conn = crate::bus::system().await?;
    conn.object_server().at(AGENT_PATH, Agent { tx }).await?;
    let subject = subject(&conn).await?;
    let described = describe(&subject);
    let locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| "C".into());
    conn.call_method(
        Some(POLKIT_NAME),
        AUTHORITY_PATH,
        Some(AUTHORITY_IFACE),
        "RegisterAuthenticationAgent",
        &(subject, locale, AGENT_PATH),
    )
    .await
    .map_err(|err| anyhow::anyhow!("polkitd refused the agent for {described}: {err}"))?;
    tracing::info!("Polkit authentication agent registered for {described}");
    Ok(())
}

type Subject = (&'static str, HashMap<&'static str, Value<'static>>);

/// The subject the agent answers for: this login session.
///
/// `CONCHY_POLKIT_PROCESS=<pid>` registers for one process instead (as
/// `pkttyagent --process` does), for testing without taking over the session.
async fn subject(conn: &zbus::Connection) -> anyhow::Result<Subject> {
    if let Some(pid) = std::env::var("CONCHY_POLKIT_PROCESS")
        .ok()
        .and_then(|p| p.trim().parse::<u32>().ok())
    {
        let start = process_start_time(pid)
            .ok_or_else(|| anyhow::anyhow!("no process {pid} to register the agent for"))?;
        let mut details = HashMap::new();
        details.insert("pid", Value::U32(pid));
        details.insert("start-time", Value::U64(start));
        return Ok(("unix-process", details));
    }
    let id = match std::env::var("XDG_SESSION_ID") {
        Ok(id) if !id.is_empty() => id,
        _ => logind_session_id(conn).await?,
    };
    let mut details = HashMap::new();
    details.insert("session-id", Value::from(id));
    Ok(("unix-session", details))
}

fn describe((kind, details): &Subject) -> String {
    let value = details
        .get("session-id")
        .or_else(|| details.get("pid"))
        .map(|v| match v {
            Value::Str(s) => s.to_string(),
            Value::U32(n) => n.to_string(),
            other => other.to_string(),
        })
        .unwrap_or_default();
    format!("{kind} {value}")
}

/// Ask logind (elogind on Void) which session this process belongs to.
async fn logind_session_id(conn: &zbus::Connection) -> anyhow::Result<String> {
    let reply = conn
        .call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "GetSessionByPID",
            &(std::process::id()),
        )
        .await
        .map_err(|err| anyhow::anyhow!("no XDG_SESSION_ID and logind does not know us: {err}"))?;
    let path: zbus::zvariant::OwnedObjectPath = reply.body().deserialize()?;
    let props = fdo::PropertiesProxy::builder(conn)
        .destination("org.freedesktop.login1")?
        .path(path)?
        .build()
        .await?;
    let id = props
        .get(
            zbus::names::InterfaceName::try_from("org.freedesktop.login1.Session")?,
            "Id",
        )
        .await?;
    Ok(String::try_from(id)?)
}

/// Field 22 of `/proc/<pid>/stat`, which polkit uses to tell a pid from a
/// later process with the same number.
fn process_start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name may hold spaces and parentheses: count after the last ')'.
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(19)?.parse().ok()
}

/// `unix-user` identities as they are, `unix-group` ones as their members;
/// duplicates and unknown ids dropped, the order kept.
fn expand_identities(identities: &[(String, HashMap<String, OwnedValue>)]) -> Vec<Identity> {
    let mut out: Vec<Identity> = Vec::new();
    let mut push = |identity: Identity| {
        if !out.iter().any(|i| i.uid == identity.uid) {
            out.push(identity);
        }
    };
    for (kind, details) in identities {
        match kind.as_str() {
            "unix-user" => {
                if let Some(uid) = details.get("uid").and_then(|v| u32::try_from(v).ok())
                    && let Some(name) = users::name_of(uid)
                {
                    push(Identity { uid, name });
                }
            }
            "unix-group" => {
                if let Some(gid) = details.get("gid").and_then(|v| u32::try_from(v).ok()) {
                    for name in users::members_of(gid) {
                        if let Some(uid) = users::uid_of(&name) {
                            push(Identity { uid, name });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The user database through NSS (`getpw*_r`, `getgr*_r`).
mod users {
    use std::ffi::{CStr, CString};

    const BUF: usize = 16 * 1024;

    pub fn name_of(uid: u32) -> Option<String> {
        let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
        let mut buf = vec![0 as libc::c_char; BUF];
        let mut result = std::ptr::null_mut();
        // SAFETY: all pointers are valid for the call; `result` is checked.
        let rc =
            unsafe { libc::getpwuid_r(uid, &mut pw, buf.as_mut_ptr(), buf.len(), &mut result) };
        if rc != 0 || result.is_null() {
            return None;
        }
        // SAFETY: on success `pw_name` points into `buf`.
        Some(
            unsafe { CStr::from_ptr(pw.pw_name) }
                .to_string_lossy()
                .into_owned(),
        )
    }

    pub fn uid_of(name: &str) -> Option<u32> {
        let name = CString::new(name).ok()?;
        let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
        let mut buf = vec![0 as libc::c_char; BUF];
        let mut result = std::ptr::null_mut();
        // SAFETY: as above.
        let rc = unsafe {
            libc::getpwnam_r(
                name.as_ptr(),
                &mut pw,
                buf.as_mut_ptr(),
                buf.len(),
                &mut result,
            )
        };
        (rc == 0 && !result.is_null()).then_some(pw.pw_uid)
    }

    pub fn members_of(gid: u32) -> Vec<String> {
        let mut gr: libc::group = unsafe { std::mem::zeroed() };
        let mut buf = vec![0 as libc::c_char; BUF];
        let mut result = std::ptr::null_mut();
        // SAFETY: as above.
        let rc =
            unsafe { libc::getgrgid_r(gid, &mut gr, buf.as_mut_ptr(), buf.len(), &mut result) };
        if rc != 0 || result.is_null() || gr.gr_mem.is_null() {
            return Vec::new();
        }
        let mut members = Vec::new();
        let mut i = 0;
        loop {
            // SAFETY: `gr_mem` is a NULL-terminated array into `buf`.
            let member = unsafe { *gr.gr_mem.add(i) };
            if member.is_null() {
                break;
            }
            members.push(
                unsafe { CStr::from_ptr(member) }
                    .to_string_lossy()
                    .into_owned(),
            );
            i += 1;
        }
        members
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(uid: u32) -> (String, HashMap<String, OwnedValue>) {
        let mut d = HashMap::new();
        d.insert("uid".to_string(), OwnedValue::from(uid));
        ("unix-user".to_string(), d)
    }

    #[test]
    fn identities_resolve_to_names_once_each() {
        let root = expand_identities(&[user(0), user(0), ("unix-netgroup".into(), HashMap::new())]);
        assert_eq!(
            root,
            vec![Identity {
                uid: 0,
                name: "root".into()
            }]
        );
        // A uid nobody has is dropped rather than shown as a number.
        assert!(expand_identities(&[user(4_000_000_000)]).is_empty());
    }

    #[test]
    fn pkexec_requests_show_their_command() {
        let mut details = HashMap::new();
        details.insert("program".to_string(), "/usr/bin/true".to_string());
        let mut req = AuthRequest {
            cookie: String::new(),
            action_id: "org.freedesktop.policykit.exec".into(),
            message: String::new(),
            icon_name: String::new(),
            details,
            identities: Vec::new(),
        };
        assert_eq!(req.command(), Some("/usr/bin/true"));
        req.details
            .insert("command_line".into(), "/usr/bin/true --flag".into());
        assert_eq!(req.command(), Some("/usr/bin/true --flag"));

        let me = unsafe { libc::getuid() };
        req.identities = vec![
            Identity {
                uid: 0,
                name: "root".into(),
            },
            Identity {
                uid: me,
                name: "me".into(),
            },
        ];
        assert_eq!(req.preferred_identity(), if me == 0 { 0 } else { 1 });
    }

    #[test]
    fn our_own_start_time_is_read() {
        assert!(process_start_time(std::process::id()).is_some_and(|t| t > 0));
        assert_eq!(process_start_time(u32::MAX), None);
    }
}
