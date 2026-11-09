use anyhow::{Context, Result};
use conchy_config::ConfigClient;
use futures_util::StreamExt;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use zbus::Connection;

/// Shell runtime configuration parameters loaded from ~/.config/conchy/shell.kdl
/// and reactively updated via org.conchy.Config D-Bus signals.
#[derive(Debug, Clone)]
pub struct ShellConfig {
    pub bar_position: String,
    pub bar_height: u32,
    pub bar_background: String,
    pub bar_foreground: String,
    pub accent_color: String,
    pub urgent_color: String,
    pub font_family: String,
    pub font_size: f32,
    pub clock_format: String,
    pub show_battery: bool,
    pub show_cpu: bool,
    pub show_memory: bool,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            bar_position: "top".to_string(),
            bar_height: 32,
            bar_background: "#1a1b26".to_string(),
            bar_foreground: "#c0caf5".to_string(),
            accent_color: "#7aa2f7".to_string(),
            urgent_color: "#f7768e".to_string(),
            font_family: "JetBrains Mono".to_string(),
            font_size: 11.0,
            clock_format: "%Y-%m-%d %H:%M".to_string(),
            show_battery: true,
            show_cpu: true,
            show_memory: true,
        }
    }
}

impl ShellConfig {
    /// Update a specific property key with a new string value.
    pub fn update_key(&mut self, key: &str, value: &str) {
        match key {
            "bar.position" => self.bar_position = value.to_string(),
            "bar.height" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.bar_height = v;
                }
            }
            "bar.background" => self.bar_background = value.to_string(),
            "bar.foreground" => self.bar_foreground = value.to_string(),
            "bar.accent" | "colors.accent" => self.accent_color = value.to_string(),
            "bar.urgent" | "colors.urgent" => self.urgent_color = value.to_string(),
            "font.family" => self.font_family = value.to_string(),
            "font.size" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.font_size = v;
                }
            }
            "clock.format" => self.clock_format = value.to_string(),
            "widgets.battery" => self.show_battery = value == "true",
            "widgets.cpu" => self.show_cpu = value == "true",
            "widgets.memory" => self.show_memory = value == "true",
            other => debug!("Unhandled shell config key: {}", other),
        }
    }
}

/// Dynamic manager that connects to org.conchy.Config D-Bus and keeps ShellConfig up-to-date.
pub struct ConfigManager {
    config: Arc<RwLock<ShellConfig>>,
    notify_tx: mpsc::UnboundedSender<ShellConfig>,
}

impl ConfigManager {
    pub fn new(notify_tx: mpsc::UnboundedSender<ShellConfig>) -> Self {
        Self {
            config: Arc::new(RwLock::new(ShellConfig::default())),
            notify_tx,
        }
    }

    pub fn current(&self) -> ShellConfig {
        self.config.read().unwrap().clone()
    }

    /// Connect to D-Bus, query initial values from the "shell" channel, and listen for PropertyChanged signals.
    pub async fn run(&self) -> Result<()> {
        let connection = Connection::session()
            .await
            .context("Failed to connect to D-Bus session bus")?;

        let client = ConfigClient::new(&connection)
            .await
            .context("Failed to connect to org.conchy.Config D-Bus service")?;

        info!("Connected to org.conchy.Config D-Bus service");

        // Load initial values from "shell" channel
        match client.list("shell").await {
            Ok(props) => {
                info!("Fetched {} initial shell config properties from daemon", props.len());
                let mut cfg = self.config.write().unwrap();
                for (k, v) in props {
                    cfg.update_key(&k, &v);
                }
                let _ = self.notify_tx.send(cfg.clone());
            }
            Err(e) => {
                warn!("Could not fetch initial shell config from D-Bus ({e}); using default values");
            }
        }

        // Subscribe to PropertyChanged signals
        let mut stream = client
            .receive_property_changed()
            .await
            .context("Failed to subscribe to org.conchy.Config PropertyChanged signals")?;

        info!("Listening for live shell configuration updates via D-Bus...");

        while let Some(signal) = stream.next().await {
            let args = match signal.args() {
                Ok(a) => a,
                Err(e) => {
                    error!("Error deserializing PropertyChanged signal: {e}");
                    continue;
                }
            };

            if args.channel == "shell" {
                info!("Shell config updated: {} = {}", args.key, args.value);
                let mut cfg = self.config.write().unwrap();
                cfg.update_key(&args.key, &args.value);
                let _ = self.notify_tx.send(cfg.clone());
            }
        }

        Ok(())
    }
}
