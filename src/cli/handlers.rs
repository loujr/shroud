//! CLI command handlers.
//!
//! Implements the client-side execution of CLI commands by communicating
//! with the daemon over IPC.

use log::{error, info};
use std::io::Write;
use std::path::PathBuf;

use super::args::{Args, DebugAction, ParsedCommand, ToggleAction};
use super::help;
use crate::ipc::client::{send_command, ClientError};
use crate::ipc::protocol::{IpcCommand, IpcResponse};
use crate::logging;

/// Run the CLI in client mode.
///
/// Parses the command from arguments and sends it to the daemon.
///
/// # Returns
///
/// Exit code: 0 for success, non-zero for errors.
pub async fn run_client_mode(args: &Args) -> i32 {
    let command = match &args.command {
        Some(cmd) => cmd,
        None => return 0, // Should likely not happen if parsing enforced it, but explicit check safe
    };

    // Handle local commands that don't need the daemon
    match command {
        ParsedCommand::Update { yes, debug } => {
            return handle_update_command(*yes, *debug).await;
        }
        ParsedCommand::Version { check } => {
            return handle_version_command(*check).await;
        }
        ParsedCommand::Help { command: Some(cmd) } => {
            help::print_command_help(cmd);
            return 0;
        }
        ParsedCommand::Help { command: None } => {
            help::print_main_help();
            return 0;
        }
        ParsedCommand::Debug {
            action: DebugAction::Tail,
        } => {
            // Tail is a local command
            // We assume logging module is available via crate root
            let log_path = logging::log_directory().join("debug.log");
            let status = std::process::Command::new("tail")
                .arg("-f")
                .arg(&log_path)
                .status();
            match status {
                Ok(s) => return s.code().unwrap_or(1),
                Err(e) => {
                    eprintln!("Failed to run tail: {}", e);
                    return 1;
                }
            }
        }
        _ => {}
    }

    // Convert CLI args to IPC command
    let ipc_command = match args_to_command(command) {
        Some(cmd) => cmd,
        None => {
            error!("Invalid command");
            return 1;
        }
    };

    info!("Sending command: {:?}", ipc_command);

    match command {
        ParsedCommand::Restart => match send_command(ipc_command).await {
            Ok(IpcResponse::OkMessage { message }) => {
                if !args.quiet {
                    println!("{}", message);
                }
                std::thread::sleep(std::time::Duration::from_secs(2));
                if !args.quiet {
                    println!("Daemon restarted successfully");
                }
                0
            }
            Ok(IpcResponse::Error { message }) => {
                eprintln!("Error: {}", message);
                1
            }
            Ok(other) => {
                eprintln!("Unexpected response: {:?}", other);
                1
            }
            Err(e) => {
                match e {
                    ClientError::DaemonNotRunning => {
                        eprintln!("Error: Shroud daemon is not running.");
                        eprintln!("Start it with: shroud --daemon");
                    }
                    _ => {
                        eprintln!("Error: {}", e);
                    }
                }
                1
            }
        },
        _ => match send_command(ipc_command).await {
            Ok(response) => handle_response(response, args),
            Err(e) => {
                match e {
                    ClientError::DaemonNotRunning => {
                        eprintln!("Error: Shroud daemon is not running.");
                        eprintln!("Start it with: shroud --daemon");
                        // Special exit code for daemon not running?
                        // Standard practice is 1, but maybe another is better. Sticking to 1.
                    }
                    _ => {
                        eprintln!("Error: {}", e);
                    }
                }
                1
            }
        },
    }
}

/// Convert CLI arguments to an IPC command.
fn args_to_command(cmd: &ParsedCommand) -> Option<IpcCommand> {
    match cmd {
        ParsedCommand::Connect { name } => Some(IpcCommand::Connect { name: name.clone() }),
        ParsedCommand::Disconnect => Some(IpcCommand::Disconnect),
        ParsedCommand::Reconnect => Some(IpcCommand::Reconnect),
        ParsedCommand::Switch { name } => Some(IpcCommand::Switch { name: name.clone() }),
        ParsedCommand::Status => Some(IpcCommand::Status),
        ParsedCommand::List => Some(IpcCommand::List),

        ParsedCommand::KillSwitch { action } => match action {
            ToggleAction::On => Some(IpcCommand::KillSwitch { enable: true }),
            ToggleAction::Off => Some(IpcCommand::KillSwitch { enable: false }),
            ToggleAction::Toggle => Some(IpcCommand::KillSwitchToggle),
            ToggleAction::Status => Some(IpcCommand::KillSwitchStatus),
        },

        ParsedCommand::AutoReconnect { action } => match action {
            ToggleAction::On => Some(IpcCommand::AutoReconnect { enable: true }),
            ToggleAction::Off => Some(IpcCommand::AutoReconnect { enable: false }),
            ToggleAction::Toggle => Some(IpcCommand::AutoReconnectToggle),
            ToggleAction::Status => Some(IpcCommand::AutoReconnectStatus),
        },

        ParsedCommand::Debug { action } => match action {
            DebugAction::On => Some(IpcCommand::Debug { enable: true }),
            DebugAction::Off => Some(IpcCommand::Debug { enable: false }),
            DebugAction::Dump => Some(IpcCommand::DebugDump),
            DebugAction::LogPath => Some(IpcCommand::DebugLogPath),
            DebugAction::Tail => None, // Handled locally
        },

        ParsedCommand::Ping => Some(IpcCommand::Ping),
        ParsedCommand::Refresh => Some(IpcCommand::Refresh),
        ParsedCommand::Quit => Some(IpcCommand::Quit),
        ParsedCommand::Restart => Some(IpcCommand::Restart),
        ParsedCommand::Reload => Some(IpcCommand::Reload),
        ParsedCommand::Update { .. } => None,
        ParsedCommand::Version { .. } => None,

        ParsedCommand::Help { .. } => None, // Handled locally
    }
}

