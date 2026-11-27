//! A conversation with `polkit-agent-helper-1`, the program that checks the
//! password with PAM and tells polkitd the outcome.
//!
//! Two ways to reach it, the same ones `libpolkit-agent-1` uses:
//!
//! * **Socket** (`/run/polkit/agent-helper.socket`, systemd distributions):
//!   the agent writes `<user>\n<cookie>\n`; the helper learns the caller's
//!   uid from the socket.
//! * **setuid helper** (Void and other distributions without the socket): the
//!   user name is its argument, `<cookie>\n` goes to its stdin.
//!
//! The helper then talks in lines: `PAM_PROMPT_ECHO_OFF <text>` (and `_ON`,
//! `PAM_ERROR_MSG`, `PAM_TEXT_INFO`, the text C-escaped), each prompt answered
//! with one line, and finally `SUCCESS` or `FAILURE`.

use std::path::Path;
use std::process::Stdio;

use futures::channel::mpsc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::task::JoinHandle;

const SOCKET: &str = "/run/polkit/agent-helper.socket";

/// Where distributions install the setuid helper.
const HELPER_PATHS: &[&str] = &[
    "/usr/lib/polkit-1/polkit-agent-helper-1",
    "/usr/libexec/polkit-agent-helper-1",
    "/usr/libexec/polkit-1/polkit-agent-helper-1",
    "/usr/lib/policykit-1/polkit-agent-helper-1",
];

/// One line from the helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperMsg {
    /// PAM asks for something: the password (`echo` false) or a visible
    /// answer such as a one-time code (`echo` true).
    Prompt { text: String, echo: bool },
    /// A message to show as an error (e.g. "Authentication failure").
    Error(String),
    /// A message to show as information.
    Info(String),
    /// The password was right; the helper has told polkitd.
    Success,
    /// Wrong password, or the helper could not run. The session is over.
    Failure,
}

/// A running helper. Dropping it ends the conversation (the helper is
/// killed or the socket closed).
pub struct HelperSession {
    answers: mpsc::UnboundedSender<String>,
    tasks: Vec<JoinHandle<()>>,
}

impl HelperSession {
    /// Start authenticating `user` for `cookie`. Messages arrive on the
    /// receiver; it ends after `Success` or `Failure`. Needs a tokio runtime.
    pub fn start(user: &str, cookie: &str) -> (Self, mpsc::UnboundedReceiver<HelperMsg>) {
        let (events, rx) = mpsc::unbounded();
        let (answers, answers_rx) = mpsc::unbounded();
        let user = user.to_string();
        let cookie = cookie.to_string();
        let task = tokio::spawn(async move {
            if let Err(err) = run(&user, &cookie, events.clone(), answers_rx).await {
                tracing::warn!("polkit helper: {err:#}");
                let _ = events.unbounded_send(HelperMsg::Error(format!("{err:#}")));
                let _ = events.unbounded_send(HelperMsg::Failure);
            }
        });
        (
            Self {
                answers,
                tasks: vec![task],
            },
            rx,
        )
    }

    /// Answer the last prompt.
    pub fn respond(&self, answer: String) {
        let _ = self.answers.unbounded_send(answer);
    }
}

impl Drop for HelperSession {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

async fn run(
    user: &str,
    cookie: &str,
    events: mpsc::UnboundedSender<HelperMsg>,
    answers: mpsc::UnboundedReceiver<String>,
) -> anyhow::Result<()> {
    if Path::new(SOCKET).exists() {
        match tokio::net::UnixStream::connect(SOCKET).await {
            Ok(stream) => {
                let (read, mut write) = stream.into_split();
                write
                    .write_all(format!("{user}\n{cookie}\n").as_bytes())
                    .await?;
                return converse(read, write, events, answers).await;
            }
            Err(err) => {
                tracing::debug!("{SOCKET}: {err}; falling back to the setuid helper");
            }
        }
    }

    let helper = HELPER_PATHS
        .iter()
        .find(|p| Path::new(p).exists())
        .ok_or_else(|| anyhow::anyhow!("polkit-agent-helper-1 is not installed"))?;
    let mut child = tokio::process::Command::new(helper)
        .arg(user)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| anyhow::anyhow!("cannot run {helper}: {err}"))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    // The cookie on stdin, not in argv where other processes could read it.
    stdin.write_all(format!("{cookie}\n").as_bytes()).await?;
    let result = converse(stdout, stdin, events, answers).await;
    let _ = child.wait().await;
    result
}

/// Relay helper lines as messages and answers as lines until the outcome.
async fn converse(
    read: impl AsyncRead + Unpin,
    mut write: impl AsyncWrite + Unpin,
    events: mpsc::UnboundedSender<HelperMsg>,
    mut answers: mpsc::UnboundedReceiver<String>,
) -> anyhow::Result<()> {
    use futures::StreamExt;

    let mut lines = BufReader::new(read).lines();
    loop {
        let Some(line) = lines.next_line().await? else {
            // The helper went away without a verdict.
            let _ = events.unbounded_send(HelperMsg::Failure);
            return Ok(());
        };
        let msg = parse_line(&line);
        let prompt = matches!(msg, HelperMsg::Prompt { .. });
        let done = matches!(msg, HelperMsg::Success | HelperMsg::Failure);
        if events.unbounded_send(msg).is_err() {
            return Ok(());
        }
        if done {
            return Ok(());
        }
        if prompt {
            let Some(mut answer) = answers.next().await else {
                return Ok(());
            };
            // One line per answer: a newline inside would answer the next
            // prompt too.
            answer.retain(|c| c != '\n' && c != '\r');
            answer.push('\n');
            let written = write.write_all(answer.as_bytes()).await;
            wipe(&mut answer);
            written?;
            write.flush().await?;
        }
    }
}

