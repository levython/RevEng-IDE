//! Frida dynamic instrumentation — full implementation.

use crate::app::{AppState, LogEntry, LogLevel};
use anyhow::Result;
use std::io::BufRead;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

/// A process entry returned by frida-ps.
#[derive(Clone, Debug, Default)]
pub struct FridaProcess {
    pub pid: u32,
    pub name: String,
    /// Package identifier (may be empty for system processes).
    pub identifier: String,
}

pub struct FridaManager;

impl FridaManager {
    fn resolved_tool(state: &Arc<Mutex<AppState>>, name: &str) -> Result<std::path::PathBuf> {
        let tool = {
            let s = state.lock().unwrap();
            s.toolchain.get(name).cloned()
        }
        .ok_or_else(|| anyhow::anyhow!("Unknown tool: {}", name))?;

        if !tool.available {
            anyhow::bail!("Tool '{}' is not available", name);
        }

        Ok(tool.executable)
    }

    /// Returns true if the configured `frida` CLI tool is runnable.
    pub fn is_available(state: &Arc<Mutex<AppState>>) -> bool {
        let Ok(frida) = Self::resolved_tool(state, "frida") else {
            return false;
        };
        std::process::Command::new(frida)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// List processes on the connected USB device via `frida-ps -U`.
    pub fn list_processes(state: &Arc<Mutex<AppState>>) -> Result<()> {
        {
            state.lock().unwrap().push_log(LogLevel::Info, "[frida] Querying device processes...");
        }

        let frida_ps = Self::resolved_tool(state, "frida-ps")
            .map_err(|_| anyhow::anyhow!("frida-ps not found — install Frida with: pip install frida-tools"))?;

        let output = std::process::Command::new(frida_ps)
            .args(["-U"])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to start frida-ps: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            let msg = if stderr.is_empty() { stdout.to_string() } else { stderr.to_string() };
            state.lock().unwrap().push_log(LogLevel::Warn, &format!("[frida] frida-ps failed: {}", msg.trim()));
            return Ok(());
        }

        // frida-ps -U output format (2 or 3 columns):
        //  PID  Name
        // ----  ----
        //  123  com.example.app
        // or with -a:
        //  PID  Name             Identifier
        let mut processes = Vec::new();
        for line in stdout.lines().skip(2) {
            let trimmed = line.trim();
            if trimmed.is_empty() { continue; }
            // Split on 2+ consecutive spaces to handle names with single spaces
            let parts: Vec<&str> = trimmed.splitn(2, "  ").collect();
            let pid_str = parts[0].trim();
            let rest = parts.get(1).unwrap_or(&"").trim();
            // rest may be "Name" or "Name   Identifier"
            let (name, identifier) = if let Some(idx) = rest.find("  ") {
                (rest[..idx].trim().to_string(), rest[idx..].trim().to_string())
            } else {
                (rest.to_string(), String::new())
            };
            if let Ok(pid) = pid_str.parse::<u32>() {
                processes.push(FridaProcess { pid, name, identifier });
            }
        }

        let count = processes.len();
        let mut s = state.lock().unwrap();
        s.frida_processes = processes;
        s.push_log(LogLevel::Info, &format!("[frida] Found {} processes on device.", count));
        Ok(())
    }

    /// Push the frida-server binary from `tools/frida/frida-server` to `/data/local/tmp/`.
    pub fn push_server(state: &Arc<Mutex<AppState>>) -> Result<()> {
        let server_path = {
            let s = state.lock().unwrap();
            let tools_dir = s.toolchain.tools_dir().to_path_buf();
            let candidates = [
                tools_dir.join("frida").join("frida-server"),
                tools_dir.join("frida-server"),
            ];
            candidates.into_iter().find(|p| p.exists()).ok_or_else(|| anyhow::anyhow!(
                "frida-server not found — download from github.com/frida/frida/releases \
                 and place at tools/frida/frida-server"
            ))?
        };

        {
            state.lock().unwrap().push_log(
                LogLevel::Info,
                &format!("[frida] Pushing {} …", server_path.display()),
            );
        }

        let adb = Self::resolved_tool(state, "adb")?;

        let push = std::process::Command::new(&adb)
            .args(["push", &server_path.to_string_lossy(), "/data/local/tmp/frida-server"])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to start adb: {}", e))?;

        if !push.status.success() {
            let err = String::from_utf8_lossy(&push.stderr);
            anyhow::bail!("adb push failed: {}", err.trim());
        }

        let chmod = std::process::Command::new(&adb)
            .args(["shell", "chmod", "755", "/data/local/tmp/frida-server"])
            .output()?;

        let mut s = state.lock().unwrap();
        if chmod.status.success() {
            s.push_log(LogLevel::Info, "[frida] frida-server pushed and marked executable.");
        } else {
            s.push_log(LogLevel::Warn, "[frida] Pushed but chmod failed (may still work with su).");
        }
        Ok(())
    }

