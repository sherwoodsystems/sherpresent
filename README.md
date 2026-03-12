# SherPresent

Cross-platform desktop application for remotely controlling presentation software (PowerPoint, Keynote, LibreOffice Impress) via OSC (Open Sound Control) protocol.

## Apps

| App | Description | Tech |
|-----|-------------|------|
| [Desktop](apps/desktop/) | Main presentation controller | Tauri v2, SvelteKit, Rust |
| [Bridge](apps/bridge/) | USB clicker → OSC bridge for Raspberry Pi | Python |
| [Companion Module](apps/companion-module/) | Bitfocus Companion integration | TypeScript |

## Quick Start

### Desktop App

```bash
cd apps/desktop
bun install
bun tauri dev
```

### Bridge (Raspberry Pi)

```bash
cd apps/bridge/scripts
./build-installer.sh
# Deploy the .run file to your Pi
```

## Documentation

- [Getting Started](docs/getting-started.md)
- [Development](docs/development.md)
- [Deployment](docs/deployment.md)
- [Architecture](docs/architecture.md)

## Specifications

- [OSC Protocol](spec/osc-protocol.md)
- [Desktop App](spec/desktop-app.md)
- [Bridge](spec/bridge.md)
- [Config Formats](spec/config-formats.md)
- [Platform Adapters](spec/platform-adapters.md)

## License

MIT + Commons Clause — see [LICENSE](LICENSE)
