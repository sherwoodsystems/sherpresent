$ErrorActionPreference = "Stop"

Write-Host "=== Sherpresent Windows Build VM Setup ===" -ForegroundColor Cyan

# --- Visual Studio Build Tools 2022 (C++ workload) ---
Write-Host "`n[1/6] Installing Visual Studio Build Tools 2022..." -ForegroundColor Yellow
$vsUrl = "https://aka.ms/vs/17/release/vs_BuildTools.exe"
$vsInstaller = "$env:TEMP\vs_BuildTools.exe"
Invoke-WebRequest -Uri $vsUrl -OutFile $vsInstaller
Start-Process -Wait -FilePath $vsInstaller -ArgumentList `
    "--quiet", "--wait", "--norestart", "--nocache", `
    "--add", "Microsoft.VisualStudio.Workload.VCTools", `
    "--add", "Microsoft.VisualStudio.Component.Windows11SDK.22621", `
    "--includeRecommended"
Write-Host "  Visual Studio Build Tools installed." -ForegroundColor Green

# --- Rust via rustup ---
Write-Host "`n[2/6] Installing Rust..." -ForegroundColor Yellow
$rustupUrl = "https://win.rustup.rs/x86_64"
$rustupInstaller = "$env:TEMP\rustup-init.exe"
Invoke-WebRequest -Uri $rustupUrl -OutFile $rustupInstaller
Start-Process -Wait -FilePath $rustupInstaller -ArgumentList "-y", "--default-toolchain", "stable"
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
Write-Host "  Rust installed: $(rustc --version)" -ForegroundColor Green

# --- Bun ---
Write-Host "`n[3/6] Installing Bun..." -ForegroundColor Yellow
irm bun.sh/install.ps1 | iex
$env:PATH = "$env:USERPROFILE\.bun\bin;$env:PATH"
Write-Host "  Bun installed: $(bun --version)" -ForegroundColor Green

# --- NSIS ---
Write-Host "`n[4/6] Installing NSIS..." -ForegroundColor Yellow
$nsisUrl = "https://sourceforge.net/projects/nsis/files/NSIS%203/3.10/nsis-3.10-setup.exe/download"
$nsisInstaller = "$env:TEMP\nsis-setup.exe"
Invoke-WebRequest -Uri $nsisUrl -OutFile $nsisInstaller -UserAgent "Mozilla/5.0"
Start-Process -Wait -FilePath $nsisInstaller -ArgumentList "/S"
$env:PATH = "C:\Program Files (x86)\NSIS;$env:PATH"
Write-Host "  NSIS installed." -ForegroundColor Green

# --- Git ---
Write-Host "`n[5/6] Installing Git..." -ForegroundColor Yellow
$gitUrl = "https://github.com/git-for-windows/git/releases/download/v2.47.1.windows.1/Git-2.47.1-64-bit.exe"
$gitInstaller = "$env:TEMP\git-setup.exe"
Invoke-WebRequest -Uri $gitUrl -OutFile $gitInstaller
Start-Process -Wait -FilePath $gitInstaller -ArgumentList "/VERYSILENT", "/NORESTART"
Write-Host "  Git installed." -ForegroundColor Green

# --- OpenSSH Server ---
Write-Host "`n[6/6] Enabling OpenSSH Server..." -ForegroundColor Yellow
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
Set-Service -Name sshd -StartupType Automatic
Start-Service sshd
New-NetFirewallRule -Name "OpenSSH-Server" -DisplayName "OpenSSH Server (sshd)" `
    -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22 `
    -ErrorAction SilentlyContinue
Write-Host "  OpenSSH Server enabled and started." -ForegroundColor Green

# --- Create project directory ---
$projectDir = "C:\sherpresent"
if (-not (Test-Path $projectDir)) {
    New-Item -ItemType Directory -Path $projectDir | Out-Null
}
Write-Host "`nProject directory: $projectDir" -ForegroundColor Cyan

# --- Persist PATH additions (Machine scope so SSH sessions can see them) ---
$machinePath = [Environment]::GetEnvironmentVariable("PATH", "Machine")
$additions = @(
    "$env:USERPROFILE\.cargo\bin",
    "$env:USERPROFILE\.bun\bin",
    "C:\Program Files (x86)\NSIS",
    "C:\Program Files\Git\bin"
)
foreach ($dir in $additions) {
    if ($machinePath -notlike "*$dir*") {
        $machinePath = "$dir;$machinePath"
    }
}
[Environment]::SetEnvironmentVariable("PATH", $machinePath, "Machine")

# --- Set PowerShell as default SSH shell ---
New-ItemProperty -Path 'HKLM:\SOFTWARE\OpenSSH' -Name DefaultShell `
    -Value 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' `
    -PropertyType String -Force | Out-Null
Write-Host "  Default SSH shell set to PowerShell." -ForegroundColor Green

Write-Host "`n=== Setup Complete ===" -ForegroundColor Cyan
Write-Host "Verify with: rustc --version && bun --version && git --version"
Write-Host "SSH should now be available on port 22 (mapped to 2222 on host)."
