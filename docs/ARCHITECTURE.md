# PadCommander Architecture

PadCommander is planned as a Windows-only Rust full-screen graphical two-pane file manager built with `eframe`/`egui`.

## Planned Module Layout

- `main.rs`: application entry point, native window setup, and full-screen configuration.
- `app.rs`: top-level `eframe` app integration and frame lifecycle.
- `state.rs`: central `AppState`, including two `PaneState` values, settings, focus, dialogs, and task summaries.
- `pane.rs`: pane navigation state, selection state, sorting, filtering, and active item tracking.
- `action.rs`: shared action/command enum used by gamepad input, keyboard input, and UI controls.
- `input/`: gamepad and keyboard input mapping into shared actions.
- `ui/`: egui rendering for panes, command bars, dialogs, progress, settings, and the on-screen keyboard.
- `vfs/`: `VirtualFileSystem` traits and shared file/navigation types.
- `vfs/local.rs`: local Windows filesystem implementation.
- `vfs/ftp.rs`: FTP implementation once network support is added.
- `ops/`: copy, move, delete, compress, decompress, upload, and download services.
- `tasks/`: background task manager, task progress, cancellation, and completion reporting.
- `platform/windows.rs`: small Windows-specific helpers and integration points.
- `settings.rs`: persisted settings, including UI scaling.
- `error.rs`: application error types and conversion helpers.

## Core Flow

Gamepad, keyboard, and UI events are translated into `Action` values. The app applies actions to `AppState`, starts background tasks when needed, and renders the latest state each frame. UI modules should not perform filesystem operations directly.

## Testing Focus

Pure logic and service modules should be tested where practical, especially pane navigation, selection behavior, action handling, path handling, VFS behavior, operation planning, settings parsing, and task state transitions.
