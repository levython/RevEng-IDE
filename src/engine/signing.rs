//! APK signing — zipalign + apksigner pipeline.

use crate::app::{AppState, LogLevel};
use anyhow::Result;
use std::sync::{Arc, Mutex};

pub struct SigningManager;

impl SigningManager {
    /// Sign an APK: zipalign then apksigner.
    pub fn sign(state: &Arc<Mutex<AppState>>) -> Result<()> {
        let build_dir = {
            let mut s = state.lock().unwrap();
            s.busy = true;
            s.status_message = "Signing APK...".into();
            s.push_log(LogLevel::Info, "Starting APK signing pipeline...");
            s.workspace
                .build_dir()
                .ok_or_else(|| anyhow::anyhow!("No build directory"))?
        };

        let input_apk = build_dir.join("output.apk");
        let _aligned_apk = build_dir.join("output-aligned.apk");
        let _signed_apk = build_dir.join("output-signed.apk");

        if !input_apk.exists() {
            let mut s = state.lock().unwrap();
            s.push_log(LogLevel::Error, "No built APK found. Run Build first.");
            s.busy = false;
            anyhow::bail!("No built APK found");
        }

        // uber-apk-signer handles both zipalign and signing automatically.
        // It even provides an embedded debug keystore out of the box.
        {
            let mut s = state.lock().unwrap();
            s.push_log(LogLevel::Info, "Running Uber APK Signer (zipalign + apksigner)...");
        }

        let signer_result = {
            let s = state.lock().unwrap();
            s.toolchain.run_tool_string(
                "uber-apk-signer",
                &[
                    "-a",
                    &input_apk.to_string_lossy(),
                    "-o",
                    &build_dir.to_string_lossy(),
                ],
            )
        };

        match signer_result {
            Ok(output) => {
                let mut s = state.lock().unwrap();
                for line in output.lines().take(25) {
                    s.push_log(LogLevel::Debug, line);
                }
                
                // Uber APK Signer usually suffixes output with "-aligned-debugSigned.apk"
                // Let's find what it actually generated to print it.
                if let Ok(entries) = std::fs::read_dir(&build_dir) {
                     for entry in entries.flatten() {
                         let name = entry.file_name().to_string_lossy().to_string();
                         if name.starts_with("output-") && name.ends_with(".apk") && name != "output.apk" {
                              s.push_log(
                                  LogLevel::Info,
                                  &format!("APK signed & aligned successfully: {}", entry.path().display()),
                              );
                              break;
                         }
                     }
                }
                
                s.status_message = "Signing complete".into();
                s.busy = false;
            }
            Err(e) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Error, &format!("Signing pipeline failed: {}", e));
                s.status_message = "Signing failed".into();
                s.busy = false;
                return Err(e);
            }
        }

        Ok(())
    }
}
