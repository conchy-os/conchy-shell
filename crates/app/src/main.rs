//! GPUi Shell - A Wayland status bar built with GPUI.
//!
//! Supports single-instance mode: if the app is already running,
//! subsequent invocations will signal the existing instance to open the launcher.
//!
//! Usage:
//!   gpuishell              - Start the shell or open launcher if already running
//!   gpuishell --input "x"  - Open launcher with prefilled input
//!   gpuishell --validate   - Check the config files and exit

use crate::ipc::IpcSubscriber;
use assets::Assets;
use gpui_platform::application;
use tracing_subscriber::EnvFilter;

mod args;
mod bar;
pub mod calendar;
pub mod config;
pub mod control_center;
pub mod dock;
pub(crate) mod frame;
pub mod icons;
mod ipc;
mod keybinds;
pub mod launcher;
pub mod notification;
pub mod osd;
pub mod shortcuts;
pub mod state;
mod windows;

#[tokio::main]
async fn main() {
    #[cfg(not(target_os = "linux"))]
    compile_error!("This application requires a Linux system with Wayland.");

    // Initialize tracing with RUST_LOG env var support
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Parse command-line arguments
    let args = args::Args::parse();

    // Before the single-instance check: a running shell must not swallow this.
    if args.validate {
        std::process::exit(if config::validate() { 0 } else { 1 });
    }

    // Try to acquire single-instance lock or signal existing instance.
    // Secondary path exits immediately after signaling the primary instance.
    let Some(ipc) = IpcSubscriber::init(&args) else {
        return;
    };

    // Super opens the launcher, out of the box. Only the daemon gets here, so this
    // runs once per session and never from a `toggle-launcher` key press.
    match shortcuts::Store::standard().and_then(|store| shortcuts::ensure_launcher_tap(&store)) {
        Ok(true) => tracing::info!("Added the Super launcher shortcut to shortcuts.kdl"),
        Ok(false) => {}
        Err(err) => tracing::warn!("Could not set up the Super launcher shortcut: {err:#}"),
    }

    // Initialize services (requires async)
    let services = state::init_services()
        .await
        .expect("Failed to initialize services");

    // Create and run the GPUI application
    // gpui ships a no-op client, so `img(url)` (album art) needs a real one.
    let http_client = reqwest_client::ReqwestClient::user_agent("conchy-shell")
        .expect("Failed to build the HTTP client");
    let app = application()
        .with_assets(Assets {})
        .with_http_client(std::sync::Arc::new(http_client));
    app.run(move |cx| {
        config::Config::init(cx);
        // Registers the TextField / Menu keybindings the component set needs.
        ui::init(cx);
        windows::ShellWindows::init(cx);
        state::AppState::init(services, cx);

        // Register keybindings
        keybinds::register(cx);

        bar::init(cx);
        frame::init(cx);
        dock::init(cx);
        notification::init(cx);
        osd::init(cx);
        launcher::init(cx);

        // Now that the notification service is up, say what didn't parse.
        config::report_load_errors(cx);

        ipc.start(cx);
    });
}