    /// Start frida-server on the device (requires root).
    pub fn start_server(state: &Arc<Mutex<AppState>>) -> Result<()> {
        {
            state.lock().unwrap().push_log(LogLevel::Info, "[frida] Starting frida-server on device...");
        }

        let adb = Self::resolved_tool(state, "adb")?;

        // Try plain (for rooted devices where shell is already root) and via su
        let plain = std::process::Command::new(&adb)
            .args(["shell", "/data/local/tmp/frida-server &"])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to start adb: {}", e))?;

        if !plain.status.success() {
            // Fallback: attempt su
            let _ = std::process::Command::new(&adb)
                .args(["shell", "su", "-c", "/data/local/tmp/frida-server &"])
                .output();
        }

        state.lock().unwrap().push_log(LogLevel::Info, "[frida] frida-server start command sent.");
        Ok(())
    }

    /// Kill frida-server on the device.
    pub fn kill_server(state: &Arc<Mutex<AppState>>) -> Result<()> {
        {
            state.lock().unwrap().push_log(LogLevel::Info, "[frida] Killing frida-server...");
        }

        let adb = Self::resolved_tool(state, "adb")?;

        let out = std::process::Command::new(adb)
            .args(["shell", "pkill", "-f", "frida-server"])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to start adb: {}", e))?;

        let mut s = state.lock().unwrap();
        if out.status.success() {
            s.push_log(LogLevel::Info, "[frida] frida-server killed.");
        } else {
            s.push_log(LogLevel::Warn, "[frida] frida-server was not running (or kill failed).");
        }
        Ok(())
    }

