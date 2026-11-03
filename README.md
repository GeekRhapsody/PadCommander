# PadCommander

<!-- Logo placeholder -->
<!-- <p align="center">
  <img src="assets/logo.png" alt="PadCommander Logo" width="200"/>
</p> -->

<p align="center">
  <strong>A gamepad-first dual-pane file manager for Windows and Linux</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux-blue" alt="Platform: Windows | Linux"/>
  <img src="https://img.shields.io/badge/built%20with-Rust-orange" alt="Built with Rust"/>
</p>

---

PadCommander is a full-screen, gamepad-first two-pane file manager for Windows and Linux (including Steam Deck), inspired by Total Commander and Double Commander. Navigate your files with an Xbox, PlayStation, or Nintendo Switch controller — or stick to the keyboard if you prefer.

<p align="center">
  <img src="assets/screenshots/main-view.png" alt="Main dual-pane view" width="700"/>
  <br/>
  <em>Dual-pane navigation with gamepad input</em>
</p>
<p align="center">
  <img src="assets/screenshots/actions.png" alt="Actions menu" width="700"/>
  <br/>
  <em>Actions menu for copying, moving, archiving and organizing marked files</em>
</p>
<p align="center">
  <img src="assets/screenshots/osk.png" alt="On-screen keyboard" width="700"/>
  <br/>
  <em>Built-in on-screen keyboard for controller-driven text entry</em>
</p>

## Features

- **Dual-pane commander layout** — browse two directories side by side
- **Gamepad as first-class input** — full navigation and file operations from a controller
- **On-screen keyboard** — enter filenames and paths without touching a keyboard
- **File operations** — copy, move, delete, compress, and decompress archives
- **FTP and FTPS servers** — open a server in either pane and copy or move files to and from it, with saved servers and optional saved passwords
- **Controller auto-detection** — supports Xbox, PlayStation 4/5, and Nintendo Switch controllers
- **Theme support** — Dark and Light modes
- **Configurable UI scale** — adjust the interface scale to suit your display

## Requirements

- Windows 10 or later, or Linux (x86_64, X11 or Wayland; Steam Deck supported)
- Rust 1.85+
- Windows: MSVC C++ Build Tools
- Linux: see [docs/BUILD_LINUX.md](docs/BUILD_LINUX.md) for the system packages

## Building from Source

```powershell
# Debug build
cargo build

# Run directly
cargo run

# Optimized release build
cargo build --release
```

The release binary is output to `target\release\pad-commander.exe` on Windows and `target/release/pad-commander` on Linux.

## Usage

Launch the application and use your gamepad or keyboard to navigate. The two panes work independently — switch between them to copy or move files across directories.

### Gamepad Controls

| Action | Button |
|---|---|
| Move selection | D-pad up / down |
| Switch pane | D-pad left / right |
| Page up / down | LB / RB |
| Confirm / Open | A (Xbox) / Cross (PS) |
| Back / Up a directory | B (Xbox) / Circle (PS) |
| Mark item | X (Xbox) / Square (PS) |
| Actions menu | Y (Xbox) / Triangle (PS) |
| Favourites | L3 (press left stick) |
| Servers (FTP) | R3 (press right stick) |
| Settings | Start / Options |
| Input help | View / Select |

In the Favourites and Servers lists, Y adds an entry and X removes the focused one.

### Keyboard Controls

| Action | Key |
|---|---|
| Navigate | Arrow keys |
| Confirm / Open | Enter |
| Parent folder | Backspace |
| Switch pane | Tab |
| Mark item | Space |
| Rename | F2 |
| Favourites | F3 / Ctrl+D |
| Copy | F5 |
| Move | F6 |
| New folder | F7 |
| Delete | F8 / Delete |
| Actions menu | F9 |
| Settings | F10 |
| Servers (FTP) | Ctrl+F |
| Input help | F1 |

In the Favourites and Servers lists, Insert adds an entry and Delete removes the focused one.

### FTP Servers

Open **Servers** (Ctrl+F / R3), choose **Add server** and type an address such as `ftp://user@nas.local/media` or `ftps://user@example.com:21/` on the on-screen keyboard. Leave out the user for anonymous FTP. After the first successful login you can save the password on the device; it is encrypted with a key kept in `secret.key` next to `settings.json`, so the settings file alone cannot reveal it. With one pane on a server and the other on a local folder, Copy and Move upload or download. Back out of the server root, or pick **Disconnect from server** in the actions menu, to return to the local folder.

## Configuration

Settings are saved automatically and persist between sessions. You can configure:

- **Theme** — Dark or Light
- **UI Scale** — adjust interface size (minimum 0.5×)
- **Controller type** — auto-detect or force a specific controller layout (Xbox, PS4/5, Switch)
- **Servers** — saved FTP/FTPS servers and, if you chose to save them, their encrypted passwords

## Tech Stack

| Component | Library |
|---|---|
| GUI | [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) |
| Gamepad input | [gilrs](https://gitlab.com/gilrs-project/gilrs) |
| Archive support | zip, unarc-rs |
| FTP / FTPS | suppaftp (rustls) |
| Saved passwords | chacha20poly1305 |
| Platform APIs | windows, windows-sys (Windows); libc, trash (Linux) |
| Serialization | serde / serde_json |

## License

[MIT](LICENSE)

---

<p align="center">
  <a href="https://ko-fi.com/claudiopereira">☕ Support this project on Ko-fi</a>
</p>
