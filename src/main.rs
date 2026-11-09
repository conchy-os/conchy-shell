use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use conchy_shell::{BarModel, ConchyClient, ConfigManager, ShellConfig, ShellState};
use tokio::sync::mpsc;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(
    name = "conchy-shell",
    about = "Desktop shell, status bar, OSD, and control center for Conchy OS",
    version
)]
struct Cli {
    #[arg(short, long, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the background shell daemon (status bar, event listener, D-Bus reactive configs)
    Daemon,
    /// Query current compositor state (workspaces, windows, outputs)
    Status,
    /// Send an action command to the running Conchy WM instance
    Action {
        #[command(subcommand)]
        action: ActionCommand,
    },
}

#[derive(Subcommand)]
enum ActionCommand {
    /// Switch focus to a workspace by index or name
    FocusWorkspace { target: String },
    /// Focus a specific window by ID
    FocusWindow { id: u64 },
    /// Close the currently focused window
    CloseWindow,
    /// Toggle the window overview
    ToggleOverview,
    /// Spawn a program via the compositor
    Spawn { command: Vec<String> },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let level = match cli.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .context("Failed to set tracing subscriber")?;

    match cli.command.unwrap_or(Commands::Daemon) {
        Commands::Daemon => run_daemon().await,
        Commands::Status => run_status().await,
        Commands::Action { action } => run_action(action).await,
    }
}

async fn run_daemon() -> Result<()> {
    info!("Starting Conchy Shell daemon...");

    let client = ConchyClient::new();
    let mut state = ShellState::new();

    // Query initial state
    if let Ok(outputs) = client.get_outputs() {
        info!("Discovered {} connected display output(s)", outputs.len());
        state.outputs = outputs;
    }
    if let Ok(workspaces) = client.get_workspaces() {
        info!("Discovered {} initial workspace(s)", workspaces.len());
        state.workspaces = workspaces;
        state.focused_workspace_id = state
            .workspaces
            .iter()
            .find(|w| w.is_focused)
            .map(|w| w.id);
    }
    if let Ok(windows) = client.get_windows() {
        info!("Discovered {} open window(s)", windows.len());
        for win in windows {
            if win.is_focused {
                state.focused_window_id = Some(win.id);
            }
            state.windows.insert(win.id, win);
        }
    }

    // Initialize D-Bus Config Manager
    let (config_tx, mut config_rx) = mpsc::unbounded_channel::<ShellConfig>();
    let config_mgr = ConfigManager::new(config_tx);
    let mut current_config = config_mgr.current();

    // Spawn config listener in background
    tokio::spawn(async move {
        if let Err(e) = config_mgr.run().await {
            error!("Config manager error: {e}");
        }
    });

    // Subscribe to IPC event stream
    let mut event_rx = client
        .subscribe_events()
        .context("Could not subscribe to Conchy WM event stream")?;

    info!("Conchy Shell daemon is active and listening for events.");

    // Initial bar rendering
    let bar = BarModel::new(current_config.clone(), state.clone());
    bar.print_bar();

    // Main event loop
    loop {
        tokio::select! {
            Some(event) = event_rx.recv() => {
                state.handle_event(event);
                let bar = BarModel::new(current_config.clone(), state.clone());
                bar.print_bar();
            }
            Some(new_cfg) = config_rx.recv() => {
                info!("Shell received live config update: bar_height={}, bg={}", new_cfg.bar_height, new_cfg.bar_background);
                current_config = new_cfg;
                let bar = BarModel::new(current_config.clone(), state.clone());
                bar.print_bar();
            }
            else => {
                info!("Event sources closed; exiting shell daemon");
                break;
            }
        }
    }

    Ok(())
}

async fn run_status() -> Result<()> {
    let client = ConchyClient::new();

    println!("=== Conchy WM Status ===");
    match client.get_outputs() {
        Ok(outputs) => {
            println!("Outputs ({}):", outputs.len());
            for (name, out) in outputs {
                let scale = out.logical.as_ref().map(|l| l.scale).unwrap_or(1.0);
                println!("  - {}: mode {:?}, scale {}", name, out.current_mode, scale);
            }
        }
        Err(e) => println!("Error fetching outputs: {e}"),
    }

    match client.get_workspaces() {
        Ok(workspaces) => {
            println!("\nWorkspaces ({}):", workspaces.len());
            for ws in workspaces {
                let status = if ws.is_focused {
                    "[FOCUSED]"
                } else if ws.is_active {
                    "[ACTIVE]"
                } else {
                    ""
                };
                let urgent = if ws.is_urgent { " !URGENT!" } else { "" };
                println!(
                    "  - #{}: {:?} on {:?} {}{}",
                    ws.idx,
                    ws.name.unwrap_or_else(|| ws.idx.to_string()),
                    ws.output.unwrap_or_default(),
                    status,
                    urgent
                );
            }
        }
        Err(e) => println!("Error fetching workspaces: {e}"),
    }

    match client.get_windows() {
        Ok(windows) => {
            println!("\nWindows ({}):", windows.len());
            for win in windows {
                let focused = if win.is_focused { " [FOCUSED]" } else { "" };
                println!(
                    "  - [{}] app={:?} title={:?}{}",
                    win.id,
                    win.app_id.unwrap_or_default(),
                    win.title.unwrap_or_default(),
                    focused
                );
            }
        }
        Err(e) => println!("Error fetching windows: {e}"),
    }

    Ok(())
}

async fn run_action(action: ActionCommand) -> Result<()> {
    let client = ConchyClient::new();
    match action {
        ActionCommand::FocusWorkspace { target } => {
            client.focus_workspace(&target)?;
            println!("Switched to workspace '{}'", target);
        }
        ActionCommand::FocusWindow { id } => {
            client.focus_window(id)?;
            println!("Focused window ID {}", id);
        }
        ActionCommand::CloseWindow => {
            client.close_window()?;
            println!("Closed active window");
        }
        ActionCommand::ToggleOverview => {
            client.toggle_overview()?;
            println!("Toggled overview mode");
        }
        ActionCommand::Spawn { command } => {
            if command.is_empty() {
                anyhow::bail!("Command cannot be empty");
            }
            client.spawn(command.clone())?;
            println!("Spawned command: {:?}", command);
        }
    }
    Ok(())
}
