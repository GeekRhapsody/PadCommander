# Build PadCommander on Windows

PadCommander is a Rust desktop app for Windows and Linux. These instructions cover Windows: they cover local debug builds, local runs, release builds, and the manual smoke test expected before sharing a binary.

## Required Tools

- Windows 10 or newer.
- Rust 1.85 or newer with Cargo.
- Microsoft C++ Build Tools or Visual Studio with the Desktop development with C++ workload.

No additional runtime system dependencies are currently required. Gamepad support is provided through the Rust dependencies in `Cargo.toml`.

## Build and Run

From the repository root:

```powershell
cargo build
```

Run a debug build:

```powershell
cargo run
```

Build the optimized release executable:

```powershell
cargo build --release
```

The release binary is written to:

```text
target\release\pad-commander.exe
```

The executable includes the Windows app icon from `assets\windows\app.ico`.

## Validation

Before publishing or sharing a release build, run:

```powershell
cargo fmt --check
cargo check
cargo test
```

When dependency and platform support allow it, also run:

```powershell
cargo clippy --all-targets --all-features -- -D warnings
```

## Smoke Test Checklist

- App launches full-screen.
- Both commander panes render.
- Gamepad is detected and can move focus/selection.
- Local navigation works in both panes.
- Settings persist after changing UI scale, closing the app, and reopening it.

## Release Notes

Installer technology is not included yet. Future release work can add an installer or packaging flow once the executable behavior and release checklist are stable.
