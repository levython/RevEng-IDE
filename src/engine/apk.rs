//! APK processor orchestrates open, decode, decompile, build, and sign operations.

use crate::app::{AppState, LogLevel};
use crate::engine::apktool::ApkToolRunner;
use crate::engine::jadx::JadxRunner;

use anyhow::Result;
use std::path::Component;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct ApkProcessor;

impl ApkProcessor {
    const MAX_XAPK_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
    const MAX_XAPK_BASE_APK_BYTES: u64 = 4 * 1024 * 1024 * 1024;
    const MAX_NATIVE_LIB_BYTES: u64 = 512 * 1024 * 1024;

    fn safe_archive_output_path(root: &Path, entry_name: &str) -> Option<PathBuf> {
        let rel = Path::new(entry_name);
        if rel.as_os_str().is_empty() {
            return None;
        }
        if rel.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return None;
        }
        Some(root.join(rel))
    }

    fn safe_xapk_output_name(entry_name: &str) -> Result<String> {
        let path = Path::new(entry_name);
        if path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            anyhow::bail!("Unsafe base APK path in XAPK manifest: {}", entry_name);
        }

        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid base APK name in XAPK manifest"))?;

        if !file_name.to_ascii_lowercase().ends_with(".apk") {
            anyhow::bail!("XAPK base entry is not an APK: {}", entry_name);
        }

        Ok(file_name.to_string())
    }

    fn ensure_archive_entry_size(name: &str, size: u64, limit: u64, label: &str) -> Result<()> {
        if size > limit {
            anyhow::bail!(
                "{} '{}' is too large: {:.1} MB > {:.1} MB",
                label,
                name,
                size as f64 / 1_048_576.0,
                limit as f64 / 1_048_576.0
            );
        }
        Ok(())
    }


    /// Open an APK or XAPK, initialize the workspace, and extract basic info.
    pub fn open_apk(state: &Arc<Mutex<AppState>>, apk_path: &Path) -> Result<()> {
        {
            let mut s = state.lock().unwrap();
            s.busy = true;
            s.status_message = format!("Opening: {}", apk_path.display());
            s.reset_workspace_state();
            s.push_log(LogLevel::Info, &format!("Opening: {}", apk_path.display()));
        }

        let init_result = (|| -> Result<(PathBuf, PathBuf)> {
            let effective_path = Self::resolve_apk_path(state, apk_path)?;

            let ws_root = {
                let mut s = state.lock().unwrap();
                let ws_root = s.workspace.init(&effective_path)?;
                s.recent_apks.retain(|p| p != &effective_path);
                s.recent_apks.insert(0, effective_path.clone());
                s.recent_apks.truncate(10);
                ws_root
            };

            Ok((effective_path, ws_root))
        })();

        let (effective_path, ws_root) = match init_result {
            Ok(paths) => paths,
            Err(e) => {
                let mut s = state.lock().unwrap();
                s.busy = false;
                s.status_message = "APK open failed".into();
                s.push_log(LogLevel::Error, &format!("APK open failed: {}", e));
                return Err(e);
            }
        };

        {
            let mut s = state.lock().unwrap();
            s.push_log(
                LogLevel::Info,
                &format!("Workspace created: {}", ws_root.display()),
            );
        }

        // Offload heavy index and extraction tasks to background thread
        let state_c = Arc::clone(state);
        let effective_path_c = effective_path.clone();
        std::thread::spawn(move || {
            let mut failures = Vec::new();

            if let Err(e) = Self::extract_apk_info(&state_c, &effective_path_c) {
                failures.push(format!("APK metadata: {}", e));
            }
            if let Err(e) = Self::extract_native_libs(&state_c, &effective_path_c) {
                failures.push(format!("native library scan: {}", e));
            }
            Self::hydrate_existing_workspace(&state_c);

            if let Err(e) = Self::decode_apk(&state_c) {
                failures.push(format!("APKTool decode: {}", e));
            }
            if let Err(e) = Self::decompile_apk(&state_c) {
                failures.push(format!("JADX decompile: {}", e));
            }

            let mut s = state_c.lock().unwrap();
            s.busy = false;
            if failures.is_empty() {
                s.status_message = "APK fully decoded and decompiled".into();
                s.push_log(LogLevel::Info, "APK opened, decoded, and decompiled automatically.");
            } else {
                s.status_message = format!("APK opened with {} issue(s)", failures.len());
                for failure in failures {
                    s.push_log(LogLevel::Warn, &format!("Open workflow issue: {}", failure));
                }
            }
        });

        Ok(())
    }

    fn hydrate_existing_workspace(state: &Arc<Mutex<AppState>>) {
        let (smali_dir_count, has_java_sources) = {
            let s = state.lock().unwrap();
            let smali_dir_count = s.decoded_smali_dir_count();
            let has_java_sources = s
                .workspace
                .decompiled_dir()
                .map(|dir| dir.join("sources"))
                .is_some_and(|dir| dir.exists());
            (smali_dir_count, has_java_sources)
        };

        if smali_dir_count > 0 {
            {
                let mut s = state.lock().unwrap();
                s.push_log(
                    LogLevel::Info,
                    &format!(
                        "Existing decoded workspace detected — rebuilding indexes from {} smali directories...",
                        smali_dir_count
                    ),
                );
            }
            let _ = Self::build_nav_index(state);
            let _ = Self::build_xref_db(state);
            Self::extract_strings(state);
            Self::analyze_manifest(state);
            Self::compute_dex_stats(state);
        }

        if has_java_sources {
            let mut s = state.lock().unwrap();
            s.push_log(LogLevel::Info, "Existing JADX output detected in workspace.");
        }
    }

    /// Returns the effective APK path to work with.
    fn resolve_apk_path(state: &Arc<Mutex<AppState>>, path: &Path) -> Result<PathBuf> {
        let is_xapk = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("xapk"))
            .unwrap_or(false);

        if is_xapk {
            Self::extract_base_from_xapk(state, path)
        } else {
            Ok(path.to_path_buf())
        }
    }

    /// Extract the base APK from an XAPK archive.
    fn extract_base_from_xapk(
        state: &Arc<Mutex<AppState>>,
        xapk_path: &Path,
    ) -> Result<PathBuf> {
        use std::io::Read;

        let xapk_stem = xapk_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("xapk");

        let file = std::fs::File::open(xapk_path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        let manifest_contents = {
            match archive.by_name("manifest.json") {
                Ok(mut entry) => {
                    Self::ensure_archive_entry_size(
                        "manifest.json",
                        entry.size(),
                        Self::MAX_XAPK_MANIFEST_BYTES,
                        "XAPK manifest",
                    )?;
                    let mut contents = String::new();
                    entry.read_to_string(&mut contents)?;
                    Some(contents)
                }
                Err(_) => None,
            }
        };

        let base_apk_name = if let Some(contents) = manifest_contents {
            let json: serde_json::Value =
                serde_json::from_str(&contents).unwrap_or(serde_json::Value::Null);

            json["split_apks"]
                .as_array()
                .and_then(|arr| {
                    arr.iter()
                        .find(|e| e["id"].as_str() == Some("base"))
                        .and_then(|e| e["file"].as_str().map(String::from))
                })
                .or_else(|| {
                    json["apk_list"].as_array().and_then(|arr| {
                        arr.iter()
                            .find(|e| {
                                matches!(e["id"].as_str(), Some("base") | Some("master"))
                            })
                            .and_then(|e| e["file"].as_str().map(String::from))
                    })
                })
                .unwrap_or_else(|| "base.apk".to_string())
        } else {
            let mut apk_name = None;
            let entry_count = archive.len();

            for i in 0..entry_count {
                if let Ok(entry) = archive.by_index(i) {
                    let name = entry.name().to_string();
                    if name.ends_with(".apk") {
                        apk_name = Some(name);
                        break;
                    }
                }
            }

            apk_name.unwrap_or_else(|| "base.apk".to_string())
        };

        {
            let mut s = state.lock().unwrap();
            s.push_log(
                LogLevel::Info,
                &format!("XAPK: extracting base APK '{}'", base_apk_name),
            );
        }

        let out_dir = std::env::temp_dir().join(format!("reveng_xapk_{}", xapk_stem));
        std::fs::create_dir_all(&out_dir)?;
        let out_path = out_dir.join(Self::safe_xapk_output_name(&base_apk_name)?);

        let file2 = std::fs::File::open(xapk_path)?;
        let mut archive2 = zip::ZipArchive::new(file2)?;
        let mut entry = archive2.by_name(&base_apk_name).map_err(|_| {
            anyhow::anyhow!(
                "Base APK '{}' not found inside XAPK. The file may be corrupted or unsupported.",
                base_apk_name
            )
        })?;
        Self::ensure_archive_entry_size(
            &base_apk_name,
            entry.size(),
            Self::MAX_XAPK_BASE_APK_BYTES,
            "XAPK base APK",
        )?;

        let mut outfile = std::fs::File::create(&out_path)?;
        std::io::copy(&mut entry, &mut outfile)?;

        {
            let mut s = state.lock().unwrap();
            s.push_log(
                LogLevel::Info,
                &format!("XAPK: base APK extracted to {}", out_path.display()),
            );
        }

        Ok(out_path)
    }

    /// Extract basic info from an APK, which is a ZIP archive.
    fn extract_apk_info(state: &Arc<Mutex<AppState>>, apk_path: &Path) -> Result<()> {
        let file = std::fs::File::open(apk_path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        let entry_count = archive.len();
        let mut dex_count = 0;
        let mut so_count = 0;
        let mut total_size: u64 = 0;

        for i in 0..archive.len() {
            if let Ok(entry) = archive.by_index(i) {
                total_size += entry.size();
                let name = entry.name().to_string();
                if name.ends_with(".dex") {
                    dex_count += 1;
                }
                if name.ends_with(".so") {
                    so_count += 1;
                }
            }
        }

        let mut s = state.lock().unwrap();
        s.push_log(
            LogLevel::Info,
            &format!(
                "APK entries: {} | DEX files: {} | Native libs: {} | Uncompressed size: {:.1} MB",
                entry_count,
                dex_count,
                so_count,
                total_size as f64 / 1_048_576.0
            ),
        );

        Ok(())
    }

    /// Extract native libraries from the APK into workspace/native.
    fn extract_native_libs(state: &Arc<Mutex<AppState>>, apk_path: &Path) -> Result<()> {
        let native_dir = {
            let s = state.lock().unwrap();
            match s.workspace.native_dir() {
                Some(dir) => dir,
                None => return Ok(()),
            }
        };

        let file = std::fs::File::open(apk_path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let mut extracted = 0;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_string();

            if name.ends_with(".so") {
                if let Err(err) = Self::ensure_archive_entry_size(
                    &name,
                    entry.size(),
                    Self::MAX_NATIVE_LIB_BYTES,
                    "Native library",
                ) {
                    let mut s = state.lock().unwrap();
                    s.push_log(LogLevel::Warn, &format!("{}", err));
                    continue;
                }
                let Some(out_path) = Self::safe_archive_output_path(&native_dir, &name) else {
                    let mut s = state.lock().unwrap();
                    s.push_log(
                        LogLevel::Warn,
                        &format!("Skipped unsafe native library path in APK: {}", name),
                    );
                    continue;
                };
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut outfile = std::fs::File::create(&out_path)?;
                std::io::copy(&mut entry, &mut outfile)?;
                extracted += 1;
            }
        }

        if extracted > 0 {
            let mut s = state.lock().unwrap();
            s.push_log(
                LogLevel::Info,
                &format!("Extracted {} native libraries", extracted),
            );
        }

        // ── Flutter detection ──────────────────────────────────────────────
        {
            use crate::native::flutter_patch::FlutterPatcher;
            let flutter_libs = FlutterPatcher::detect_flutter(&native_dir);
            let libapp = FlutterPatcher::find_libapp(&native_dir);
            let all_libs = FlutterPatcher::all_native_libs(&native_dir);
            let flutter_version = flutter_libs.first()
                .and_then(|p| FlutterPatcher::extract_version_from_lib(p));
            let is_flutter = !flutter_libs.is_empty();

            let mut s = state.lock().unwrap();
            s.is_flutter_app = is_flutter;
            s.flutter_lib_paths = flutter_libs;
            s.libapp_path = libapp;
            s.flutter_version = flutter_version;
            s.native_lib_paths = all_libs;

            if s.is_flutter_app {
                    let ver_str = s.flutter_version.clone().unwrap_or_else(|| "unknown version".to_string());
                    s.push_log(
                        LogLevel::Info,
                        &format!("Flutter app detected ✓ (engine: {})", ver_str),
                    );
                s.push_log(
                    LogLevel::Info,
                    "Tip: Use Runtime → Frida SSL bypass, or open libflutter.so → Auto-Patch Flutter",
                );
            } else {
                s.push_log(LogLevel::Debug, "No Flutter runtime detected");
            }
        }

        Ok(())
    }

    /// Decode APK using APKTool.
    pub fn decode_apk(state: &Arc<Mutex<AppState>>) -> Result<()> {
        let (apk_path, decoded_dir, toolchain, log_tx) = {
            let mut s = state.lock().unwrap();
            let apk = s
                .workspace
                .apk_path()
                .ok_or_else(|| anyhow::anyhow!("No APK loaded"))?
                .to_path_buf();
            let decoded = s
                .workspace
                .decoded_dir()
                .ok_or_else(|| anyhow::anyhow!("No workspace"))?;
            s.busy = true;
            s.status_message = "Decoding APK with APKTool...".into();
            s.push_log(LogLevel::Info, "Starting APKTool decode...");
            (apk, decoded, s.toolchain.clone(), s.log_tx.clone())
        };

        let apk_arg = apk_path.to_string_lossy().into_owned();
        let decoded_arg = decoded_dir.to_string_lossy().into_owned();
        let args = ApkToolRunner::decode_args(&apk_arg, &decoded_arg);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

        let result = toolchain.run_tool_streaming("apktool", &arg_refs, &log_tx, "[APKTool]");

        match result {
            Ok(output) => {
                if let Err(e) = ApkToolRunner::validate_decoded(&decoded_dir) {
                    let mut s = state.lock().unwrap();
                    s.push_log(LogLevel::Error, &format!("APKTool decode validation failed: {}", e));
                    s.status_message = "Decode validation failed".into();
                    s.busy = false;
                    return Err(e);
                }

                let mut s = state.lock().unwrap();
                for line in output.lines().take(20) {
                    s.push_log(LogLevel::Debug, line);
                }
                s.push_log(LogLevel::Info, "APKTool decode completed.");
                s.status_message = "Decode complete".into();
                s.busy = false;
                drop(s);
                let _ = Self::build_nav_index(state);
                let _ = Self::build_xref_db(state);
                Self::extract_strings(state);
                Self::analyze_manifest(state);
                Self::compute_dex_stats(state);
            }
            Err(e) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Error, &format!("APKTool decode failed: {}", e));
                s.status_message = "Decode failed".into();
                s.busy = false;
                return Err(e);
            }
        }

        Ok(())
    }

    /// Decompile APK using JADX.
    pub fn decompile_apk(state: &Arc<Mutex<AppState>>) -> Result<()> {
        let (apk_path, decompiled_dir, toolchain, log_tx) = {
            let mut s = state.lock().unwrap();
            let apk = s
                .workspace
                .apk_path()
                .ok_or_else(|| anyhow::anyhow!("No APK loaded"))?
                .to_path_buf();
            let decompiled = s
                .workspace
                .decompiled_dir()
                .ok_or_else(|| anyhow::anyhow!("No workspace"))?;
            s.busy = true;
            s.status_message = "Decompiling with JADX...".into();
            s.push_log(LogLevel::Info, "Starting JADX decompilation...");
            (apk, decompiled, s.toolchain.clone(), s.log_tx.clone())
        };

        let apk_arg = apk_path.to_string_lossy().into_owned();
        let decompiled_arg = decompiled_dir.to_string_lossy().into_owned();
        let mut args = JadxRunner::decompile_args(&apk_arg, &decompiled_arg);
        args.extend(["--threads-count".to_string(), "4".to_string()]);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

        let result = toolchain.run_tool_streaming("jadx", &arg_refs, &log_tx, "[JADX]");

        match result {
            Ok(output) => {
                if let Err(e) = JadxRunner::validate_decompiled(&decompiled_dir) {
                    let mut s = state.lock().unwrap();
                    s.push_log(LogLevel::Error, &format!("JADX output validation failed: {}", e));
                    s.status_message = "Decompilation validation failed".into();
                    s.busy = false;
                    return Err(e);
                }

                let mut s = state.lock().unwrap();
                for line in output.lines().take(20) {
                    s.push_log(LogLevel::Debug, line);
                }
                s.push_log(LogLevel::Info, "JADX decompilation completed.");
                s.status_message = "Decompilation complete".into();
                s.busy = false;
                drop(s);
                let _ = Self::build_nav_index(state);
            }
            Err(e) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Error, &format!("JADX failed: {}", e));
                s.status_message = "Decompilation failed".into();
                s.busy = false;
                return Err(e);
            }
        }

        Ok(())
    }

    /// Build the Java-to-Smali navigation index from the current workspace.
    pub fn build_nav_index(state: &Arc<Mutex<AppState>>) -> Result<()> {
        let decoded_root = {
            let s = state.lock().unwrap();
            s.workspace.decoded_dir()
        };

        if let Some(root) = decoded_root {
            let smali_dir_count = std::fs::read_dir(&root)
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry.path().is_dir()
                        && entry
                            .file_name()
                            .to_str()
                            .map(|name| name.starts_with("smali"))
                            .unwrap_or(false)
                })
                .count();

            if smali_dir_count > 0 {
                {
                    let mut s = state.lock().unwrap();
                    s.push_log(
                        LogLevel::Info,
                        &format!(
                            "Indexing Smali for navigation across {} smali directories...",
                            smali_dir_count
                        ),
                    );
                }

                let index = crate::engine::navigation::NavIndexer::index_workspace(&root);
                let count = index.mappings.len();
                let class_count = index.class_mappings.len();

                {
                    let mut s = state.lock().unwrap();
                    s.nav_index = index;
                    s.push_log(
                        LogLevel::Info,
                        &format!(
                            "Indexed {} Java-to-Smali mappings ({} class fallbacks).",
                            count, class_count
                        ),
                    );
                }
            } else {
                let mut s = state.lock().unwrap();
                s.nav_index = crate::engine::navigation::NavIndex::default();
            }
        }

        Ok(())
    }

    /// Build APK using APKTool.
    pub fn build_apk(state: &Arc<Mutex<AppState>>) -> Result<()> {
        let (decoded_dir, build_dir, toolchain, log_tx) = {
            let mut s = state.lock().unwrap();
            let decoded = s
                .workspace
                .decoded_dir()
                .ok_or_else(|| anyhow::anyhow!("No decoded sources"))?;
            let build = s
                .workspace
                .build_dir()
                .ok_or_else(|| anyhow::anyhow!("No workspace"))?;
            s.busy = true;
            s.status_message = "Building APK...".into();
            s.push_log(LogLevel::Info, "Starting APK build...");
            (decoded, build, s.toolchain.clone(), s.log_tx.clone())
        };

        let output_apk = build_dir.join("output.apk");

        let decoded_arg = decoded_dir.to_string_lossy().into_owned();
        let output_arg = output_apk.to_string_lossy().into_owned();
        let args = ApkToolRunner::build_args(&decoded_arg, &output_arg);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

        let result = toolchain.run_tool_streaming("apktool", &arg_refs, &log_tx, "[APKTool]");

        match result {
            Ok(output) => {
                let mut s = state.lock().unwrap();
                for line in output.lines().take(20) {
                    s.push_log(LogLevel::Debug, line);
                }
                s.push_log(LogLevel::Info, &format!("APK built: {}", output_apk.display()));
                s.status_message = "Build complete".into();
                s.busy = false;
            }
            Err(e) => {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Error, &format!("Build failed: {}", e));
                s.status_message = "Build failed".into();
                s.busy = false;
                return Err(e);
            }
        }

        Ok(())
    }

    /// Sign APK using the signing manager.
    pub fn sign_apk(state: &Arc<Mutex<AppState>>) -> Result<()> {
        use crate::engine::signing::SigningManager;
        SigningManager::sign(state)
    }

    /// Build the cross-reference database from decoded smali files.
    pub fn build_xref_db(state: &Arc<Mutex<AppState>>) -> Result<()> {
        let decoded_dir = {
            let s = state.lock().unwrap();
            s.workspace.decoded_dir()
        };

        if let Some(decoded) = decoded_dir {
            // Scan all smali directories (smali, smali_classes2, smali_classes3, ...)
            let smali_dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&decoded)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let name = e.file_name();
                    let name_str = name.to_string_lossy();
                    name_str.starts_with("smali") && e.path().is_dir()
                })
                .map(|e| e.path())
                .collect();

            if smali_dirs.is_empty() {
                return Ok(());
            }

            {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Info, &format!("Building cross-reference index ({} smali dirs)...", smali_dirs.len()));
            }

            // Build xref from the primary smali directory (index all of them by
            // indexing the parent decoded dir — XrefIndexer walks recursively)
            let db = crate::engine::xref::XrefIndexer::index_workspace(&decoded);

            let file_count = db.file_count;
            let method_count = db.method_callers.len();
            let class_count = db.classes.len();
            let string_count = db.string_refs.len();

            {
                let mut s = state.lock().unwrap();
                s.push_log(
                    LogLevel::Info,
                    &format!(
                        "Xref index: {} files, {} classes, {} method refs, {} unique strings",
                        file_count, class_count, method_count, string_count
                    ),
                );
                s.xref_db = Some(db);
            }
        }

        Ok(())
    }

    /// Extract and categorize strings from decoded APK.
    pub fn extract_strings(state: &Arc<Mutex<AppState>>) {
        let decoded_dir = {
            let s = state.lock().unwrap();
            s.workspace.decoded_dir()
        };

        if let Some(decoded) = decoded_dir {
            if !decoded.exists() {
                return;
            }

            {
                let mut s = state.lock().unwrap();
                s.push_log(LogLevel::Info, "Extracting strings from smali + resources...");
            }

            let strings = crate::engine::strings::StringExtractor::extract_all(&decoded);
            let total = strings.len();
            let interesting = strings
                .iter()
                .filter(|s| s.category != crate::engine::strings::StringCategory::Other)
                .count();

            {
                let mut s = state.lock().unwrap();
                s.push_log(
                    LogLevel::Info,
                    &format!(
                        "Extracted {} strings ({} interesting: URLs, keys, secrets, IPs, etc.)",
                        total, interesting
                    ),
                );
                s.extracted_strings = strings.clone();
                s.strings_view_cache = Some(std::sync::Arc::new(strings));
                s.strings_revision = s.strings_revision.wrapping_add(1);
            }
        }
    }

    /// Parse and analyze AndroidManifest.xml from decoded directory.
    pub fn analyze_manifest(state: &Arc<Mutex<AppState>>) {
        let decoded_dir = {
            let s = state.lock().unwrap();
            s.workspace.decoded_dir()
        };

        if let Some(decoded) = decoded_dir {
            let manifest_path = decoded.join("AndroidManifest.xml");
            if !manifest_path.exists() {
                return;
            }

            match crate::engine::manifest::ManifestAnalyzer::analyze(&decoded) {
                Ok(info) => {
                    let mut s = state.lock().unwrap();
                    let warn_count = info.warnings.len();
                    let perm_count = info.permissions.len();
                    let dangerous = info.permissions.iter()
                        .filter(|p| p.risk == crate::engine::manifest::PermissionRisk::Dangerous)
                        .count();
                    s.push_log(
                        LogLevel::Info,
                        &format!(
                            "Manifest: {} v{} | {} permissions ({} dangerous) | {} warnings",
                            info.package, info.version_name, perm_count, dangerous, warn_count
                        ),
                    );
                    s.manifest_info = Some(info);
                }
                Err(e) => {
                    let mut s = state.lock().unwrap();
                    s.push_log(LogLevel::Warn, &format!("Manifest analysis failed: {}", e));
                }
            }
        }
    }

    /// Compute DEX statistics from decoded smali directories.
    pub fn compute_dex_stats(state: &Arc<Mutex<AppState>>) {
        let decoded_dir = {
            let s = state.lock().unwrap();
            s.workspace.decoded_dir()
        };

        if let Some(decoded) = decoded_dir {
            if !decoded.exists() {
                return;
            }

            let stats = crate::engine::dex_stats::DexAnalyzer::analyze(&decoded);

            let mut s = state.lock().unwrap();
            s.push_log(
                LogLevel::Info,
                &format!(
                    "DEX stats: {} classes, {} methods, {} fields | {:.0}% of 64K limit | obfuscation: {:.0}%",
                    stats.total_classes, stats.total_methods, stats.total_fields,
                    stats.max_method_pct(), stats.obfuscation_score
                ),
            );
            s.dex_stats = Some(stats);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ApkProcessor;
    use crate::app::AppState;
    use std::sync::{Arc, Mutex};

    #[test]
    fn pipeline_precondition_errors_do_not_leave_ui_busy() {
        let state = Arc::new(Mutex::new(AppState::new()));

        assert!(ApkProcessor::decode_apk(&state).is_err());
        assert!(!state.lock().unwrap().busy);

        assert!(ApkProcessor::decompile_apk(&state).is_err());
        assert!(!state.lock().unwrap().busy);

        assert!(ApkProcessor::build_apk(&state).is_err());
        assert!(!state.lock().unwrap().busy);
    }

    #[test]
    fn open_apk_resolution_errors_do_not_leave_ui_busy() {
        let state = Arc::new(Mutex::new(AppState::new()));
        let missing_xapk = std::env::temp_dir().join("reveng_missing_input.xapk");

        assert!(ApkProcessor::open_apk(&state, &missing_xapk).is_err());
        let s = state.lock().unwrap();
        assert!(!s.busy);
        assert_eq!(s.status_message, "APK open failed");
    }

    #[test]
    fn archive_output_paths_reject_traversal() {
        let root = std::path::Path::new("/workspace/native");

        assert_eq!(
            ApkProcessor::safe_archive_output_path(root, "lib/arm64-v8a/libx.so"),
            Some(root.join("lib/arm64-v8a/libx.so"))
        );
        assert!(ApkProcessor::safe_archive_output_path(root, "../escape.so").is_none());
        assert!(ApkProcessor::safe_archive_output_path(root, "lib/../../escape.so").is_none());
        assert!(ApkProcessor::safe_archive_output_path(root, "/tmp/escape.so").is_none());
    }

    #[test]
    fn xapk_output_name_rejects_unsafe_paths() {
        assert_eq!(
            ApkProcessor::safe_xapk_output_name("splits/base.apk").unwrap(),
            "base.apk"
        );
        assert!(ApkProcessor::safe_xapk_output_name("../base.apk").is_err());
        assert!(ApkProcessor::safe_xapk_output_name("/tmp/base.apk").is_err());
        assert!(ApkProcessor::safe_xapk_output_name("base.txt").is_err());
    }

    #[test]
    fn archive_entry_size_guard_rejects_oversized_entries() {
        assert!(ApkProcessor::ensure_archive_entry_size("classes.dex", 10, 10, "entry").is_ok());
        let err = ApkProcessor::ensure_archive_entry_size("classes.dex", 11, 10, "entry")
            .unwrap_err()
            .to_string();
        assert!(err.contains("too large"));
        assert!(err.contains("classes.dex"));
    }
}