/// Overwrite a secret before its memory is freed.
fn wipe(secret: &mut String) {
    // SAFETY: zero bytes are valid UTF-8.
    unsafe { secret.as_bytes_mut() }.fill(0);
    secret.clear();
}

/// One helper line, the way `libpolkit-agent-1` reads it.
pub fn parse_line(line: &str) -> HelperMsg {
    let line = line.strip_suffix('\n').unwrap_or(line);
    let text = |rest: &str| unescape(rest);
    if let Some(rest) = line.strip_prefix("PAM_PROMPT_ECHO_OFF ") {
        HelperMsg::Prompt {
            text: text(rest),
            echo: false,
        }
    } else if let Some(rest) = line.strip_prefix("PAM_PROMPT_ECHO_ON ") {
        HelperMsg::Prompt {
            text: text(rest),
            echo: true,
        }
    } else if let Some(rest) = line.strip_prefix("PAM_ERROR_MSG ") {
        HelperMsg::Error(text(rest))
    } else if let Some(rest) = line.strip_prefix("PAM_TEXT_INFO ") {
        HelperMsg::Info(text(rest))
    } else if line.starts_with("SUCCESS") {
        HelperMsg::Success
    } else {
        if !line.starts_with("FAILURE") {
            tracing::warn!("Unknown line from the polkit helper: {line:?}");
        }
        HelperMsg::Failure
    }
}

/// Undo `g_strescape`, as `g_strcompress` does: `\n`, `\t`, `\\`, `\"`,
/// octal `\ooo` and the rest; an unknown `\x` is `x`.
pub fn unescape(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        i += 1;
        if b != b'\\' || i == bytes.len() {
            out.push(b);
            continue;
        }
        let c = bytes[i];
        i += 1;
        match c {
            b'0'..=b'7' => {
                let mut value = u32::from(c - b'0');
                let mut digits = 1;
                while digits < 3 && i < bytes.len() && (b'0'..=b'7').contains(&bytes[i]) {
                    value = value * 8 + u32::from(bytes[i] - b'0');
                    i += 1;
                    digits += 1;
                }
                out.push((value & 0xff) as u8);
            }
            b'b' => out.push(0x08),
            b'f' => out.push(0x0c),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(0x0b),
            other => out.push(other),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_lines_are_read_like_libpolkit_agent() {
        assert_eq!(
            parse_line("PAM_PROMPT_ECHO_OFF Password: "),
            HelperMsg::Prompt {
                text: "Password: ".into(),
                echo: false
            }
        );
        assert_eq!(
            parse_line("PAM_PROMPT_ECHO_ON Code:"),
            HelperMsg::Prompt {
                text: "Code:".into(),
                echo: true
            }
        );
        assert_eq!(
            parse_line("PAM_ERROR_MSG Authentication failure"),
            HelperMsg::Error("Authentication failure".into())
        );
        assert_eq!(
            parse_line("PAM_TEXT_INFO Touch the key"),
            HelperMsg::Info("Touch the key".into())
        );
        assert_eq!(parse_line("SUCCESS"), HelperMsg::Success);
        assert_eq!(parse_line("FAILURE"), HelperMsg::Failure);
        assert_eq!(parse_line("garbage"), HelperMsg::Failure);
    }

    #[test]
    fn escaped_text_is_restored() {
        assert_eq!(unescape(r"a\nb\tc"), "a\nb\tc");
        assert_eq!(unescape(r#"say \"hi\" \\ ok"#), "say \"hi\" \\ ok");
        // UTF-8 escaped byte by byte in octal, as g_strescape does.
        assert_eq!(
            unescape(r"\320\237\320\260\321\200\320\276\320\273\321\214"),
            "Пароль"
        );
        assert_eq!(unescape(r"trailing\"), "trailing\\");
    }

    #[tokio::test]
    async fn a_conversation_answers_prompts_until_the_verdict() {
        use futures::StreamExt;

        let (agent_side, helper_side) = tokio::io::duplex(1024);
        let (read, write) = tokio::io::split(agent_side);
        let (events, mut rx) = mpsc::unbounded();
        let (answers, answers_rx) = mpsc::unbounded();
        let agent = tokio::spawn(converse(read, write, events, answers_rx));

        let (helper_read, mut helper_write) = tokio::io::split(helper_side);
        let mut helper_lines = BufReader::new(helper_read).lines();
        helper_write
            .write_all(b"PAM_PROMPT_ECHO_OFF Password: \n")
            .await
            .unwrap();
        assert_eq!(
            rx.next().await,
            Some(HelperMsg::Prompt {
                text: "Password: ".into(),
                echo: false
            })
        );
        answers.unbounded_send("se\ncret".into()).unwrap();
        assert_eq!(
            helper_lines.next_line().await.unwrap().as_deref(),
            Some("secret")
        );
        helper_write.write_all(b"SUCCESS\n").await.unwrap();
        assert_eq!(rx.next().await, Some(HelperMsg::Success));
        agent.await.unwrap().unwrap();
    }
}
