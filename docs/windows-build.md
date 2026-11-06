# Windows Build

Produces Windows installers (NSIS `.exe` + `.msi`) using a Dockurr-based Windows 11 VM running on the Linux host via Docker + KVM.

## Prerequisites

- Docker with compose plugin
- KVM support on the host (`ls -la /dev/kvm`)
- Ports 2222, 3389, and 8006 available

## First-Time Setup

1. Start the VM:

   ```bash
   docker compose -f docker-compose.windows.yml up -d
   ```

2. Monitor setup progress via noVNC at `http://localhost:8006`. The OEM scripts (`oem/install.bat` → `oem/setup.ps1`) run automatically on first boot and install:

   - Visual Studio Build Tools 2022 (C++ workload + Windows 11 SDK)
   - Rust (stable via rustup)
   - Bun
   - NSIS 3.10
   - Git
   - OpenSSH Server

   This takes ~20-30 minutes. SSH becomes available on port 2222 once complete.

3. Verify SSH access:

   ```bash
   ssh -p 2222 Docker@localhost "rustc --version && bun --version"
   ```

   Default credentials: `Docker` / `admin`

## Building

```bash
bash scripts/build-windows.sh
```

This will:

1. Start the VM (if not already running)
2. Wait for SSH connectivity
3. Sync project files to the VM via tar-over-SSH
4. Run `bun install` + `bun run tauri build` inside the VM
5. Copy NSIS and MSI artifacts back to `apps/desktop/src-tauri/target/release/bundle/`
6. Stop the VM

Use `--no-stop` to keep the VM running after the build (useful for debugging or repeated builds).

## Files

| File | Location | Purpose |
|------|----------|---------|
| `docker-compose.windows.yml` | Project root | VM configuration (8GB RAM, 4 CPUs, 64GB disk) |
| `oem/install.bat` | `oem/` | First-boot entry point |
| `oem/setup.ps1` | `oem/` | Toolchain installation script |
| `scripts/build-windows.sh` | `scripts/` | Host-side build orchestration |
| `scripts/windows-build-remote.ps1` | `scripts/` | VM-side build commands |

## Troubleshooting

- **SSH not connecting**: Check VM status via noVNC at `http://localhost:8006` or `docker logs sherpresent-windows-build`
- **Build fails on first run**: Ensure the OEM setup completed successfully — SSH availability is the signal that setup is done
- **KVM not available**: Ensure your kernel supports KVM and your user is in the `kvm` group (`sudo usermod -aG kvm $USER`)