/// Handle and display a response from the daemon.
///
/// Returns exit code: 0 for success, 1 for errors.
fn handle_response(response: IpcResponse, args: &Args) -> i32 {
    let json = args.json_output;

    if json {
        // Just dump the JSON structure of the response
        match serde_json::to_string_pretty(&response) {
            Ok(s) => println!("{}", s),
            Err(e) => {
                eprintln!("Error serializing response: {}", e);
                return 1;
            }
        }
        return if response.is_ok() { 0 } else { 1 };
    }

    match response {
        IpcResponse::Ok => {
            if !args.quiet {
                println!("OK");
            }
            0
        }
        IpcResponse::Error { message } => {
            eprintln!("Error: {}", message);
            1
        }
        IpcResponse::Status {
            connected,
            vpn_name,
            state,
            kill_switch_enabled,
        } => {
            println!("Status: {}", state);
            if connected {
                println!(
                    "Connected to: {}",
                    vpn_name.unwrap_or_else(|| "unknown".to_string())
                );
            } else {
                println!("Not connected");
            }
            println!(
                "Kill switch: {}",
                if kill_switch_enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            0
        }
        IpcResponse::Connections { names } => {
            println!("Available VPN connections:");
            for name in names {
                println!("  - {}", name);
            }
            0
        }
        IpcResponse::OkMessage { message } => {
            println!("{}", message);
            0
        }
        IpcResponse::KillSwitchStatus { enabled } => {
            println!(
                "Kill Switch: {}",
                if enabled { "enabled" } else { "disabled" }
            );
            0
        }
        IpcResponse::AutoReconnectStatus { enabled } => {
            println!(
                "Auto-Reconnect: {}",
                if enabled { "enabled" } else { "disabled" }
            );
            0
        }
        IpcResponse::DebugInfo {
            log_path,
            debug_enabled,
        } => {
            println!("Debug Mode: {}", if debug_enabled { "on" } else { "off" });
            if let Some(path) = log_path {
                println!("Log Path: {}", path);
            }
            0
        }
        IpcResponse::Pong => {
            println!("Pong");
            0
        }
    }
}

async fn handle_update_command(skip_confirm: bool, debug_mode: bool) -> i32 {
    match try_handle_update_command(skip_confirm, debug_mode).await {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Error: {}", e);
            1
        }
    }
}

