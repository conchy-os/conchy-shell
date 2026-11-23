//! Drive the equalizer service by hand, against the running PipeWire.
//!
//! `XDG_CONFIG_HOME=$(mktemp -d) cargo run -p services --example equalizer_probe`
//!
//! Reads commands from stdin, one per line, and prints the state after
//! each: `on`, `off`, `band <i> <db>`, `preamp <db>`, `auto <on|off>`,
//! `preset <name>`, `save <name>`, `delete <name>`, `reset`, `output <sink>`,
//! `state`, `quit`. Point `XDG_CONFIG_HOME` at a scratch directory so the
//! user's own settings stay alone.

use std::io::BufRead;
use std::time::Duration;

use services::{EqualizerCommand, EqualizerSubscriber, ManagedService};

fn show(service: &EqualizerSubscriber) {
    let data = service.get();
    println!(
        "engine={:?} output={:?} enabled={} preset={:?} gains={:?} preamp={} effective={:.2} outputs={}",
        data.engine,
        data.output,
        data.settings.enabled,
        data.settings.active_preset(),
        data.settings.curve.gains,
        data.settings.curve.preamp_db,
        data.settings.effective_preamp_db(),
        data.outputs.len(),
    );
}

#[tokio::main]
async fn main() {
    let service = EqualizerSubscriber::new();
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
            Some("on") => Some(EqualizerCommand::SetEnabled(true)),
            Some("off") => Some(EqualizerCommand::SetEnabled(false)),
            Some("band") => {
                let band = words.next().and_then(|w| w.parse().ok());
                let gain = words.next().and_then(|w| w.parse().ok());
                band.zip(gain)
                    .map(|(band, gain)| EqualizerCommand::SetBand { band, gain })
            }
            Some("preamp") => words
                .next()
                .and_then(|w| w.parse().ok())
                .map(EqualizerCommand::SetPreamp),
            Some("auto") => Some(EqualizerCommand::SetAutoPreamp(words.next() == Some("on"))),
            Some("preset") => rest().map(EqualizerCommand::ApplyPreset),
            Some("save") => rest().map(EqualizerCommand::SavePreset),
            Some("delete") => rest().map(EqualizerCommand::DeletePreset),
            Some("reset") => Some(EqualizerCommand::Reset),
            Some("output") => rest().map(EqualizerCommand::SetOutput),
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
