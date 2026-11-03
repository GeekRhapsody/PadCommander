# Build PadCommander on Linux

These instructions cover desktop distributions and the Steam Deck.

## Required Tools

- x86_64 Linux with X11 or Wayland.
- Rust 1.85 or newer with Cargo.
- System development packages for gamepad input (udev) and windowing.

Debian / Ubuntu:

```bash
sudo apt-get install -y build-essential pkg-config libudev-dev libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev
```

Fedora:

```bash
sudo dnf install -y gcc pkgconf-pkg-config systemd-devel libxkbcommon-devel wayland-devel libX11-devel libXcursor-devel libXrandr-devel libXi-devel mesa-libGL-devel
```

## Build and Run

```bash
cargo build
cargo run
cargo build --release
```

The release binary is written to `target/release/pad-commander`.

Settings are stored in `$XDG_CONFIG_HOME/PadCommander/settings.json`, falling back to `~/.config/PadCommander/settings.json`.

## Optional Tools

- `7z` / `7za` / `7zz` (p7zip or 7-Zip) on `PATH` for creating `.7z` archives.
- `rar` on `PATH` for creating `.rar` archives.
- `xdg-open` (from `xdg-utils`) for opening files with their default application.

## Steam Deck

Use the AppImage from the GitHub release; it needs no installation.

1. In Desktop Mode, download `pad-commander-<version>-linux-x86_64.AppImage` and mark it executable (Properties → Permissions, or `chmod +x`).
2. In Steam, choose **Add a Game → Add a Non-Steam Game** and select the AppImage.
3. Switch to Gaming Mode and launch PadCommander from the library. Steam Input presents the Deck controls as a standard gamepad.

SD cards and USB drives appear on the **Devices** screen (go up from `/` or a drive root) under `/run/media/deck`.

## Validation

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## Smoke Test Checklist

- App launches full-screen (also in Steam Deck Gaming Mode).
- Both panes render; the Devices screen lists `/`, the home folder and mounted removable media.
- Gamepad is detected and can move focus/selection.
- Dot-files are hidden unless **Show hidden files** is enabled.
- Delete with **Use Recycle Bin** enabled moves items to the desktop Trash.
- Opening a file launches its default application.
- Settings persist after changing UI scale, closing the app, and reopening it.
