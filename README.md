# wsl-devctl

A terminal UI for managing WSL development servers. Built with Ratatui in Rust.

## What It Does

wsl-devctl gives you a unified view of your dev environment:

- **PM2 processes** — Node.js apps, Python services, background workers
- **Caddy static servers** — Static file serving roots and ports
- **Port conflict detection** — See collisions between PM2, Caddy, and manual ports
- **Quick actions** — Restart, stop/start, view logs, open in browser

## Installation

```bash
# Clone and build
git clone https://github.com/solomonneas/wsl-devctl.git
cd wsl-devctl
cargo build --release

# Install to ~/.cargo/bin
cargo install --path .
```

## Usage

```bash
# Run with defaults (2s refresh)
wsl-devctl

# Slower refresh
wsl-devctl --refresh 5

# Watch additional manual ports
wsl-devctl --manual-ports 3000,5173,8080
```

## Controls

| Key | Action |
|-----|--------|
| `↑/↓` or `j/k` | Navigate |
| `Enter` | Open in browser |
| `r` | Restart PM2 process |
| `s` | Stop/start PM2 process |
| `l` | View recent logs |
| `/` or `f` | Filter/search |
| `q` or `Esc` | Quit |

Mouse clicks supported on action buttons.

## Requirements

- WSL2 with Ubuntu/Debian
- PM2 installed (`npm install -g pm2`)
- Caddy (optional, for static server detection)

## WSL Integration

Pairs with [wsl-bridge](https://github.com/solomonneas/wsl-bridge) for automatic port forwarding to Windows.

## License

MIT
