//! Example: Dispatching Actions to Conchy WM
//!
//! Run with:
//!     cargo run --example send_action -- <workspace-number>
//!
//! Example:
//!     cargo run --example send_action -- 2

use anyhow::Result;
use conchy_ipc::socket::Socket;
use conchy_ipc::{Action, Request, Response, WorkspaceReferenceArg};
use std::env;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let target = args.get(1).map(|s| s.as_str()).unwrap_or("1");

    println!("Connecting to Conchy WM...");
    let mut socket = Socket::connect()?;

    let reference = if let Ok(idx) = target.parse::<u8>() {
        WorkspaceReferenceArg::Index(idx)
    } else {
        WorkspaceReferenceArg::Name(target.to_string())
    };

    println!("Switching to workspace: {:?}", reference);
    let reply = socket.send(Request::Action(Action::FocusWorkspace { reference }))?;

    match reply {
        Ok(Response::Handled) => println!("Success: Workspace switched!"),
        Ok(resp) => println!("Compositor response: {:?}", resp),
        Err(err) => eprintln!("Error from compositor: {}", err),
    }

    Ok(())
}
