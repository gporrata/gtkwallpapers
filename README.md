# gtkwallpapers

A Rust CLI tool that runs a background daemon to automatically rotate desktop wallpapers on GNOME/GTK desktops (Ubuntu, etc.). Wallpapers are downloaded from multiple photo providers using configurable search terms and rotate on a schedule you control.

## Installation

```
cargo install --path .
```

## Getting started

1. Add API keys for the providers you want to use (see [Photo providers](#photo-providers) below).
2. Add search terms:
   ```
   gtkwallpapers terms mountains "sunset sky" ocean
   ```
3. Start the daemon:
   ```
   gtkwallpapers start
   ```

The daemon installs itself as a systemd user service, downloads wallpapers matching your search terms from all configured providers, and rotates them on the configured interval.

## Commands

| Command | Description |
|---|---|
| `gtkwallpapers start` | Install the systemd user service (if needed) and start the daemon |
| `gtkwallpapers stop` | Stop the running daemon |
| `gtkwallpapers status` | Show whether the daemon is running and recent log output |
| `gtkwallpapers uninstall` | Stop and remove the systemd service unit |
| `gtkwallpapers terms <term> [terms...]` | Add one or more search terms (e.g. `terms mountains "sunset sky"`) |
| `gtkwallpapers terms` | Open interactive menu to review and remove existing search terms |
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

API keys are set directly in `~/.config/gtkwallpapers/config.json`. Only providers with a key configured will download photos. **Wallhaven does not require a key** for SFW content.

| Provider | Key field | Get a key |
|---|---|---|
| Unsplash | `unsplash_api_key` | [unsplash.com/developers](https://unsplash.com/developers) |
| Pexels | `pexels_api_key` | [pexels.com/api](https://www.pexels.com/api/) |
| Pixabay | `pixabay_api_key` | [pixabay.com/api/docs](https://pixabay.com/api/docs/) |
| Wallhaven | `wallhaven_api_key` | [wallhaven.cc/settings/account](https://wallhaven.cc/settings/account) (optional) |

Example `config.json`:

```json
{
  "frequency_secs": 1800,
  "terms": ["mountains", "sunset sky"],
  "unsplash_api_key": "your-key-here",
  "pexels_api_key": "your-key-here",
  "pixabay_api_key": "your-key-here",
  "wallhaven_api_key": null
}
```

## Configuration

Config is stored at `~/.config/gtkwallpapers/config.json`. Changes take effect on the next daemon rotation cycle (or immediately with `gtkwallpapers next`).
