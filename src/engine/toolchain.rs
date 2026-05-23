//! Toolchain manager verifies, locates, and runs bundled external tools.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolRequirement {
    Required,
    Optional,
}

/// Describes a single external tool.
#[derive(Clone, Debug)]
pub struct ToolInfo {
    pub name: String,
    pub executable: PathBuf,
    /// Optional: if the tool is a jar, launch it through Java.
    pub is_jar: bool,
    pub available: bool,
    pub required: ToolRequirement,
    path_lookup: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ToolUpdateCandidate {
    pub tool: String,
    pub display_name: String,
    pub current_version: Option<String>,
    pub latest_version: String,
    pub download_url: String,
    pub source_url: String,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GithubAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Manages all bundled reverse-engineering tools.
#[derive(Clone)]
pub struct ToolchainManager {
    tools_base: PathBuf,
    tools: HashMap<String, ToolInfo>,
}

impl ToolchainManager {
    const MAX_JADX_CLI_JAR_BYTES: u64 = 512 * 1024 * 1024;
    const MAX_CAPTURED_TOOL_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
    const MAX_TOOL_DOWNLOAD_BYTES: usize = 700 * 1024 * 1024;

    fn sanitize_path(path: PathBuf) -> PathBuf {
        #[cfg(windows)]
        {
            let raw = path.to_string_lossy();
            if let Some(stripped) = raw.strip_prefix(r"\\?\UNC\") {
                return PathBuf::from(format!(r"\\{}", stripped));
            }
            if let Some(stripped) = raw.strip_prefix(r"\\?\") {
                return PathBuf::from(stripped);
            }
        }

        path
    }

    pub fn new() -> Self {
        let tools_base = Self::resolve_tools_base();

        log::info!("Toolchain base: {}", tools_base.display());

        let mut mgr = Self {
            tools_base: tools_base.clone(),
            tools: HashMap::new(),
        };

        let jadx_cli = mgr.ensure_jadx_cli();

        mgr.register_candidates(
            "jadx",
            &[jadx_cli],
            true,
            None,
            ToolRequirement::Required,
        );
        mgr.register_candidates(
            "jadx-gui",
            &[
                tools_base.join("jadx").join("bin").join("jadx-gui"),
                tools_base.join("jadx").join("bin").join("jadx-gui.bat"),
                tools_base.join("jadx").join("bin").join("jadx-gui.cmd"),
                tools_base.join("jadx").join("jadx-gui"),
                tools_base.join("jadx").join("jadx-gui-1.5.0.exe"),
            ],
            false,
            Some("jadx-gui"),
            ToolRequirement::Optional,
        );
        mgr.register_candidates(
            "apktool",
            &[
                tools_base.join("apktool.jar"),
                tools_base.join("apktool").join("apktool.jar"),
            ],
            true,
            None,
            ToolRequirement::Required,
        );
        mgr.register_candidates(
            "adb",
            &[
                tools_base.join("platform-tools").join("adb"),
                tools_base.join("platform-tools").join("adb.exe"),
                tools_base.join("adb").join("adb"),
                tools_base.join("adb").join("adb.exe"),
            ],
            false,
            Some("adb"),
            ToolRequirement::Required,
        );
        mgr.register_candidates(
            "uber-apk-signer",
            &[
                tools_base.join("uber-apk-signer.jar"),
                tools_base.join("apksigner").join("uber-apk-signer.jar"),
            ],
            true,
            None,
            ToolRequirement::Required,
        );

        // Frida CLI tools are installed via `pip install frida-tools` and live on PATH.
        mgr.register_candidates(
            "frida",
            &[],
            false,
            Some("frida"),
            ToolRequirement::Optional,
        );
        mgr.register_candidates(
            "frida-ps",
            &[],
            false,
            Some("frida-ps"),
            ToolRequirement::Optional,
        );

        // APKiD — APK identifier tool.
        mgr.register_candidates(
            "apkid",
            &[],
            false,
            Some("apkid"),
            ToolRequirement::Optional,
        );

        mgr
    }

    fn resolve_tools_base() -> PathBuf {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));

        let mut candidates = vec![current_dir.join("tools")];

        if let Some(exe_dir) = exe_dir {
            candidates.push(exe_dir.join("tools"));
            if let Some(parent) = exe_dir.parent() {
                candidates.push(parent.join("tools"));
                candidates.push(parent.join("Resources").join("tools"));
                if let Some(grandparent) = parent.parent() {
                    candidates.push(grandparent.join("tools"));
                }
            }
        }

        for candidate in candidates {
            if candidate.exists() {
                return Self::normalize_path(candidate);
            }
        }

        PathBuf::from("tools")
    }

