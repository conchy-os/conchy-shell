//! Example: Watching Live Configuration Changes via org.conchy.Config D-Bus
//!
//! Run with:
//!     cargo run --example config_watcher
//!
//! When you change ~/.config/conchy/shell.kdl or run:
//!     conchy-config set shell bar.height 40
//! This example will immediately print the change!

use anyhow::Result;
use conchy_config::ConfigClient;
use futures_util::StreamExt;
use zbus::Connection;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Connecting to D-Bus session bus...");
    let connection = Connection::session().await?;

    let client = ConfigClient::new(&connection).await?;
    println!("Connected to org.conchy.Config service!");

    println!("Querying initial settings for 'shell' channel:");
    match client.list("shell").await {
        Ok(props) => {
            for (key, val) in props {
                println!("  {} = {}", key, val);
            }
        }
        Err(e) => eprintln!("Could not list shell properties: {e}"),
    }

    println!("\nSubscribing to live PropertyChanged signals...");
    let mut stream = client.receive_property_changed().await?;

    while let Some(signal) = stream.next().await {
        if let Ok(args) = signal.args() {
            println!(
                "[SIGNAL] Channel '{}': Key '{}' -> '{}'",
                args.channel, args.key, args.value
            );
        }
    }

    Ok(())
}