    /// Attach to or spawn a target process and inject a Frida script.
    ///
    /// Writes the script to a temp file, spawns the `frida` CLI, and streams
    /// all output into the console log via `log_tx`. Returns the OS PID of the
    /// frida child so the caller can kill it later.
    pub fn attach_and_run(
        state: &Arc<Mutex<AppState>>,
        target: String,
        script_content: String,
        spawn: bool,
        log_tx: std::sync::mpsc::Sender<LogEntry>,
    ) -> Result<u32> {
        // Write script to a well-known temp path
        let script_path = std::env::temp_dir().join("reveng_frida_hook.js");
        std::fs::write(&script_path, &script_content)
            .map_err(|e| anyhow::anyhow!("Failed to write temp script: {}", e))?;

        let mode_flag = if spawn { "-f" } else { "-n" };
        let frida = Self::resolved_tool(state, "frida")
            .map_err(|_| anyhow::anyhow!("frida not found — install with: pip install frida-tools"))?;

        {
            let mut s = state.lock().unwrap();
            s.push_log(LogLevel::Info, &format!(
                "[frida] {} '{}' with script ({}B)…",
                if spawn { "Spawning" } else { "Attaching to" },
                target,
                script_content.len(),
            ));
        }

        let mut child = std::process::Command::new(frida)
            .args([
                "-U",
                mode_flag, &target,
                "-l", &script_path.to_string_lossy(),
                "--no-pause",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start frida: {}", e))?;

        let pid = child.id();
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        let state_c = Arc::clone(state);
        let log_tx_err = log_tx.clone();

        std::thread::spawn(move || {
            // Stream stdout
            let out_handle = {
                let tx = log_tx.clone();
                std::thread::spawn(move || {
                    if let Some(pipe) = stdout_pipe {
                        let reader = std::io::BufReader::new(pipe);
                        for line in reader.lines().map_while(Result::ok) {
                            let _ = tx.send(LogEntry {
                                timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
                                level: LogLevel::Debug,
                                message: format!("[frida] {}", line),
                            });
                        }
                    }
                })
            };

            // Stream stderr
            let err_handle = std::thread::spawn(move || {
                if let Some(pipe) = stderr_pipe {
                    let reader = std::io::BufReader::new(pipe);
                    for line in reader.lines().map_while(Result::ok) {
                        let _ = log_tx_err.send(LogEntry {
                            timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
                            level: LogLevel::Warn,
                            message: format!("[frida:err] {}", line),
                        });
                    }
                }
            });

            let _ = child.wait();
            let _ = out_handle.join();
            let _ = err_handle.join();

            // Mark detached when the process exits
            let mut s = state_c.lock().unwrap();
            s.frida_attached = false;
            s.frida_child_pid = None;
            s.push_log(LogLevel::Info, "[frida] Session ended.");
        });

        Ok(pid)
    }

    /// Kill a running frida session by OS process ID.
    pub fn detach(pid: u32, state: &Arc<Mutex<AppState>>) {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .output();
        }
        #[cfg(not(windows))]
        {
            let _ = std::process::Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .output();
        }

        let mut s = state.lock().unwrap();
        s.frida_attached = false;
        s.frida_child_pid = None;
        s.push_log(LogLevel::Info, "[frida] Detached.");
    }

    /// Run `tools/frida/setup_frida.ps1` to download frida-server for the given Android ABI.
    /// Streams script output to the console log in real time.
    pub fn run_setup_script(state: &Arc<Mutex<AppState>>, arch: &str) -> Result<()> {
        let (tools_dir, log_tx) = {
            let s = state.lock().unwrap();
            (s.toolchain.tools_dir().to_path_buf(), s.log_tx.clone())
        };

        let script_path = tools_dir.join("frida").join("setup_frida.ps1");
        let frida_dir   = tools_dir.join("frida");

        if !script_path.exists() {
            anyhow::bail!(
                "Setup script not found at {}\n\
                 Re-clone the repo or download it from the releases page.",
                script_path.display()
            );
        }

        {
            state.lock().unwrap().push_log(
                LogLevel::Info,
                &format!("[frida] Running setup_frida.ps1 for android-{}…", arch),
            );
        }

        let shell = if cfg!(windows) {
            "powershell"
        } else {
            "pwsh"
        };

        let mut child = std::process::Command::new(shell)
            .args([
                "-NoLogo",
                "-ExecutionPolicy", "Bypass",
                "-File", &script_path.to_string_lossy(),
                "-Arch", arch,
                "-ToolsDir", &frida_dir.to_string_lossy(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start {}: {}", shell, e))?;

        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        // Stream stdout
        let out_tx = log_tx.clone();
        let out_handle = std::thread::spawn(move || {
            if let Some(pipe) = stdout_pipe {
                let reader = std::io::BufReader::new(pipe);
                for line in reader.lines().map_while(Result::ok) {
                    // Strip ANSI color codes (PowerShell may emit them)
                    let clean = strip_ansi(&line);
                    let _ = out_tx.send(LogEntry {
                        timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
                        level: LogLevel::Debug,
                        message: format!("[frida-setup] {}", clean),
                    });
                }
            }
        });

        // Stream stderr
        let err_tx = log_tx;
        let err_handle = std::thread::spawn(move || {
            if let Some(pipe) = stderr_pipe {
                let reader = std::io::BufReader::new(pipe);
                for line in reader.lines().map_while(Result::ok) {
                    let clean = strip_ansi(&line);
                    if !clean.trim().is_empty() {
                        let _ = err_tx.send(LogEntry {
                            timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
                            level: LogLevel::Warn,
                            message: format!("[frida-setup:err] {}", clean),
                        });
                    }
                }
            }
        });

        let status = child.wait()?;
        let _ = out_handle.join();
        let _ = err_handle.join();

        let mut s = state.lock().unwrap();
        if status.success() {
            s.push_log(LogLevel::Info, "[frida] Setup complete — frida-server ready to push.");
        } else {
            s.push_log(LogLevel::Warn, &format!("[frida] Setup script exited with {}", status));
        }

        Ok(())
    }
}

/// Remove ANSI escape sequences from a string (e.g., colour codes PowerShell emits).
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Consume the escape sequence: ESC [ ... final-byte
            if chars.peek() == Some(&'[') {
                chars.next();
                for ch in chars.by_ref() {
                    if ch.is_ascii_alphabetic() { break; }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