    fn normalize_path(path: PathBuf) -> PathBuf {
        Self::sanitize_path(path.canonicalize().unwrap_or(path))
    }

    fn ensure_jadx_cli(&self) -> PathBuf {
        let cli_jar = self.tools_base.join("jadx").join("lib").join("jadx-all.jar");
        if cli_jar.exists() {
            return Self::normalize_path(cli_jar);
        }

        let archives = [
            self.tools_base.join("jadx_cli_temp.zip"),
            self.tools_base.join("jadx.zip"),
            self.tools_base.join("jadx_cli_bundle.zip"),
        ];

        for archive in archives {
            if archive.exists() && Self::extract_jadx_cli_jar(&archive, &cli_jar).is_ok() {
                return Self::normalize_path(cli_jar);
            }
        }

        cli_jar
    }

    fn extract_jadx_cli_jar(archive_path: &Path, output_path: &Path) -> Result<()> {
        let file = std::fs::File::open(archive_path)
            .with_context(|| format!("Failed to open {}", archive_path.display()))?;
        let mut archive = zip::ZipArchive::new(file)
            .with_context(|| format!("Failed to read {}", archive_path.display()))?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().replace('\\', "/");
            if name.ends_with("/jadx-1.5.0-all.jar") || name.ends_with("/jadx-all.jar") {
                Self::ensure_archive_entry_size(
                    &name,
                    entry.size(),
                    Self::MAX_JADX_CLI_JAR_BYTES,
                    "JADX CLI jar",
                )?;
                if let Some(parent) = output_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut outfile = std::fs::File::create(output_path)?;
                std::io::copy(&mut entry, &mut outfile)?;
                return Ok(());
            }
        }

        anyhow::bail!(
            "Could not find jadx CLI jar inside {}",
            archive_path.display()
        );
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

    fn append_capped_output(output: &mut String, line: &str, cap: usize, truncated: &mut bool) {
        if output.len() >= cap {
            *truncated = true;
            return;
        }

        let remaining = cap - output.len();
        if line.len() + 1 <= remaining {
            output.push_str(line);
            output.push('\n');
        } else {
            let take = line
                .char_indices()
                .map(|(idx, _)| idx)
                .take_while(|idx| *idx < remaining.saturating_sub(1))
                .last()
                .unwrap_or(0);
            output.push_str(&line[..take]);
            output.push('\n');
            *truncated = true;
        }
    }

    fn register_candidates(
        &mut self,
        name: &str,
        candidates: &[PathBuf],
        is_jar: bool,
        path_lookup: Option<&str>,
        required: ToolRequirement,
    ) {
        let resolved = candidates
            .iter()
            .find(|candidate| {
                if is_jar {
                    candidate.is_file()
                } else {
                    Self::is_native_executable(candidate)
                }
            })
            .map(|candidate| Self::normalize_path(candidate.clone()))
            .or_else(|| path_lookup.and_then(Self::find_on_path));

        let executable = resolved.unwrap_or_else(|| {
            candidates
                .first()
                .cloned()
                .unwrap_or_else(|| PathBuf::from(path_lookup.unwrap_or(name)))
        });

        let available = if is_jar {
            executable.is_file()
        } else {
            Self::is_native_executable(&executable)
        };

        self.tools.insert(
            name.to_string(),
            ToolInfo {
                name: name.to_string(),
                executable,
                is_jar,
                available,
                required,
                path_lookup: path_lookup.map(str::to_string),
            },
        );
    }