async fn try_handle_update_command(
    skip_confirm: bool,
    debug_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let project_dir = find_project_directory()?;

    println!("Project directory: {}", project_dir.display());

    let git_status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&project_dir)
        .output()?;

    let has_changes = !git_status.stdout.is_empty();
    if has_changes {
        println!("⚠ Warning: You have uncommitted changes");
        let changes = String::from_utf8_lossy(&git_status.stdout);
        for line in changes.lines().take(5) {
            println!("  {}", line);
        }
        let total = changes.lines().count();
        if total > 5 {
            println!("  ... and {} more", total - 5);
        }
    }

    if !skip_confirm {
        print!("Build and install shroud? [Y/n] ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if !input.is_empty() && input != "y" && input != "yes" {
            println!("Cancelled");
            return Ok(());
        }
    }

    println!("\n📦 Building...");
    let build_args = if debug_mode {
        vec!["build"]
    } else {
        vec!["build", "--release"]
    };

    let build_status = std::process::Command::new("cargo")
        .args(&build_args)
        .current_dir(&project_dir)
        .status()?;

    if !build_status.success() {
        return Err("Build failed".into());
    }
    println!("✓ Build successful");

    println!("\n📥 Installing...");
    let install_status = std::process::Command::new("cargo")
        .args(["install", "--path", ".", "--force"])
        .current_dir(&project_dir)
        .status()?;

    if !install_status.success() {
        return Err("Install failed".into());
    }
    println!("✓ Installed to ~/.cargo/bin/shroud");

    println!("\n🔄 Restarting daemon...");
    match send_command(IpcCommand::Restart).await {
        Ok(IpcResponse::OkMessage { message }) => {
            println!("✓ {}", message);
        }
        Ok(_) | Err(_) => {
            println!("ℹ Daemon not running or already stopped");
        }
    }

    std::thread::sleep(std::time::Duration::from_secs(2));

    let local_path = dirs::home_dir()
        .map(|h| h.join(".local/bin/shroud"))
        .filter(|p| p.exists() || p.parent().map(|p| p.exists()).unwrap_or(false));

    if let Some(local_path) = local_path {
        let cargo_bin = dirs::home_dir()
            .ok_or("Failed to resolve home directory")?
            .join(".cargo/bin/shroud");

        let mut attempts = 0;
        loop {
            match std::fs::copy(&cargo_bin, &local_path) {
                Ok(_) => {
                    println!("✓ Copied to {}", local_path.display());
                    break;
                }
                Err(e) if e.raw_os_error() == Some(26) && attempts < 3 => {
                    attempts += 1;
                    println!(
                        "  Waiting for old process to exit (attempt {}/3)...",
                        attempts
                    );
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
                Err(e) if e.raw_os_error() == Some(26) => {
                    println!("⚠ Could not copy to {}: file busy", local_path.display());
                    println!("  Run manually: cp ~/.cargo/bin/shroud ~/.local/bin/shroud");
                    break;
                }
                Err(e) => {
                    println!("⚠ Could not copy to {}: {}", local_path.display(), e);
                    break;
                }
            }
        }
    }

    println!("\n✅ Update complete!");
    let version_output = std::process::Command::new("shroud")
        .arg("--version")
        .output()?;

    if version_output.status.success() {
        let version = String::from_utf8_lossy(&version_output.stdout);
        println!("   Installed version: {}", version.trim());
    }

    Ok(())
}

async fn handle_version_command(check: bool) -> i32 {
    match try_handle_version_command(check).await {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Error: {}", e);
            1
        }
    }
}

async fn try_handle_version_command(check: bool) -> Result<(), Box<dyn std::error::Error>> {
    let version = env!("CARGO_PKG_VERSION");
    println!("shroud {}", version);

    if check {
        match find_project_directory() {
            Ok(project_dir) => {
                let exe_path = std::env::current_exe()?;
                let exe_mtime = std::fs::metadata(&exe_path)?.modified()?;

                let src_dir = project_dir.join("src");
                let newest_src = walkdir::WalkDir::new(&src_dir)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
                    .filter_map(|e| e.metadata().ok()?.modified().ok())
                    .max();

                let cargo_mtime = std::fs::metadata(project_dir.join("Cargo.toml"))?.modified()?;

                let newest_source = newest_src
                    .map(|src| std::cmp::max(src, cargo_mtime))
                    .unwrap_or(cargo_mtime);

                if newest_source > exe_mtime {
                    println!("\n⚠ Update available: source files are newer than binary");
                    println!("  Run 'shroud update' to rebuild and install");
                } else {
                    println!("\n✓ Binary is up to date with source");
                }
            }
            Err(_) => {
                println!("\nℹ Cannot check for updates: project directory not found");
            }
        }
    }

    if let Ok(response) = send_command(IpcCommand::Ping).await {
        if let IpcResponse::Pong | IpcResponse::Ok = response {
            println!("\n✓ Daemon is running");
        }
    } else {
        println!("\nℹ Daemon is not running");
    }

    Ok(())
}

fn find_project_directory() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(dir) = std::env::var("SHROUD_PROJECT_DIR") {
        let path = PathBuf::from(dir);
        if path.join("Cargo.toml").exists() {
            return Ok(path);
        }
    }

    let current = std::env::current_dir()?;
    if current.join("Cargo.toml").exists() {
        let cargo_toml = std::fs::read_to_string(current.join("Cargo.toml"))?;
        if cargo_toml.contains("name = \"shroud\"") {
            return Ok(current);
        }
    }

    if let Some(home) = dirs::home_dir() {
        let path = home.join("src/shroud");
        if path.join("Cargo.toml").exists() {
            return Ok(path);
        }
    }

    let mut dir = current;
    while let Some(parent) = dir.parent() {
        if parent.join("Cargo.toml").exists() {
            let cargo_toml = std::fs::read_to_string(parent.join("Cargo.toml"))?;
            if cargo_toml.contains("name = \"shroud\"") {
                return Ok(parent.to_path_buf());
            }
        }
        dir = parent.to_path_buf();
    }

    Err("Could not find shroud project directory. Set SHROUD_PROJECT_DIR or run from project directory.".into())
}
