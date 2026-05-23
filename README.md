# RevEng IDE

<p align="center">
  <img src="./icon.png" alt="RevEng IDE icon" width="132" height="132">
</p>

<p align="center">
  <strong>A native Android reverse-engineering IDE for APK analysis, patching, rebuilds, signing, and runtime work.</strong>
</p>

<p align="center">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-2021-orange">
  <img alt="Platform" src="https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-blue">
  <img alt="License" src="https://img.shields.io/badge/License-MIT-green">
  <img alt="Status" src="https://img.shields.io/badge/Status-Active%20development-yellow">
</p>

RevEng IDE is a desktop application for Android reverse engineering. It brings APK opening, APKTool decode/rebuild, JADX decompilation, Smali navigation, native library inspection, strings and manifest analysis, ADB, Frida helpers, patching, signing, and install workflows into one Rust-based UI.

RevEng IDE is a product of **Levython Technologies**.

## Contents

- [Features](#features)
- [Requirements](#requirements)
- [Build from Source](#build-from-source)
- [Run](#run)
- [Toolchain Setup](#toolchain-setup)
- [Usage](#usage)
- [Project Layout](#project-layout)
- [Release Checklist](#release-checklist)
- [Contributing](#contributing)
- [License](#license)

## Features

- Open `.apk` and `.xapk` files and create isolated workspaces.
- Decode resources and Smali through APKTool.
- Decompile Java sources through JADX.
- Rebuild, sign, and install patched APKs.
- Java-to-Smali and Smali-to-Java navigation.
- Smali cross references, callers, usages, hierarchy, and opcode completion.
- AndroidManifest analysis with permissions, exported components, deep links, and security warnings.
- DEX statistics, string extraction, and secret-like string categorization.
- Native `.so` inspection with ELF metadata and Capstone-backed disassembly.
- Flutter app detection and selected Flutter patching helpers.
- ADB device workflow, Logcat streaming, screenshots, shell commands, and file push/pull.
- Frida process listing, attach/spawn helpers, and built-in script templates.
- Command palette, quick open, split editor, search, and workspace file tree.

## Requirements

Core build requirements:

- Rust stable, edition 2021 compatible.
- A C/C++ build toolchain for native dependencies such as Capstone.
- Git.

Runtime tool requirements depend on which workflows you use:

| Tool | Used for |
| --- | --- |
| Java 11+ | APKTool, JADX, and APK signing tools |
| APKTool | Decode and rebuild APKs |
| JADX | Java decompilation |
| Android SDK Platform Tools | ADB, install, device shell, screenshots, Logcat |
| Uber APK Signer | Zipalign and debug signing |
| Python 3 + pip | Optional Frida tooling |
| frida-tools | Optional dynamic analysis |
| APKiD | Optional packer/protector detection |

On Linux, install the desktop libraries required by `eframe`/`egui`. Debian and Ubuntu users commonly need:

```bash
sudo apt install build-essential pkg-config libgl1-mesa-dev libgtk-3-dev
```

On macOS, install Xcode command line tools:

```bash
xcode-select --install
```

## Build from Source

```bash
git clone https://github.com/Levython-Technologies/reveng-ide.git
cd reveng-ide
cargo build --release
```

The release binary is written to:

- Windows: `target/release/reveng-ide.exe`
- Linux/macOS: `target/release/reveng-ide`

For a faster development build:

```bash
cargo build
```

## Run

After building:

```bash
cargo run
```

Or run the compiled release binary directly:

```bash
./target/release/reveng-ide
```

On Windows:

```powershell
.\target\release\reveng-ide.exe
```

## Toolchain Setup

RevEng IDE looks for external Android reverse-engineering tools in `tools/` and can also use tools available on your `PATH`.

Expected local layout:

```text
tools/
  apktool.jar
  apktool/
    apktool.jar
  jadx/
    lib/
      jadx-all.jar
  platform-tools/
    adb
    adb.exe
  uber-apk-signer.jar
  apksigner/
    uber-apk-signer.jar
```

The `tools/` directory is intentionally ignored by Git because these files are downloaded binaries and can be large. Populate it with the in-app tool updater, by installing the tools globally, or by running the PowerShell helper on Windows:

```powershell
.\setup_tools_robust.ps1
```

Useful upstream downloads:

- APKTool: <https://apktool.org>
- JADX: <https://github.com/skylot/jadx/releases>
- Android Platform Tools: <https://developer.android.com/tools/releases/platform-tools>
- Uber APK Signer: <https://github.com/patrickfav/uber-apk-signer/releases>
- Frida tools: `pip install frida-tools`
- APKiD: `pip install apkid`

## Usage

1. Launch RevEng IDE.
2. Open an `.apk` or `.xapk` file from the toolbar.
3. Use **Decode** to extract resources and Smali with APKTool.
4. Use **Decompile** to generate Java sources with JADX.
5. Browse files from the Explorer, or use quick open and the command palette.
6. Use Java/Smali navigation and xref tools to understand code flow.
7. Edit resources or Smali as needed.
8. Use **Build** to rebuild the APK.
9. Use **Sign** to sign the rebuilt APK.
10. Use **Install** to deploy it to a connected Android device.

Workspace data is written under `workspace/`. That directory is ignored by Git and can be deleted when you no longer need a local analysis session.

## Project Layout

```text
.
  assets/              UI assets
  icon.png             Application icon, also used by this README
  src/
    app.rs             Shared application state and project actions
    main.rs            Native application entry point
    engine/            APK, toolchain, workspace, patching, analysis logic
    native/            Native binary and Flutter helpers
    runtime/           ADB and Frida integration
    ui/                egui user interface
  vendor/eframe/       Local eframe patch used by Cargo.toml
  Cargo.toml           Rust package manifest
  Cargo.lock           Locked dependency graph for reproducible builds
```

## Release Checklist

Before publishing a release:

```bash
cargo fmt --check
cargo test
cargo build --release
```

Also verify:

- The app starts on each target platform.
- The application icon appears in the window and packaged artifact.
- A sample APK can be opened, decoded, decompiled, rebuilt, signed, and installed.
- `tools/`, `workspace/`, and build outputs are not included in the source archive.

## Contributing

Contributions are welcome. Levython Technologies will be glad if developers, reverse engineers, designers, testers, and documentation writers help improve RevEng IDE.

Good contributions include:

- Bug fixes with a clear reproduction case.
- Focused feature work that fits the APK analysis and patching workflow.
- UI polish that keeps the app fast and practical.
- Tests for parsers, workspace behavior, and analysis logic.
- Documentation improvements for setup, platform quirks, and workflows.

Please keep pull requests focused, run formatting and tests before submitting, and describe the behavior change clearly.

## Security and Legal Use

Use RevEng IDE only on applications and devices you own, have permission to test, or are legally authorized to analyze. The project is intended for research, interoperability, education, malware analysis, defensive testing, and legitimate app repair workflows.

## License

RevEng IDE is released under the MIT License. See [LICENSE](./LICENSE).