    fn find_on_path(name: &str) -> Option<PathBuf> {
        #[cfg(windows)]
        let which_cmd = "where";
        #[cfg(not(windows))]
        let which_cmd = "which";

        Command::new(which_cmd)
            .arg(name)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                let raw = String::from_utf8_lossy(&o.stdout).trim().to_string();
                raw.lines()
                    .next()
                    .map(|line| Self::normalize_path(PathBuf::from(line.trim())))
            })
    }

    fn is_native_executable(path: &Path) -> bool {
        if !path.is_file() {
            return false;
        }

        #[cfg(windows)]
        {
            return true;
        }

        #[cfg(not(windows))]
        {
            if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
                if matches!(ext, "exe" | "bat" | "cmd") {
                    return false;
                }
            }

            return path
                .metadata()
                .map(|meta| meta.permissions().mode() & 0o111 != 0)
                .unwrap_or(false);
        }
    }

    fn refresh_tool(tool: &mut ToolInfo) {
        if tool.is_jar {
            tool.available = tool.executable.is_file();
            return;
        }

        if Self::is_native_executable(&tool.executable) {
            tool.available = true;
            return;
        }

        if let Some(path_lookup) = tool.path_lookup.as_deref() {
            if let Some(executable) = Self::find_on_path(path_lookup) {
                tool.executable = executable;
                tool.available = true;
                return;
            }
        }

        tool.available = false;
    }

    /// Verify all tools and return their availability.
    pub fn verify_all(&mut self) -> Vec<(String, bool)> {
        let mut results = Vec::new();
        for (name, info) in &mut self.tools {
            Self::refresh_tool(info);
            results.push((name.clone(), info.available));
        }
        results.sort_by(|a, b| a.0.cmp(&b.0));
        results
    }

    /// Get a tool's info.
    pub fn get(&self, name: &str) -> Option<&ToolInfo> {
        self.tools.get(name)
    }

    pub fn describe_tool(&self, name: &str) -> Option<String> {
        self.tools.get(name).map(|tool| {
            format!(
                "{} => path={} mode={} role={}",
                tool.name,
                tool.executable.display(),
                if tool.is_jar { "jar" } else { "native" },
                match tool.required {
                    ToolRequirement::Required => "required",
                    ToolRequirement::Optional => "optional",
                }
            )
        })
    }

    /// Get the base tools directory.
    pub fn tools_dir(&self) -> &Path {
        &self.tools_base
    }

    pub fn check_for_updates(&self) -> Result<Vec<ToolUpdateCandidate>> {
        let mut updates = Vec::new();

        if let Some(update) = self.github_tool_update(
            "apktool",
            "APKTool",
            "iBotPeaches/Apktool",
            |asset| {
                let name = asset.name.to_ascii_lowercase();
                name.starts_with("apktool_") && name.ends_with(".jar")
            },
            "Replaces the bundled APKTool jar used for decode/build.",
        )? {
            updates.push(update);
        }

        if let Some(update) = self.github_tool_update(
            "jadx",
            "JADX CLI",
            "skylot/jadx",
            |asset| {
                let name = asset.name.to_ascii_lowercase();
                name.starts_with("jadx-")
                    && name.ends_with(".zip")
                    && !name.contains("gui")
                    && !name.contains("with-jre")
            },
            "Updates the JADX command-line jar used for Java decompilation.",
        )? {
            updates.push(update);
        }

        if let Some(update) = self.github_tool_update(
            "uber-apk-signer",
            "Uber APK Signer",
            "patrickfav/uber-apk-signer",
            |asset| asset.name.to_ascii_lowercase().ends_with(".jar"),
            "Replaces the APK signing jar used before install.",
        )? {
            updates.push(update);
        }

        if let Some(update) = self.platform_tools_update()? {
            updates.push(update);
        }

        Ok(updates)
    }

    pub fn install_updates(&mut self, updates: &[ToolUpdateCandidate]) -> Result<Vec<String>> {
        let mut installed = Vec::new();

        for update in updates {
            match update.tool.as_str() {
                "apktool" => {
                    let bytes = Self::download_bytes(&update.download_url)?;
                    let apktool_dir = self.tools_base.join("apktool");
                    std::fs::create_dir_all(&apktool_dir)?;
                    std::fs::write(self.tools_base.join("apktool.jar"), &bytes)?;
                    std::fs::write(apktool_dir.join("apktool.jar"), &bytes)?;
                    installed.push(format!("APKTool {}", update.latest_version));
                }
                "jadx" => {
                    let bytes = Self::download_bytes(&update.download_url)?;
                    self.install_jadx_zip(&bytes)?;
                    installed.push(format!("JADX CLI {}", update.latest_version));
                }
                "uber-apk-signer" => {
                    let bytes = Self::download_bytes(&update.download_url)?;
                    let signer_dir = self.tools_base.join("apksigner");
                    std::fs::create_dir_all(&signer_dir)?;
                    std::fs::write(self.tools_base.join("uber-apk-signer.jar"), &bytes)?;
                    std::fs::write(signer_dir.join("uber-apk-signer.jar"), &bytes)?;
                    installed.push(format!("Uber APK Signer {}", update.latest_version));
                }
                "adb" => {
                    let bytes = Self::download_bytes(&update.download_url)?;
                    self.install_platform_tools_zip(&bytes)?;
                    installed.push(format!("Android platform-tools {}", update.latest_version));
                }
                _ => {}
            }
        }

        self.tools.clear();
        *self = Self::new();
        self.verify_all();

        Ok(installed)
    }

    fn github_tool_update<F>(
        &self,
        tool: &str,
        display_name: &str,
        repo: &str,
        asset_match: F,
        detail: &str,
    ) -> Result<Option<ToolUpdateCandidate>>
    where
        F: Fn(&GithubAsset) -> bool,
    {
        let release = Self::github_latest_release(repo)?;
        let latest_version = Self::normalize_version(&release.tag_name);
        let asset = release
            .assets
            .iter()
            .find(|asset| asset_match(asset))
            .with_context(|| format!("No downloadable asset found for {}", display_name))?;
        let current_version = self.current_tool_version(tool);

        if !Self::is_update_needed(current_version.as_deref(), &latest_version) {
            return Ok(None);
        }

        Ok(Some(ToolUpdateCandidate {
            tool: tool.to_string(),
            display_name: display_name.to_string(),
            current_version,
            latest_version,
            download_url: asset.browser_download_url.clone(),
            source_url: release.html_url,
            detail: detail.to_string(),
        }))
    }

    fn platform_tools_update(&self) -> Result<Option<ToolUpdateCandidate>> {
        let xml = ureq::get("https://dl.google.com/android/repository/repository2-1.xml")
            .set("User-Agent", "RevEng-IDE")
            .timeout(Duration::from_secs(15))
            .call()
            .context("Failed to query Android repository metadata")?
            .into_string()
            .context("Failed to read Android repository metadata")?;

        let version_re = regex::Regex::new(
            r#"(?s)<remotePackage path="platform-tools">.*?<revision>.*?<major>(\d+)</major>.*?<minor>(\d+)</minor>.*?<micro>(\d+)</micro>"#,
        )?;
        let Some(caps) = version_re.captures(&xml) else {
            return Ok(None);
        };

        let latest_version = format!("{}.{}.{}", &caps[1], &caps[2], &caps[3]);
        let current_version = self.platform_tools_version();
        if !Self::is_update_needed(current_version.as_deref(), &latest_version) {
            return Ok(None);
        }

        let os = if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "macos") {
            "darwin"
        } else {
            "linux"
        };

        Ok(Some(ToolUpdateCandidate {
            tool: "adb".to_string(),
            display_name: "Android platform-tools".to_string(),
            current_version,
            latest_version,
            download_url: format!(
                "https://dl.google.com/android/repository/platform-tools-latest-{}.zip",
                os
            ),
            source_url: "https://developer.android.com/tools/releases/platform-tools".to_string(),
            detail: "Installs the current OS build of adb and fastboot.".to_string(),
        }))
    }

    fn github_latest_release(repo: &str) -> Result<GithubRelease> {
        let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
        let release = ureq::get(&url)
            .set("User-Agent", "RevEng-IDE")
            .set("Accept", "application/vnd.github+json")
            .timeout(Duration::from_secs(15))
            .call()
            .with_context(|| format!("Failed to query {}", repo))?
            .into_json()
            .with_context(|| format!("Failed to parse {}", repo))?;
        Ok(release)
    }

    fn download_bytes(url: &str) -> Result<Vec<u8>> {
        let response = ureq::get(url)
            .set("User-Agent", "RevEng-IDE")
            .timeout(Duration::from_secs(120))
            .call()
            .with_context(|| format!("Failed to download {}", url))?;

        let mut reader = response.into_reader();
        let mut bytes = Vec::new();
        reader
            .by_ref()
            .take(Self::MAX_TOOL_DOWNLOAD_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > Self::MAX_TOOL_DOWNLOAD_BYTES {
            anyhow::bail!("Download is too large: {}", url);
        }
        Ok(bytes)
    }

    fn install_jadx_zip(&self, bytes: &[u8]) -> Result<()> {
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor)?;
        let output_path = self.tools_base.join("jadx").join("lib").join("jadx-all.jar");

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().replace('\\', "/");
            if name.ends_with("/jadx-all.jar") || name.ends_with("-all.jar") {
                Self::ensure_archive_entry_size(
                    &name,
                    entry.size(),
                    Self::MAX_JADX_CLI_JAR_BYTES,
                    "JADX CLI jar",
                )?;
                if let Some(parent) = output_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut outfile = std::fs::File::create(&output_path)?;
                std::io::copy(&mut entry, &mut outfile)?;
                return Ok(());
            }
        }

        anyhow::bail!("JADX archive did not contain jadx-all.jar");
    }

    fn install_platform_tools_zip(&self, bytes: &[u8]) -> Result<()> {
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor)?;
        std::fs::create_dir_all(&self.tools_base)?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let Some(name) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
                continue;
            };
            let out_path = self.tools_base.join(name);

            if entry.is_dir() {
                std::fs::create_dir_all(&out_path)?;
                continue;
            }

            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut outfile)?;

            #[cfg(unix)]
            {
                let filename = out_path.file_name().and_then(|s| s.to_str()).unwrap_or_default();
                if matches!(filename, "adb" | "fastboot" | "sqlite3" | "make_f2fs" | "mke2fs") {
                    let mut perms = std::fs::metadata(&out_path)?.permissions();
                    perms.set_mode(0o755);
                    std::fs::set_permissions(&out_path, perms)?;
                }
            }
        }

        Ok(())
    }

    fn current_tool_version(&self, tool: &str) -> Option<String> {
        match tool {
            "apktool" => self.tool_version_output("apktool", &["--version"]),
            "jadx" => self.tool_version_output("jadx", &["--version"]),
            "uber-apk-signer" => self
                .tool_version_output("uber-apk-signer", &["--version"])
                .or_else(|| self.jar_filename_version("uber-apk-signer")),
            "adb" => self.platform_tools_version(),
            _ => None,
        }
        .and_then(|text| Self::first_version(&text))
    }

    fn platform_tools_version(&self) -> Option<String> {
        let source_properties = self.tools_base.join("platform-tools").join("source.properties");
        if let Ok(text) = std::fs::read_to_string(source_properties) {
            for line in text.lines() {
                if let Some(version) = line.strip_prefix("Pkg.Revision") {
                    return version
                        .split('=')
                        .nth(1)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string);
                }
            }
        }

        self.tool_version_output("adb", &["version"])
            .and_then(|text| Self::first_version(&text))
    }

    fn jar_filename_version(&self, tool: &str) -> Option<String> {
        let file = self.tools.get(tool)?.executable.file_name()?.to_string_lossy();
        Self::first_version(&file)
    }

    fn tool_version_output(&self, tool: &str, args: &[&str]) -> Option<String> {
        let info = self.tools.get(tool)?;
        if !info.available {
            return None;
        }

        let mut command = if info.is_jar {
            let java_cmd = self.java_executable();
            let mut command = Command::new(java_cmd);
            if tool == "jadx" {
                let jadx_home = self.jadx_home_dir();
                command
                    .arg(format!("-Duser.home={}", jadx_home.display()))
                    .arg(format!("-Djava.io.tmpdir={}", jadx_home.join("tmp").display()))
                    .arg("-cp")
                    .arg(&info.executable)
                    .arg("jadx.cli.JadxCLI");
            } else {
                command.arg("-jar").arg(&info.executable);
            }
            command
        } else {
            Command::new(&info.executable)
        };

        let output = Self::run_command_timeout(command.args(args), Duration::from_secs(8)).ok()?;
        Some(format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }

    fn run_command_timeout(command: &mut Command, timeout: Duration) -> Result<Output> {
        let mut child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let started = Instant::now();

        loop {
            if child.try_wait()?.is_some() {
                return child.wait_with_output().map_err(Into::into);
            }
            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("command timed out after {}s", timeout.as_secs());
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }

    fn normalize_version(version: &str) -> String {
        version
            .trim()
            .trim_start_matches('v')
            .trim_start_matches('V')
            .to_string()
    }

    fn first_version(text: &str) -> Option<String> {
        let re = regex::Regex::new(r"\d+(?:\.\d+){1,3}").ok()?;
        re.find(text).map(|m| m.as_str().to_string())
    }

    fn is_update_needed(current: Option<&str>, latest: &str) -> bool {
        let Some(current) = current else {
            return true;
        };
        Self::compare_versions(current, latest).is_lt()
    }

    fn compare_versions(current: &str, latest: &str) -> std::cmp::Ordering {
        let parse = |value: &str| {
            value
                .split(|c: char| !c.is_ascii_digit())
                .filter(|part| !part.is_empty())
                .take(4)
                .map(|part| part.parse::<u32>().unwrap_or(0))
                .collect::<Vec<_>>()
        };
        let mut left = parse(current);
        let mut right = parse(latest);
        let len = left.len().max(right.len()).max(1);
        left.resize(len, 0);
        right.resize(len, 0);
        left.cmp(&right)
    }

    fn java_executable(&self) -> PathBuf {
        let candidates = [
            self.tools_base
                .join("jadx")
                .join("jre")
                .join("bin")
                .join("java"),
            self.tools_base
                .join("jadx")
                .join("jre")
                .join("bin")
                .join("java.exe"),
        ];

        candidates
            .into_iter()
            .find(|candidate| Self::is_native_executable(candidate))
            .map(Self::normalize_path)
            .or_else(|| Self::find_on_path("java"))
            .unwrap_or_else(|| PathBuf::from("java"))
    }

    fn jadx_home_dir(&self) -> PathBuf {
        let home = std::env::temp_dir().join("reveng-ide").join("jadx-home");
        let _ = std::fs::create_dir_all(home.join("tmp"));
        home
    }

    /// Run a tool with given arguments and return the raw output.
    pub fn run_tool(&self, name: &str, args: &[&str]) -> Result<Output> {
        let tool = self
            .tools
            .get(name)
            .with_context(|| format!("Unknown tool: {}", name))?;

        if !tool.available {
            anyhow::bail!(
                "Tool '{}' not found at: {}",
                name,
                tool.executable.display()
            );
        }

        let output = if tool.is_jar {
            let java_cmd = self.java_executable();
            let mut command = Command::new(&java_cmd);

            if name == "jadx" {
                let jadx_home = self.jadx_home_dir();
                command
                    .arg(format!("-Duser.home={}", jadx_home.display()))
                    .arg(format!("-Djava.io.tmpdir={}", jadx_home.join("tmp").display()))
                    .arg("-Xmx2g")
                    .arg("-cp")
                    .arg(&tool.executable)
                    .arg("jadx.cli.JadxCLI");
            } else {
                command.arg("-jar").arg(&tool.executable);
            }

            command
                .args(args)
                .output()
                .with_context(|| {
                    format!(
                        "Failed to run {} via {}",
                        tool.executable.display(),
                        java_cmd.display()
                    )
                })?
        } else {
            Command::new(&tool.executable)
                .args(args)
                .output()
                .with_context(|| format!("Failed to run {}", tool.executable.display()))?
        };

        Ok(output)
    }

    /// Run a tool and capture stdout/stderr as a single string.
    pub fn run_tool_string(&self, name: &str, args: &[&str]) -> Result<String> {
        let output = self.run_tool(name, args)?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            anyhow::bail!(
                "Tool '{}' exited with {}\nstdout: {}\nstderr: {}",
                name,
                output.status,
                stdout,
                stderr,
            );
        }

        Ok(format!("{}{}", stdout, stderr))
    }

    pub fn run_tool_streaming(
        &self,
        name: &str,
        args: &[&str],
        log_tx: &std::sync::mpsc::Sender<crate::app::LogEntry>,
        tag: &str,
    ) -> Result<String> {
        let tool = self
            .tools
            .get(name)
            .with_context(|| format!("Unknown tool: {}", name))?;

        if !tool.available {
            anyhow::bail!(
                "Tool '{}' not found at: {}",
                name,
                tool.executable.display()
            );
        }

        let mut command = if tool.is_jar {
            let java_cmd = self.java_executable();
            let mut cmd = Command::new(&java_cmd);
            if name == "jadx" {
                let jadx_home = self.jadx_home_dir();
                cmd.arg(format!("-Duser.home={}", jadx_home.display()))
                    .arg(format!("-Djava.io.tmpdir={}", jadx_home.join("tmp").display()))
                    .arg("-Xmx2g")
                    .arg("-cp")
                    .arg(&tool.executable)
                    .arg("jadx.cli.JadxCLI");
            } else {
                cmd.arg("-jar").arg(&tool.executable);
            }
            cmd
        } else {
            Command::new(&tool.executable)
        };

        let mut child = command
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to start {}", tool.executable.display()))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let out_tx = log_tx.clone();
        let out_tag = tag.to_string();
        let out_handle = std::thread::spawn(move || {
            let mut output = String::new();
            let mut truncated = false;
            if let Some(stdout) = stdout {
                let reader = BufReader::new(stdout);
                for line in reader.lines().map_while(Result::ok) {
                    Self::append_capped_output(
                        &mut output,
                        &line,
                        Self::MAX_CAPTURED_TOOL_OUTPUT_BYTES,
                        &mut truncated,
                    );
                    let _ = out_tx.send(crate::app::LogEntry {
                        timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
                        level: crate::app::LogLevel::Debug,
                        message: format!("{} {}", out_tag, line),
                    });
                }
            }
            if truncated {
                output.push_str("[output truncated]\n");
            }
            output
        });

        let err_tx = log_tx.clone();
        let err_tag = tag.to_string();
        let err_handle = std::thread::spawn(move || {
            let mut output = String::new();
            let mut truncated = false;
            if let Some(stderr) = stderr {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    Self::append_capped_output(
                        &mut output,
                        &line,
                        Self::MAX_CAPTURED_TOOL_OUTPUT_BYTES,
                        &mut truncated,
                    );
                    let _ = err_tx.send(crate::app::LogEntry {
                        timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
                        level: crate::app::LogLevel::Debug,
                        message: format!("{} {}", err_tag, line),
                    });
                }
            }
            if truncated {
                output.push_str("[output truncated]\n");
            }
            output
        });

        let status = child.wait()?;
        let stdout = out_handle.join().unwrap_or_default();
        let stderr = err_handle.join().unwrap_or_default();

        if !status.success() {
            anyhow::bail!(
                "Tool '{}' exited with {}\nstdout: {}\nstderr: {}",
                name,
                status,
                stdout,
                stderr,
            );
        }

        Ok(format!("{}{}", stdout, stderr))
    }

    pub fn get_tool_tip(tool: &str) -> Option<&'static str> {
        match tool {
            "apktool" => Some("Use 'Decode' to extract resources and smali. Use 'Build' to reassemble."),
            "jadx" => Some("Decompile runs through the JADX CLI and writes Java into the IDE workspace."),
            "adb" => Some("Ensure Developer Options and USB Debugging are enabled on your device."),
            "apksigner" => Some("All modified APKs must be signed before installation on non-rooted devices."),
            "frida" | "frida-ps" => Some("Install with: pip install frida-tools  •  Run tools/frida/setup_frida.ps1 to download frida-server for your device."),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ToolchainManager;

    #[test]
    fn archive_entry_size_guard_rejects_oversized_tool_payloads() {
        assert!(ToolchainManager::ensure_archive_entry_size("jadx-all.jar", 10, 10, "jar").is_ok());
        let err = ToolchainManager::ensure_archive_entry_size("jadx-all.jar", 11, 10, "jar")
            .unwrap_err()
            .to_string();
        assert!(err.contains("too large"));
        assert!(err.contains("jadx-all.jar"));
    }

    #[test]
    fn captured_tool_output_is_capped_on_char_boundaries() {
        let mut output = String::new();
        let mut truncated = false;

        ToolchainManager::append_capped_output(&mut output, "abé中cd", 6, &mut truncated);

        assert!(truncated);
        assert_eq!(output, "abé\n");
    }
}
