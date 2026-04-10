# gtkwallpapers

A Rust CLI tool that runs a background daemon to automatically rotate desktop wallpapers on GNOME/GTK desktops (Ubuntu, etc.). Wallpapers are sourced from Flickr using configurable search terms and rotate on a schedule you control.

## Installation

```
cargo install --path .
```

## Commands

| Command | Description |
|---|---|
| `gtkwallpapers start` | Install the systemd user service (if needed) and start the daemon |
| `gtkwallpapers stop` | Stop the running daemon |
| `gtkwallpapers status` | Show whether the daemon is running and recent log output |
| `gtkwallpapers uninstall` | Stop and remove the systemd service unit |
| `gtkwallpapers flickr <term> [terms...]` | Add one or more Flickr search terms (e.g. `flickr mountains "sunset sky"`) |
| `gtkwallpapers flickr` | Open interactive menu to review and remove existing search terms |
| `gtkwallpapers flickrkey <key>` | Save a Flickr API key (enables higher rate limits) |
| `gtkwallpapers flickrkey` | Clear the stored Flickr API key |
| `gtkwallpapers update <frequency>` | Set the wallpaper rotation interval (e.g. `30m`, `1h`, `2h30m`) |
| `gtkwallpapers path` | Print the folder where downloaded wallpapers are stored |

## Wallpaper storage

Downloaded wallpapers are saved to:
```
$HOME/.config/gtkwallpapers/wallpapers/
```

## How it works

1. Add one or more Flickr search terms with `gtkwallpapers flickr <term> [terms...]`.
2. Start the daemon with `gtkwallpapers start` — this installs a systemd user service that persists across reboots.
3. The daemon rotates through downloaded wallpapers on the configured interval and downloads new ones from Flickr in the background.

## Configuration

Config is stored at `$HOME/.config/gtkwallpapers/config.json`.

The tool works without a Flickr API key, but unauthenticated requests are rate-limited and may return fewer results. Get a free key at [flickr.com/services/apps/create](https://www.flickr.com/services/apps/create) and register it with:

```
gtkwallpapers flickrkey <your-key>
```

To remove it:

```
gtkwallpapers flickrkey
```
