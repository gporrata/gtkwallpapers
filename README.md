# gtkwallpapers

A Rust CLI tool that runs a background daemon to automatically rotate desktop wallpapers on GNOME/GTK desktops (Ubuntu, etc.). Wallpapers are downloaded from multiple photo providers using configurable search terms and rotate on a schedule you control.

## Installation

**1. Install system dependencies**

The system tray icon requires DBus development headers:

```
# Debian / Ubuntu
sudo apt install libdbus-1-dev pkg-config

# Fedora / RHEL
sudo dnf install dbus-devel pkgconf-pkg-config
```

**2. Install the binary**

```
cargo install --path .
```

**3. GNOME tray icon support** (Ubuntu 22.04+)

GNOME does not show tray icons by default. Install the [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) extension, then log out and back in. The tray icon will appear in the top bar when the daemon is running.

## Getting started

1. Add API keys for the providers you want to use (see [Photo providers](#photo-providers) below):
   ```
   gtkwallpapers key unsplash <key>
   gtkwallpapers key pexels <key>
   gtkwallpapers key pixabay <key>
   ```
2. Add search terms:
   ```
   gtkwallpapers terms mountains "sunset sky" ocean
   ```
3. Download an initial set of wallpapers:
   ```
   gtkwallpapers init
   ```
4. Start the daemon:
   ```
   gtkwallpapers start
   ```

The daemon installs itself as a systemd user service and rotates through your downloaded wallpapers on the configured interval, fetching new ones from all configured providers in the background.

## Commands

| Command | Description |
|---|---|
| `gtkwallpapers start` | Install the systemd user service (if needed) and start the daemon |
| `gtkwallpapers stop` | Stop the running daemon |
| `gtkwallpapers status` | Show whether the daemon is running and recent log output |
| `gtkwallpapers uninstall` | Stop and remove the systemd service unit |
| `gtkwallpapers terms <term> [terms...]` | Add one or more search terms (e.g. `terms mountains "sunset sky"`) |
| `gtkwallpapers terms` | Open interactive list — ↑↓ to navigate, Del to remove a term, Esc to exit |
| `gtkwallpapers key <provider> <key>` | Set an API key for a provider (unsplash, pexels, pixabay, wallhaven) |
| `gtkwallpapers key <provider>` | Clear the API key for a provider |
| `gtkwallpapers init` | Download wallpapers from all configured photo providers |
| `gtkwallpapers next` | Switch to the next wallpaper immediately |
| `gtkwallpapers update <frequency>` | Set the wallpaper rotation interval (e.g. `30m`, `1h`, `2h30m`) |
| `gtkwallpapers path` | Print the folders where downloaded wallpapers are stored |

## Photo providers

Wallpapers are stored per-provider under `~/.config/gtkwallpapers/`:

```
~/.config/gtkwallpapers/
  unsplash/
  pexels/
  pixabay/
  wallhaven/
```

Use `gtkwallpapers key <provider> <key>` to register a key. Only providers with a key configured will download photos. **Wallhaven does not require a key** for SFW content.

| Provider | Required | Get a key |
|---|---|---|
| Unsplash | Yes | [unsplash.com/developers](https://unsplash.com/developers) |
| Pexels | Yes | [pexels.com/api](https://www.pexels.com/api/) |
| Pixabay | Yes | [pixabay.com/api/docs](https://pixabay.com/api/docs/) |
| Wallhaven | No | [wallhaven.cc/settings/account](https://wallhaven.cc/settings/account) |

## Configuration

Config is stored at `~/.config/gtkwallpapers/config.json`. Changes take effect on the next daemon rotation cycle (or immediately with `gtkwallpapers next`).
