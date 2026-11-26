//! Drive the microphone service by hand, against the running PipeWire.
//!
//! `XDG_CONFIG_HOME=$(mktemp -d) cargo run -p services --example mic_probe`
//!
//! Reads commands from stdin, one per line, and prints the state after
//! each: `on`, `off`, `set <key> <value>` (a key of `mic.kdl`), `preset
//! <name>`, `save <name>`, `delete <name>`, `reset`, `input <source>`,
//! `rescan`, `state`, `quit`. Point `XDG_CONFIG_HOME` at a scratch directory
//! so the user's own settings stay alone.

use std::io::BufRead;
use std::time::Duration;

use services::mic::Param;
use services::{ManagedService, MicCommand, MicSubscriber};

fn show(service: &MicSubscriber) {
    let data = service.get();
    println!(
        "engine={:?} input={:?} enabled={} preset={:?} plugins={:?} missing={:?} inputs={}",
        data.engine,
        data.input,
        data.settings.enabled,
        data.settings.active_preset(),
        data.plugins,
        data.missing_packages(),
        data.inputs.len(),
    );
}

#[tokio::main]
async fn main() {
    let service = MicSubscriber::new();
    service.start();
    tokio::time::sleep(Duration::from_secs(1)).await;
    show(&service);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    while let Some(line) = rx.recv().await {
        let mut words = line.split_whitespace();
        let rest = || line.split_once(' ').map(|(_, r)| r.trim().to_string());
        let command = match words.next() {
            Some("on") => Some(MicCommand::SetEnabled(true)),
            Some("off") => Some(MicCommand::SetEnabled(false)),
            Some("set") => {
                let param = words.next().and_then(Param::from_key);
                let value = words.next().and_then(|w| w.parse().ok());
                param
                    .zip(value)
                    .map(|(param, value)| MicCommand::Set(param, value))
            }
            Some("preset") => rest().map(MicCommand::ApplyPreset),
            Some("save") => rest().map(MicCommand::SavePreset),
            Some("delete") => rest().map(MicCommand::DeletePreset),
            Some("reset") => Some(MicCommand::Reset),
            Some("input") => rest().map(MicCommand::SetInput),
            Some("rescan") => Some(MicCommand::Rescan),
            Some("quit") => break,
            Some("state") | None => None,
            Some(other) => {
                println!("unknown command {other:?}");
                None
            }
        };
        if let Some(command) = command {
            service.dispatch(command);
        }
        tokio::time::sleep(Duration::from_millis(1500)).await;
        show(&service);
    }
    service.stop();
    tokio::time::sleep(Duration::from_millis(300)).await;
}
