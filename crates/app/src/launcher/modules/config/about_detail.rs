//! What the computer is: read once from /proc and /etc when the settings open.

pub struct SystemInfo {
    pub os_name: String,
    pub kernel: String,
    pub hostname: String,
    pub cpu: String,
    pub memory: String,
    pub uptime: String,
    pub compositor: String,
}

pub fn get_system_info() -> SystemInfo {
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "Linux".to_string());

    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "localhost".to_string());

    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|info| {
            info.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split(':').nth(1))
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "Unknown CPU".to_string());

    let memory = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|info| {
            info.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|kb| kb.parse::<u64>().ok())
                .map(|kb| format!("{:.1} GB", kb as f64 / 1_048_576.0))
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let uptime = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|info| {
            info.split_whitespace()
                .next()
                .and_then(|s| s.parse::<f64>().ok())
                .map(|secs| {
                    let total_mins = (secs / 60.0) as u64;
                    let hours = total_mins / 60;
                    let mins = total_mins % 60;
                    if hours > 0 {
                        format!("{}h {}m", hours, mins)
                    } else {
                        format!("{}m", mins)
                    }
                })
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let os_name = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|content| {
            content.lines().find(|l| l.starts_with("PRETTY_NAME=")).map(|l| {
                l.trim_start_matches("PRETTY_NAME=")
                    .trim_matches('"')
                    .to_string()
            })
        })
        .unwrap_or_else(|| "Conchy OS (Linux)".to_string());

    SystemInfo {
        os_name,
        kernel,
        hostname,
        cpu,
        memory,
        uptime,
        compositor: "Conchy WM / Wayland".to_string(),
    }
}
