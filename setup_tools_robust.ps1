$ProgressPreference = 'SilentlyContinue'
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$ToolsDir = Join-Path $RepoRoot "tools"
if (!(Test-Path $ToolsDir)) { New-Item -ItemType Directory -Path $ToolsDir | Out-Null }

function Download-With-Curl {
    param($Url, $OutPath)
    Write-Host "Downloading $Url using curl..."
    curl.exe -L -s -o "$OutPath" "$Url"
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Failed to download with curl (Exit code $LASTEXITCODE)"
        return $false
    }
    return $true
}

# 1. Clean and Setup JADX
if (Test-Path "$ToolsDir\jadx") { Remove-Item -Recurse -Force "$ToolsDir\jadx" }
$JadxZip = "$ToolsDir\jadx_temp.zip"
if (Download-With-Curl "https://github.com/skylot/jadx/releases/download/v1.5.0/jadx-gui-1.5.0-with-jre-win.zip" $JadxZip) {
    Expand-Archive -Path $JadxZip -DestinationPath "$ToolsDir\jadx" -Force
    Remove-Item $JadxZip
}

$JadxCliZip = "$ToolsDir\jadx_cli_temp.zip"
if (Download-With-Curl "https://github.com/skylot/jadx/releases/download/v1.5.0/jadx-1.5.0.zip" $JadxCliZip) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($JadxCliZip)
    $entry = $zip.Entries | Where-Object { $_.FullName -like "lib/*-all.jar" } | Select-Object -First 1
    if ($null -eq $entry) {
        $zip.Dispose()
        throw "JADX CLI archive did not contain a lib/*-all.jar entry."
    }
    New-Item -ItemType Directory -Force -Path "$ToolsDir\jadx\lib" | Out-Null
    [System.IO.Compression.ZipFileExtensions]::ExtractToFile($entry, "$ToolsDir\jadx\lib\jadx-all.jar", $true)
    $zip.Dispose()
    Remove-Item $JadxCliZip
}

# 2. Clean and Setup APKTool
if (Test-Path "$ToolsDir\apktool.jar") { Remove-Item "$ToolsDir\apktool.jar" }
Download-With-Curl "https://bitbucket.org/iBotPeaches/apktool/downloads/apktool_2.9.3.jar" "$ToolsDir\apktool.jar"

# 3. Clean and Setup Platform-Tools
if (Test-Path "$ToolsDir\platform-tools") { Remove-Item -Recurse -Force "$ToolsDir\platform-tools" }
$PlatformToolsZip = "$ToolsDir\platform-tools_temp.zip"
if (Download-With-Curl "https://dl.google.com/android/repository/platform-tools-latest-windows.zip" $PlatformToolsZip) {
    Expand-Archive -Path $PlatformToolsZip -DestinationPath "$ToolsDir" -Force
    Remove-Item $PlatformToolsZip
}

# 4. Clean and Setup Uber APK Signer (Alternative to raw apksigner/zipalign for standalone usage)
if (Test-Path "$ToolsDir\uber-apk-signer.jar") { Remove-Item "$ToolsDir\uber-apk-signer.jar" }
Download-With-Curl "https://github.com/patrickfav/uber-apk-signer/releases/download/v1.3.0/uber-apk-signer-1.3.0.jar" "$ToolsDir\uber-apk-signer.jar"
$SignerDir = Join-Path $ToolsDir "apksigner"
New-Item -ItemType Directory -Force -Path $SignerDir | Out-Null
Copy-Item "$ToolsDir\uber-apk-signer.jar" "$SignerDir\uber-apk-signer.jar" -Force


Write-Host "Toolchain population finished successfully!"
