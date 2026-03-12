# Deployment

## Bridge Deployment to Raspberry Pi

### Option 1: Self-Extracting Installer (Recommended)

Build the installer on your dev machine:

```bash
cd apps/bridge/scripts
./build-installer.sh
```

Deploy to the Pi:

```bash
scp rpi-osc-bridge-installer.run pi@<PI_IP>:/tmp/
ssh pi@<PI_IP> 'sudo bash /tmp/rpi-osc-bridge-installer.run'
```

### Option 2: Direct Install

Copy the bridge directory to the Pi and run the install script:

```bash
scp -r apps/bridge pi@<PI_IP>:/tmp/bridge
ssh pi@<PI_IP> 'sudo bash /tmp/bridge/scripts/install.sh'
```

### Installed Layout

The installer deploys a flat structure to `/opt/rpi-osc-bridge/`:

```
/opt/rpi-osc-bridge/
├── bridge.py
├── config-server.py
├── config.example.json
└── web/
    └── index.html

/etc/rpi-osc-bridge/
└── config.json

/etc/systemd/system/
├── rpi-osc-bridge.service
└── config-server.service
```

### Service Management

```bash
sudo systemctl start rpi-osc-bridge
sudo systemctl enable rpi-osc-bridge   # Start on boot
sudo systemctl status rpi-osc-bridge
sudo journalctl -u rpi-osc-bridge -f   # View logs
```

### Uninstalling

```bash
sudo bash /opt/rpi-osc-bridge/uninstall.sh
# Or from the repo:
sudo bash apps/bridge/scripts/uninstall.sh
```
