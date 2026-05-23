//! Advanced reverse-engineering feature pack used by App Studio.

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use walkdir::WalkDir;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    pub pattern: String,
    #[serde(default)]
    pub extensions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginDescriptor {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum PatchRecipe {
    DisableRootChecks,
    BypassSslPinning,
    ForceDebuggable,
}

pub struct Arsenal;

impl Arsenal {
    pub fn build_smali_call_graph(decoded_root: &Path) -> Result<(PathBuf, usize)> {
        let class_re = Regex::new(r"^\s*\.class\s+.*L([^;]+);")?;
        let method_re = Regex::new(r"^\s*\.method\s+.*\s([^\(\s]+)\(")?;
        let invoke_re = Regex::new(r"invoke-[^\s]+\s+\{[^\}]*\},\s*L([^;]+);->([^\(]+)\(")?;

        let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        for entry in WalkDir::new(decoded_root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("smali") {
                continue;
            }

            let content = std::fs::read_to_string(path).unwrap_or_default();
            let mut current_class = String::new();
            let mut current_method = String::new();

            for line in content.lines() {
                let t = line.trim();
                if let Some(c) = class_re.captures(t) {
                    current_class = c[1].replace('/', ".");
                    continue;
                }
                if let Some(c) = method_re.captures(t) {
                    current_method = c[1].to_string();
                    continue;
                }
                if let Some(c) = invoke_re.captures(t) {
                    if current_class.is_empty() || current_method.is_empty() {
                        continue;
                    }
                    let caller = format!("{}::{}", current_class, current_method);
                    let callee = format!("{}::{}", c[1].replace('/', "."), &c[2]);
                    graph.entry(caller).or_default().insert(callee);
                }
            }
        }

        let analysis_dir = decoded_root.join("analysis");
        std::fs::create_dir_all(&analysis_dir)?;
        let out = analysis_dir.join("smali_call_graph.json");
        let json = serde_json::to_string_pretty(&graph)?;
        std::fs::write(&out, json)?;

        Ok((out, graph.len()))
    }

    pub fn detect_api_abuse(decoded_root: &Path) -> Vec<String> {
        let patterns = [
            ("Reflection", r"Ljava/lang/reflect/|Class;->forName|Method;->invoke"),
            ("DynamicCode", r"DexClassLoader|PathClassLoader|loadClass"),
            ("WebViewBridge", r"addJavascriptInterface|setJavaScriptEnabled"),
            ("CommandExec", r"Ljava/lang/Runtime;->exec|ProcessBuilder"),
            ("WeakCrypto", r"MD5|SHA1|DES/|RC4|ECB/"),
        ];

        Self::scan_patterns(decoded_root, &patterns, &["smali", "xml", "java"])
    }

    pub fn suggest_deobfuscation(decoded_root: &Path) -> Vec<String> {
        let class_re = Regex::new(r"^\s*\.class\s+.*L([^;]+);").ok();
        let method_re = Regex::new(r"^\s*\.method\s+.*\s([^\(\s]+)\(").ok();
        let mut out = Vec::new();

        for entry in WalkDir::new(decoded_root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("smali") {
                continue;
            }
            let content = std::fs::read_to_string(path).unwrap_or_default();
            for line in content.lines() {
                let t = line.trim();
                if let Some(re) = &class_re {
                    if let Some(c) = re.captures(t) {
                        let full = c[1].replace('/', ".");
                        let short = full.split('.').last().unwrap_or("");
                        if short.len() <= 2 && short.chars().all(|ch| ch.is_ascii_alphanumeric()) {
                            out.push(format!("Obfuscated class candidate: {} [{}]", full, path.display()));
                        }
                    }
                }
                if let Some(re) = &method_re {
                    if let Some(c) = re.captures(t) {
                        let m = c[1].to_string();
                        if m != "<init>" && m != "<clinit>" && m.len() <= 2 {
                            out.push(format!("Obfuscated method candidate: {} [{}]", m, path.display()));
                        }
                    }
                }
            }
        }

        out.sort();
        out.dedup();
        out
    }

    pub fn scan_anti_tamper(decoded_root: &Path) -> Vec<String> {
        let patterns = [
            ("RootCheck", r"su\b|/system/xbin/su|test-keys|isDeviceRooted|magisk"),
            ("DebuggerCheck", r"isDebuggerConnected|Debug;->waitForDebugger|TracerPid"),
            ("EmulatorCheck", r"generic|goldfish|ranchu|ro\.kernel\.qemu|Build\.FINGERPRINT"),
            ("SignatureCheck", r"getPackageInfo|GET_SIGNATURES|Signature;|MessageDigest"),
            ("PinningCheck", r"TrustManager|checkServerTrusted|X509TrustManager|CertificatePinner"),
        ];

        Self::scan_patterns(decoded_root, &patterns, &["smali", "xml", "java"])
    }

    pub fn generate_frida_template(symbol: &str) -> String {
        let target = if symbol.trim().is_empty() { "com.example.Target" } else { symbol.trim() };
        format!(
            "Java.perform(function() {{\n  var T = Java.use(\"{}\");\n  // TODO: replace methodName + overload signature\n  T.methodName.overload().implementation = function() {{\n    console.log(\"[hook] methodName called\");\n    return this.methodName();\n  }};\n}});\n",
            target
        )
    }

    pub fn endpoint_intel(decoded_root: &Path) -> Vec<String> {
        let url_re = Regex::new(r"https?://[A-Za-z0-9\._~:/?#\[\]@!$&'()*+,;=%-]+")
            .expect("valid url regex");
        let mut domains = BTreeSet::new();
        let mut out = Vec::new();

        for entry in WalkDir::new(decoded_root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            if !["smali", "xml", "json", "txt", "java"].contains(&ext.as_str()) {
                continue;
            }
            let content = std::fs::read_to_string(path).unwrap_or_default();
            for m in url_re.find_iter(&content) {
                let u = m.as_str();
                let host = u
                    .split("//")
                    .nth(1)
                    .unwrap_or("")
                    .split('/')
                    .next()
                    .unwrap_or("")
                    .to_string();
                if !host.is_empty() {
                    domains.insert(host.clone());
                }
                let env = if u.contains("staging") || u.contains("stage") || u.contains("qa") {
                    "staging"
                } else if u.contains("dev") || u.contains("localhost") {
                    "dev"
                } else {
                    "prod"
                };
                out.push(format!("Endpoint [{}] {} ({})", env, u, path.display()));
            }
        }

        let mut result = Vec::new();
        result.push(format!("Domains discovered: {}", domains.len()));
        for d in domains {
            result.push(format!("Domain: {}", d));
        }
        result.extend(out.into_iter().take(200));
        result
    }

    pub fn native_jni_bridge(decoded_root: &Path, native_root: &Path) -> Vec<String> {
        let native_decl = Regex::new(r"\bnative\s+[A-Za-z0-9_\[\]<>]+\s+([A-Za-z0-9_]+)\(")
            .expect("valid native regex");

        let mut java_native_methods = HashSet::new();
        for entry in WalkDir::new(decoded_root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("java") {
                continue;
            }
            let content = std::fs::read_to_string(path).unwrap_or_default();
            for cap in native_decl.captures_iter(&content) {
                java_native_methods.insert(cap[1].to_string());
            }
        }

        let mut exported = HashSet::new();
        for entry in WalkDir::new(native_root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("so") {
                continue;
            }
            if let Ok(mut f) = std::fs::File::open(path) {
                let mut buf = Vec::new();
                let _ = f.read_to_end(&mut buf);
                let text = String::from_utf8_lossy(&buf);
                for token in text.split('\0') {
                    if token.starts_with("Java_") {
                        exported.insert(token.to_string());
                    }
                }
            }
        }

        let mut out = Vec::new();
        out.push(format!("Java native declarations: {}", java_native_methods.len()));
        out.push(format!("Native exported Java_ symbols: {}", exported.len()));

        for m in java_native_methods.iter().take(200) {
            let hit = exported.iter().find(|e| e.contains(m));
            match hit {
                Some(sym) => out.push(format!("JNI map: {} -> {}", m, sym)),
                None => out.push(format!("JNI unresolved: {}", m)),
            }
        }

        out
    }

    pub fn signing_forensics(apk_path: &Path) -> Result<Vec<String>> {
        let file = std::fs::File::open(apk_path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let mut out = Vec::new();
        let mut cert_count = 0usize;

        for i in 0..archive.len() {
            if let Ok(entry) = archive.by_index(i) {
                let name = entry.name().to_string();
                if name.starts_with("META-INF/") {
                    let lower = name.to_ascii_lowercase();
                    if lower.ends_with(".rsa") || lower.ends_with(".dsa") || lower.ends_with(".ec") || lower.ends_with(".sf") {
                        cert_count += 1;
                        out.push(format!("Signature artifact: {} ({} bytes)", name, entry.size()));
                    }
                }
            }
        }

        if cert_count == 0 {
            out.push("No META-INF signature artifacts found (may be unsigned/stripped).".to_string());
        } else {
            out.insert(0, format!("Signature artifacts found: {}", cert_count));
        }

        Ok(out)
    }

    pub fn apply_patch_recipe(decoded_root: &Path, recipe: PatchRecipe) -> Result<usize> {
        let mut changed = 0usize;

        for entry in WalkDir::new(decoded_root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            if !["smali", "xml"].contains(&ext.as_str()) {
                continue;
            }

            let replacements: &[(&str, &str)] = match recipe {
                PatchRecipe::DisableRootChecks => &[
                    ("isDeviceRooted", "isDeviceNotRooted"),
                    ("/system/xbin/su", "/system/xbin/not_su"),
                ],
                PatchRecipe::BypassSslPinning => &[
                    ("checkServerTrusted", "checkServerTrustedBypassed"),
                    ("CertificatePinner", "CertificatePinnerBypassed"),
                ],
                PatchRecipe::ForceDebuggable => &[(
                    "android:debuggable=\"false\"",
                    "android:debuggable=\"true\"",
                )],
            };

            let mut file_changed = false;
            for (find, replace) in replacements {
                if crate::engine::patch::PatchEngine::patch_file(path, find, replace)? > 0 {
                    file_changed = true;
                }
            }
            if file_changed {
                changed += 1;
            }
        }

        Ok(changed)
    }

    pub fn replace_app_icon(decoded_root: &Path, icon_path: &Path) -> Result<usize> {
        if !icon_path.exists() {
            anyhow::bail!("Icon file does not exist: {}", icon_path.display());
        }

        let ext = icon_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let allowed = ["png", "webp", "jpg", "jpeg"];
        if !allowed.contains(&ext.as_str()) {
            anyhow::bail!("Unsupported icon format: {} (use png/webp/jpg)", ext);
        }

        let res_dir = decoded_root.join("res");
        if !res_dir.exists() {
            anyhow::bail!("No res directory found in decoded workspace");
        }

        let icon_bytes = std::fs::read(icon_path)?;
        let mut replaced = 0usize;
        let mut mipmap_dirs = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&res_dir) {
            for entry in entries.flatten() {
                let dir_path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if !dir_path.is_dir() || !name.starts_with("mipmap") {
                    continue;
                }

                mipmap_dirs.push(dir_path.clone());
                if let Ok(icon_entries) = std::fs::read_dir(&dir_path) {
                    for icon_entry in icon_entries.flatten() {
                        let p = icon_entry.path();
                        if !p.is_file() {
                            continue;
                        }

                        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        let p_ext = p
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_ascii_lowercase();

                        if stem.starts_with("ic_launcher") && allowed.contains(&p_ext.as_str()) {
                            std::fs::write(&p, &icon_bytes)?;
                            replaced += 1;
                        }
                    }
                }
            }
        }

        if replaced == 0 {
            for dir in mipmap_dirs {
                let out = dir.join(format!("ic_launcher.{}", ext));
                std::fs::write(out, &icon_bytes)?;
                replaced += 1;
            }
        }

        if replaced == 0 {
            anyhow::bail!("No mipmap folders found for icon replacement");
        }

        Ok(replaced)
    }

    pub fn run_rule_engine(decoded_root: &Path, rules_path: &Path) -> Result<Vec<String>> {
        let content = std::fs::read_to_string(rules_path)?;
        let rules: Vec<Rule> = serde_json::from_str(&content)?;
        let mut out = Vec::new();
        let mut hits = 0usize;

        for rule in rules {
            let name = rule.name.trim();
            if name.is_empty() {
                out.push("Rule skipped: name is empty.".to_string());
                continue;
            }
            if rule.pattern.trim().is_empty() {
                out.push(format!("Rule '{}' skipped: pattern is empty.", name));
                continue;
            }

            let re = match Regex::new(&rule.pattern) {
                Ok(r) => r,
                Err(e) => {
                    out.push(format!("Rule '{}' invalid regex: {}", name, e));
                    continue;
                }
            };

            for entry in WalkDir::new(decoded_root).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }

                if !rule.extensions.is_empty() {
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                    if !rule.extensions.iter().any(|x| x.eq_ignore_ascii_case(&ext)) {
                        continue;
                    }
                }

                let text = std::fs::read_to_string(path).unwrap_or_default();
                if re.is_match(&text) {
                    hits += 1;
                    out.push(format!("Rule hit [{}]: {}", name, path.display()));
                }
            }
        }

        if hits == 0 {
            out.push("No rule hits found.".to_string());
        }

        Ok(out)
    }

    pub fn append_session_note(workspace_root: &Path, note: &str) -> Result<PathBuf> {
        let notes_dir = workspace_root.join("analysis");
        std::fs::create_dir_all(&notes_dir)?;
        let notes_path = notes_dir.join("session_notes.md");
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let entry = format!("\n## {}\n{}\n", ts, note.trim());

        if notes_path.exists() {
            let mut existing = std::fs::read_to_string(&notes_path).unwrap_or_default();
            existing.push_str(&entry);
            std::fs::write(&notes_path, existing)?;
        } else {
            let mut seed = String::from("# RE Session Notes\n");
            seed.push_str(&entry);
            std::fs::write(&notes_path, seed)?;
        }

        Ok(notes_path)
    }

    pub fn discover_plugins(workspace_root: &Path) -> Vec<PluginDescriptor> {
        let plugin_dir = workspace_root.join("plugins");
        if !plugin_dir.exists() {
            return Vec::new();
        }

        let mut out = Vec::new();
        for entry in WalkDir::new(&plugin_dir).max_depth(2).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(plugin) = serde_json::from_str::<PluginDescriptor>(&text) {
                    out.push(plugin);
                }
            }
        }
        out
    }

    pub fn run_plugins(workspace_root: &Path) -> Vec<String> {
        const PLUGIN_TIMEOUT: Duration = Duration::from_secs(30);
        let plugins = Self::discover_plugins(workspace_root);
        let mut out = Vec::new();

        if plugins.is_empty() {
            out.push("No plugins found in workspace/plugins (*.json descriptors).".to_string());
            return out;
        }

        for plugin in plugins {
            if plugin.name.trim().is_empty() {
                out.push("Plugin descriptor skipped: missing name.".to_string());
                continue;
            }
            if plugin.command.trim().is_empty() {
                out.push(format!("Plugin '{}' skipped: command is empty.", plugin.name));
                continue;
            }

            let mut cmd = Command::new(&plugin.command);
            cmd.args(&plugin.args)
                .current_dir(workspace_root)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            match Self::run_plugin_command(cmd, PLUGIN_TIMEOUT) {
                Ok(output) => {
                    let code = output.status.code().unwrap_or(-1);
                    out.push(format!("Plugin '{}' exited with {}", plugin.name, code));
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for line in stdout.lines().take(5) {
                        out.push(format!("  stdout: {}", line));
                    }
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    for line in stderr.lines().take(5) {
                        out.push(format!("  stderr: {}", line));
                    }
                }
                Err(e) => {
                    out.push(format!("Plugin '{}' failed to execute: {}", plugin.name, e));
                }
            }
        }

        out
    }

    fn run_plugin_command(mut cmd: Command, timeout: Duration) -> Result<std::process::Output> {
        let mut child = cmd.spawn()?;
        let started = Instant::now();
        loop {
            if child.try_wait()?.is_some() {
                return child.wait_with_output().map_err(Into::into);
            }

            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("timed out after {}s", timeout.as_secs());
            }

            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn scan_patterns(decoded_root: &Path, patterns: &[(&str, &str)], exts: &[&str]) -> Vec<String> {
        let regs: Vec<(&str, Regex)> = patterns
            .iter()
            .filter_map(|(name, pat)| Regex::new(pat).ok().map(|r| (*name, r)))
            .collect();
        let mut out = Vec::new();

        for entry in WalkDir::new(decoded_root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            if !exts.iter().any(|x| x.eq_ignore_ascii_case(&ext)) {
                continue;
            }
            let text = std::fs::read_to_string(path).unwrap_or_default();
            for (name, re) in &regs {
                if re.is_match(&text) {
                    out.push(format!("{}: {}", name, path.display()));
                }
            }
        }

        out.sort();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{Arsenal, PatchRecipe};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("reveng_arsenal_test_{}", nonce))
    }

    #[test]
    fn patch_recipe_counts_changed_file_once_for_multiple_replacements() {
        let root = temp_dir();
        fs::create_dir_all(root.join("smali")).unwrap();
        let file = root.join("smali/RootCheck.smali");
        fs::write(
            &file,
            "invoke-static {}, Lx;->isDeviceRooted()Z\nconst-string v0, \"/system/xbin/su\"\n",
        )
        .unwrap();

        let changed = Arsenal::apply_patch_recipe(&root, PatchRecipe::DisableRootChecks).unwrap();
        let content = fs::read_to_string(&file).unwrap();

        assert_eq!(changed, 1);
        assert!(content.contains("isDeviceNotRooted"));
        assert!(content.contains("/system/xbin/not_su"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn replace_app_icon_updates_launcher_assets() {
        let root = temp_dir();
        let decoded = root.join("decoded");
        let mipmap = decoded.join("res/mipmap-hdpi");
        fs::create_dir_all(&mipmap).unwrap();
        fs::write(mipmap.join("ic_launcher.png"), b"old").unwrap();
        let replacement = root.join("replacement.png");
        fs::write(&replacement, b"new-icon").unwrap();

        let changed = Arsenal::replace_app_icon(&decoded, &replacement).unwrap();

        assert_eq!(changed, 1);
        assert_eq!(fs::read(mipmap.join("ic_launcher.png")).unwrap(), b"new-icon");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn plugin_runner_reports_invalid_descriptor_without_spawning() {
        let root = temp_dir();
        fs::create_dir_all(root.join("plugins")).unwrap();
        fs::write(
            root.join("plugins/empty.json"),
            r#"{"name":"empty","command":"","args":[]}"#,
        )
        .unwrap();

        let lines = Arsenal::run_plugins(&root);
        assert!(lines.iter().any(|line| line.contains("command is empty")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[cfg(unix)]
    fn plugin_runner_reports_stdout_and_stderr() {
        let root = temp_dir();
        fs::create_dir_all(root.join("plugins")).unwrap();
        fs::write(
            root.join("plugins/echo.json"),
            r#"{"name":"echo","command":"/bin/sh","args":["-c","echo out; echo err >&2"]}"#,
        )
        .unwrap();

        let lines = Arsenal::run_plugins(&root);
        assert!(lines.iter().any(|line| line.contains("Plugin 'echo' exited with 0")));
        assert!(lines.iter().any(|line| line.contains("stdout: out")));
        assert!(lines.iter().any(|line| line.contains("stderr: err")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rule_engine_reports_invalid_rules_no_hits_and_hits() {
        let root = temp_dir();
        let decoded = root.join("decoded");
        fs::create_dir_all(decoded.join("smali")).unwrap();
        fs::write(decoded.join("smali/Main.smali"), "const-string v0, \"secret\"").unwrap();
        let rules = root.join("rules.json");
        fs::write(
            &rules,
            r#"[
                {"name":"","pattern":"secret","extensions":["smali"]},
                {"name":"empty-pattern","pattern":"","extensions":["smali"]},
                {"name":"bad-regex","pattern":"(","extensions":["smali"]},
                {"name":"SecretString","pattern":"secret","extensions":["smali"]}
            ]"#,
        )
        .unwrap();

        let lines = Arsenal::run_rule_engine(&decoded, &rules).unwrap();
        assert!(lines.iter().any(|line| line.contains("name is empty")));
        assert!(lines.iter().any(|line| line.contains("pattern is empty")));
        assert!(lines.iter().any(|line| line.contains("invalid regex")));
        assert!(lines.iter().any(|line| line.contains("Rule hit [SecretString]")));
        assert!(!lines.iter().any(|line| line == "No rule hits found."));

        let nohit_rules = root.join("nohit_rules.json");
        fs::write(
            &nohit_rules,
            r#"[{"name":"NoHit","pattern":"definitely_not_present","extensions":["smali"]}]"#,
        )
        .unwrap();
        let nohit = Arsenal::run_rule_engine(&decoded, &nohit_rules).unwrap();
        assert!(nohit.iter().any(|line| line == "No rule hits found."));

        let _ = fs::remove_dir_all(root);
    }
}
