//! ADB integration — device management, APK installation, logcat.

use crate::app::{AppState, LogLevel};
use anyhow::Result;
use std::sync::{Arc, Mutex};

pub struct AdbManager;

impl AdbManager {
    fn pick_install_apk_from_build(build_dir: &std::path::Path) -> Option<std::path::PathBuf> {
        let mut signed_candidates: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
        let mut built_candidates: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();

        let entries = std::fs::read_dir(build_dir).ok()?;
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("apk") {
                continue;
            }

            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let modified = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

            if file_name.contains("signed") || file_name.contains("debugsigned") {
                signed_candidates.push((modified, path.clone()));
            }

            if file_name == "output.apk" || file_name.contains("aligned") {
                built_candidates.push((modified, path.clone()));
            }
        }

        signed_candidates.sort_by_key(|(mtime, _)| *mtime);
        built_candidates.sort_by_key(|(mtime, _)| *mtime);

        signed_candidates
            .last()
            .map(|(_, p)| p.clone())
            .or_else(|| built_candidates.last().map(|(_, p)| p.clone()))
    }

    /// List connected ADB devices.
    pub fn list_devices(state: &Arc<Mutex<AppState>>) -> Result<()> {
        let result = {
            let mut s = state.lock().unwrap();
            s.push_log(LogLevel::Info, "Querying ADB devices...");
            s.toolchain.run_tool_string("adb", &["devices", "-l"])
        };

        match result {
            Ok(output) => {
                let mut s = state.lock().unwrap();
                let lines: Vec<&str> = output.lines().collect();
                let device_count = lines.iter().filter(|l| l.contains("device") && !l.contains("List")).count();
                s.push_log(
                    LogLevel::Info,
                    &format!("Connected devices: {}", device_count),
                );
                for line in &lines {
                    if !line.trim().is_empty() {
                        s.push_log(LogLevel::Debug, line);
                    }
                }
            }
            Err(e) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Error, &format!("ADB not available: {}", e));
                return Err(e);
            }
        }

        Ok(())
    }

    /// Install an APK on the connected device.
    pub fn install_apk(state: &Arc<Mutex<AppState>>) -> Result<()> {
        // Prefer the latest signed artifact from build dir, then fall back.
        let apk_path = {
            let s = state.lock().unwrap();
            let build_dir = s.workspace.build_dir();
            let apk = s.workspace.apk_path().map(|p| p.to_path_buf());

            if let Some(bd) = &build_dir {
                Self::pick_install_apk_from_build(bd).or(apk)
            } else {
                apk
            }
        };

        let apk = apk_path.ok_or_else(|| anyhow::anyhow!("No APK to install"))?;

        {
            let mut s = state.lock().unwrap();
            s.busy = true;
            s.status_message = format!("Installing: {}", apk.display());
            s.push_log(
                LogLevel::Info,
                &format!("Installing APK: {}", apk.display()),
            );
        }

        let result = {
            let s = state.lock().unwrap();
            s.toolchain
                .run_tool_string("adb", &["install", "-r", &apk.to_string_lossy()])
        };

        match result {
            Ok(output) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Info, &format!("Install result: {}", output.trim()));
                s.status_message = "APK installed".into();
                s.busy = false;
            }
            Err(e) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Error, &format!("Install failed: {}", e));
                s.status_message = "Install failed".into();
                s.busy = false;
                return Err(e);
            }
        }

        Ok(())
    }

    /// Start logcat and stream output (future: streaming via async).
    pub fn logcat_snapshot(state: &Arc<Mutex<AppState>>) -> Result<()> {
        let result = {
            let s = state.lock().unwrap();
            s.toolchain
                .run_tool_string("adb", &["logcat", "-d", "-t", "50"])
        };

        match result {
            Ok(output) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Info, "=== Logcat snapshot ===");
                for line in output.lines().take(50) {
                    s.push_log(LogLevel::Debug, line);
                }
            }
            Err(e) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Error, &format!("Logcat failed: {}", e));
            }
        }

        Ok(())
    }

    /// Spawns a background task to stream logcat in real-time.
    pub fn stream_logcat(state: Arc<Mutex<AppState>>) {
        tokio::spawn(async move {
            let (log_tx, toolchain) = {
                let s = state.lock().unwrap();
                (s.log_tx.clone(), s.toolchain.get("adb").cloned())
            };

            let adb_path = match toolchain {
                Some(t) if t.available => t.executable,
                _ => return,
            };

            let mut child = match tokio::process::Command::new(adb_path)
                .args(["logcat", "*:V"])
                .stdout(std::process::Stdio::piped())
                .spawn() {
                    Ok(c) => c,
                    Err(_) => return,
                };

            let Some(stdout) = child.stdout.take() else {
                let _ = log_tx.send(crate::app::LogEntry {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    level: crate::app::LogLevel::Error,
                    message: "[Logcat] Failed to capture adb stdout".to_string(),
                });
                return;
            };
            use tokio::io::AsyncBufReadExt;
            let mut reader = tokio::io::BufReader::new(stdout).lines();

            while let Ok(Some(line)) = reader.next_line().await {
                let level = if line.contains(" E ") || line.contains("Error") {
                    crate::app::LogLevel::Error
                } else if line.contains(" W ") || line.contains("Warn") {
                    crate::app::LogLevel::Warn
                } else {
                    crate::app::LogLevel::Debug
                };

                let _ = log_tx.send(crate::app::LogEntry {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    level,
                    message: format!("[Logcat] {}", line),
                });
            }
        });
    }

    /// List installed packages on device.
    pub fn list_packages(state: &Arc<Mutex<AppState>>, filter: Option<&str>) -> Result<Vec<String>> {
        let mut args = vec!["shell", "pm", "list", "packages"];
        if let Some(f) = filter {
            args.push(f);
        }

        let result = {
            let s = state.lock().unwrap();
            s.toolchain.run_tool_string("adb", &args)
        };

        match result {
            Ok(output) => {
                let packages: Vec<String> = output
                    .lines()
                    .filter_map(|line| line.strip_prefix("package:"))
                    .map(|s| s.trim().to_string())
                    .collect();
                let count = packages.len();
                {
                    let mut s = state.lock().unwrap();
                    s.push_log(LogLevel::Info, &format!("Found {} packages on device", count));
                }
                Ok(packages)
            }
            Err(e) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Error, &format!("Failed to list packages: {}", e));
                Err(e)
            }
        }
    }

    /// Run a shell command on the connected device.
    pub fn shell_command(state: &Arc<Mutex<AppState>>, command: &str) -> Result<String> {
        let result = {
            let mut s = state.lock().unwrap();
            s.push_log(LogLevel::Info, &format!("[ADB Shell] {}", command));
            s.toolchain.run_tool_string("adb", &["shell", command])
        };

        match result {
            Ok(output) => {
                let mut s = state.lock().unwrap();
                for line in output.lines().take(100) {
                    s.push_log(LogLevel::Debug, &format!("[Shell] {}", line));
                }
                Ok(output)
            }
            Err(e) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Error, &format!("Shell command failed: {}", e));
                Err(e)
            }
        }
    }

    /// Uninstall a package from the device.
    pub fn uninstall_package(state: &Arc<Mutex<AppState>>, package: &str) -> Result<()> {
        {
            let mut s = state.lock().unwrap();
            s.push_log(LogLevel::Info, &format!("Uninstalling: {}", package));
        }

        let result = {
            let s = state.lock().unwrap();
            s.toolchain.run_tool_string("adb", &["uninstall", package])
        };

        match result {
            Ok(output) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Info, &format!("Uninstall result: {}", output.trim()));
            }
            Err(e) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Error, &format!("Uninstall failed: {}", e));
                return Err(e);
            }
        }
        Ok(())
    }

    /// Pull a file from the device.
    pub fn pull_file(state: &Arc<Mutex<AppState>>, remote: &str, local: &str) -> Result<()> {
        {
            let mut s = state.lock().unwrap();
            s.push_log(LogLevel::Info, &format!("Pulling: {} -> {}", remote, local));
        }

        let result = {
            let s = state.lock().unwrap();
            s.toolchain.run_tool_string("adb", &["pull", remote, local])
        };

        match result {
            Ok(output) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Info, &format!("Pull complete: {}", output.trim()));
            }
            Err(e) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Error, &format!("Pull failed: {}", e));
                return Err(e);
            }
        }
        Ok(())
    }

    /// Capture a screenshot from the connected device via `adb exec-out screencap -p`.
    /// Saves as a PNG to the system temp directory and opens it with the default OS viewer.
    pub fn capture_screenshot(state: &Arc<Mutex<AppState>>) -> Result<()> {
        {
            state.lock().unwrap().push_log(LogLevel::Info, "[ADB] Capturing screenshot...");
        }

        let output = {
            let s = state.lock().unwrap();
            s.toolchain.run_tool("adb", &["exec-out", "screencap", "-p"])
        }?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("screencap failed: {}", err.trim());
        }

        if output.stdout.is_empty() {
            anyhow::bail!("screencap returned empty output — is a device connected?");
        }

        let out_path = std::env::temp_dir().join("reveng_screenshot.png");
        std::fs::write(&out_path, &output.stdout)
            .map_err(|e| anyhow::anyhow!("Failed to save screenshot: {}", e))?;

        let mut s = state.lock().unwrap();
        s.push_log(LogLevel::Info, &format!("[ADB] Screenshot saved to: {}", out_path.display()));
        drop(s);

        open::that(&out_path)
            .map_err(|e| anyhow::anyhow!("Failed to open screenshot: {}", e))?;

        Ok(())
    }

    /// Push a file to the device.
    pub fn push_file(state: &Arc<Mutex<AppState>>, local: &str, remote: &str) -> Result<()> {
        {
            let mut s = state.lock().unwrap();
            s.push_log(LogLevel::Info, &format!("Pushing: {} -> {}", local, remote));
        }

        let result = {
            let s = state.lock().unwrap();
            s.toolchain.run_tool_string("adb", &["push", local, remote])
        };

        match result {
            Ok(output) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Info, &format!("Push complete: {}", output.trim()));
            }
            Err(e) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Error, &format!("Push failed: {}", e));
                return Err(e);
            }
        }
        Ok(())
    }
}
