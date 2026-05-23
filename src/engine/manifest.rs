//! AndroidManifest.xml analyzer — parses decoded manifest for security audit.

use std::path::Path;

use anyhow::Result;
use regex::Regex;

/// Risk level for a permission.
#[derive(Clone, Debug, PartialEq)]
pub enum PermissionRisk {
    Dangerous,
    Normal,
    Signature,
}

impl PermissionRisk {
    pub fn label(&self) -> &str {
        match self {
            Self::Dangerous => "DANGEROUS",
            Self::Normal => "normal",
            Self::Signature => "signature",
        }
    }

    pub fn color(&self) -> egui::Color32 {
        match self {
            Self::Dangerous => egui::Color32::from_rgb(255, 100, 100),
            Self::Normal => egui::Color32::from_rgb(130, 200, 130),
            Self::Signature => egui::Color32::from_rgb(200, 180, 130),
        }
    }
}

/// A parsed permission entry.
#[derive(Clone, Debug)]
pub struct PermissionEntry {
    pub name: String,
    pub short_name: String,
    pub risk: PermissionRisk,
}

/// A component declared in the manifest.
#[derive(Clone, Debug)]
pub struct ComponentEntry {
    pub component_type: ComponentType,
    pub name: String,
    pub exported: bool,
    pub intent_filters: Vec<String>,
    pub permission: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ComponentType {
    Activity,
    Service,
    Receiver,
    Provider,
}

impl ComponentType {
    pub fn label(&self) -> &str {
        match self {
            Self::Activity => "Activity",
            Self::Service => "Service",
            Self::Receiver => "Receiver",
            Self::Provider => "Provider",
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            Self::Activity => "A",
            Self::Service => "S",
            Self::Receiver => "R",
            Self::Provider => "P",
        }
    }
}

/// A security warning about the manifest.
#[derive(Clone, Debug)]
pub struct SecurityWarning {
    pub severity: WarningSeverity,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WarningSeverity {
    High,
    Medium,
    Low,
}

impl WarningSeverity {
    pub fn color(&self) -> egui::Color32 {
        match self {
            Self::High => egui::Color32::from_rgb(255, 80, 80),
            Self::Medium => egui::Color32::from_rgb(255, 180, 60),
            Self::Low => egui::Color32::from_rgb(200, 200, 100),
        }
    }
}

/// Full parsed manifest information.
#[derive(Clone, Debug)]
pub struct ManifestInfo {
    pub package: String,
    pub version_name: String,
    pub version_code: String,
    pub min_sdk: String,
    pub target_sdk: String,
    pub permissions: Vec<PermissionEntry>,
    pub components: Vec<ComponentEntry>,
    pub deeplinks: Vec<String>,
    pub warnings: Vec<SecurityWarning>,
    pub debuggable: bool,
    pub allow_backup: bool,
    pub uses_cleartext: bool,
}

// Known dangerous permissions
const DANGEROUS_PERMISSIONS: &[&str] = &[
    "android.permission.READ_CONTACTS",
    "android.permission.WRITE_CONTACTS",
    "android.permission.READ_CALENDAR",
    "android.permission.WRITE_CALENDAR",
    "android.permission.CAMERA",
    "android.permission.RECORD_AUDIO",
    "android.permission.ACCESS_FINE_LOCATION",
    "android.permission.ACCESS_COARSE_LOCATION",
    "android.permission.ACCESS_BACKGROUND_LOCATION",
    "android.permission.READ_PHONE_STATE",
    "android.permission.CALL_PHONE",
    "android.permission.READ_CALL_LOG",
    "android.permission.WRITE_CALL_LOG",
    "android.permission.SEND_SMS",
    "android.permission.RECEIVE_SMS",
    "android.permission.READ_SMS",
    "android.permission.READ_EXTERNAL_STORAGE",
    "android.permission.WRITE_EXTERNAL_STORAGE",
    "android.permission.BODY_SENSORS",
    "android.permission.ACTIVITY_RECOGNITION",
    "android.permission.READ_MEDIA_IMAGES",
    "android.permission.READ_MEDIA_VIDEO",
    "android.permission.READ_MEDIA_AUDIO",
    "android.permission.POST_NOTIFICATIONS",
];

pub struct ManifestAnalyzer;

impl ManifestAnalyzer {
    /// Parse the decoded AndroidManifest.xml file.
    pub fn analyze(decoded_root: &Path) -> Result<ManifestInfo> {
        let manifest_path = decoded_root.join("AndroidManifest.xml");
        let content = std::fs::read_to_string(&manifest_path)?;

        let package = Self::extract_attr(&content, "package").unwrap_or_default();
        let version_name = Self::extract_attr(&content, "android:versionName").unwrap_or_default();
        let version_code = Self::extract_attr(&content, "android:versionCode").unwrap_or_default();
        let min_sdk = Self::extract_uses_sdk(&content, "minSdkVersion");
        let target_sdk = Self::extract_uses_sdk(&content, "targetSdkVersion");
        let debuggable = Self::extract_attr(&content, "android:debuggable")
            .map(|v| v == "true")
            .unwrap_or(false);
        let allow_backup = Self::extract_attr(&content, "android:allowBackup")
            .map(|v| v != "false")
            .unwrap_or(true);
        let uses_cleartext = Self::extract_attr(&content, "android:usesCleartextTraffic")
            .map(|v| v == "true")
            .unwrap_or(false);

        let permissions = Self::parse_permissions(&content);
        let components = Self::parse_components(&content);
        let deeplinks = Self::parse_deeplinks(&content);
        let warnings = Self::generate_warnings(&permissions, &components, debuggable, allow_backup, uses_cleartext);

        Ok(ManifestInfo {
            package,
            version_name,
            version_code,
            min_sdk,
            target_sdk,
            permissions,
            components,
            deeplinks,
            warnings,
            debuggable,
            allow_backup,
            uses_cleartext,
        })
    }

