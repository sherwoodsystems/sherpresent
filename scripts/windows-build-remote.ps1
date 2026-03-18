# windows-build-remote.ps1
# Runs on the Windows VM — called by build-windows.sh via SSH
$ErrorActionPreference = "Stop"

Set-Location C:\sherpresent\apps\desktop
bun install
bun run tauri build --bundles nsis