    fn extract_attr(content: &str, attr: &str) -> Option<String> {
        let pattern = format!(r#"{}="([^"]*)""#, regex::escape(attr));
        Regex::new(&pattern).ok()?.captures(content)?.get(1).map(|m| m.as_str().to_string())
    }

    fn extract_uses_sdk(content: &str, attr: &str) -> String {
        let pattern = format!(r#"android:{}="([^"]*)""#, regex::escape(attr));
        Regex::new(&pattern)
            .ok()
            .and_then(|re| re.captures(content))
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "?".into())
    }

    fn parse_permissions(content: &str) -> Vec<PermissionEntry> {
        let re = Regex::new(r#"<uses-permission\s+android:name="([^"]*)""#).unwrap();
        re.captures_iter(content)
            .map(|caps| {
                let name = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
                let short_name = name.rsplit('.').next().unwrap_or(&name).to_string();
                let risk = if DANGEROUS_PERMISSIONS.contains(&name.as_str()) {
                    PermissionRisk::Dangerous
                } else if name.contains("SIGNATURE") {
                    PermissionRisk::Signature
                } else {
                    PermissionRisk::Normal
                };
                PermissionEntry { name, short_name, risk }
            })
            .collect()
    }

    fn parse_components(content: &str) -> Vec<ComponentEntry> {
        let mut components = Vec::new();

        let types = [
            ("activity", ComponentType::Activity),
            ("service", ComponentType::Service),
            ("receiver", ComponentType::Receiver),
            ("provider", ComponentType::Provider),
        ];

        for (tag, comp_type) in &types {
            let block_re = Regex::new(&format!(
                r#"(?s)<{tag}\s([^>]*?)(?:/>|>(.*?)</{tag}>)"#,
                tag = tag
            ))
            .unwrap();

            for caps in block_re.captures_iter(content) {
                let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let body = caps.get(2).map(|m| m.as_str()).unwrap_or("");

                let name = Self::extract_attr(attrs, "android:name").unwrap_or_default();
                let exported = Self::extract_attr(attrs, "android:exported")
                    .map(|v| v == "true")
                    .unwrap_or(false);
                let permission = Self::extract_attr(attrs, "android:permission");

                // Parse intent filters
                let mut intent_filters = Vec::new();
                let action_re = Regex::new(r#"android:name="([^"]*)""#).unwrap();
                if body.contains("<intent-filter") {
                    for m in action_re.captures_iter(body) {
                        if let Some(a) = m.get(1) {
                            let val = a.as_str();
                            if val.contains("action") || val.contains("category") || val.contains("://") {
                                intent_filters.push(val.to_string());
                            }
                        }
                    }
                }

                components.push(ComponentEntry {
                    component_type: comp_type.clone(),
                    name,
                    exported,
                    intent_filters,
                    permission,
                });
            }
        }

        components
    }

    fn parse_deeplinks(content: &str) -> Vec<String> {
        let re = Regex::new(r#"<data\s[^>]*android:scheme="([^"]*)"[^>]*(?:android:host="([^"]*)")?[^>]*(?:android:pathPrefix="([^"]*)")?[^>]*/>"#).unwrap();
        let mut links = Vec::new();
        for caps in re.captures_iter(content) {
            let scheme = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let host = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let path = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            if !scheme.is_empty() {
                links.push(format!("{}://{}{}", scheme, host, path));
            }
        }
        links
    }

    fn generate_warnings(
        permissions: &[PermissionEntry],
        components: &[ComponentEntry],
        debuggable: bool,
        allow_backup: bool,
        uses_cleartext: bool,
    ) -> Vec<SecurityWarning> {
        let mut warnings = Vec::new();

        if debuggable {
            warnings.push(SecurityWarning {
                severity: WarningSeverity::High,
                message: "App is debuggable — easy to attach debugger in production".into(),
            });
        }

        if allow_backup {
            warnings.push(SecurityWarning {
                severity: WarningSeverity::Medium,
                message: "Backup is allowed — app data can be extracted via adb backup".into(),
            });
        }

        if uses_cleartext {
            warnings.push(SecurityWarning {
                severity: WarningSeverity::Medium,
                message: "Cleartext traffic allowed — HTTP connections are permitted".into(),
            });
        }

        let dangerous_count = permissions.iter().filter(|p| p.risk == PermissionRisk::Dangerous).count();
        if dangerous_count > 5 {
            warnings.push(SecurityWarning {
                severity: WarningSeverity::Medium,
                message: format!("{} dangerous permissions requested", dangerous_count),
            });
        } else if dangerous_count > 0 {
            warnings.push(SecurityWarning {
                severity: WarningSeverity::Low,
                message: format!("{} dangerous permission{} requested", dangerous_count, if dangerous_count == 1 { "" } else { "s" }),
            });
        }

        for comp in components {
            if comp.exported && comp.permission.is_none() && !comp.intent_filters.is_empty() {
                warnings.push(SecurityWarning {
                    severity: WarningSeverity::High,
                    message: format!(
                        "Exported {} '{}' has no permission guard",
                        comp.component_type.label(),
                        comp.name.rsplit('.').next().unwrap_or(&comp.name)
                    ),
                });
            }
        }

        warnings
    }
}
