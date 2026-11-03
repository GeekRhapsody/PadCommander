# PadCommander Architecture

PadCommander is a Rust full-screen graphical two-pane file manager for Windows and Linux (including Steam Deck), built with `eframe`/`egui`.

## Module Layout

- `main.rs`: application entry point; starts `eframe` with the native window options.
- `app.rs`: top-level `eframe` app. Each frame it polls background tasks, collects input actions, renders the UI, and dispatches the resulting actions.
- `action.rs`: shared `CommanderAction` and `TextInputAction` enums used by gamepad input, keyboard input, and UI controls.
- `state/`: central `AppState` with two `PaneState` values, settings, favourites, dialogs, and all action handling (`mod.rs`); pane entries, selection, and sorting (`pane.rs`); dialog and modal state (`dialogs.rs`); on-screen keyboard state and filename validation (`text_input.rs`).
- `input/`: gamepad (`gilrs`) and keyboard mapping into shared actions, including key repeat.
- `ui/`: egui rendering for panes, top and hint bars, command sheet, modals, progress, settings, controller button glyphs, and the on-screen keyboard.
- `fs/`: `VirtualFileSystem` trait, the `LocalFileSystem` implementation, drive roots, create/rename/delete, and `VfsError`.
- `tasks/`: background task manager, task progress, cancellation, and completion reporting.
- `ftp/`: FTP and FTPS client service (`suppaftp` with rustls): server addresses, `ftp://host:port/path` locations, a per-pane connection, listing, and upload/download/delete task bodies.
- `secrets.rs`: encrypts saved FTP passwords with a device key stored next to the settings file.
- `archive.rs`: compress and decompress for `.zip`, `.7z`, and `.rar`, using the `zip` and `unarc-rs` crates and external `7z`/`rar` executables where needed.
- `platform/`: native window options (`mod.rs`) plus one module per OS with the same API. `windows.rs`: drives and volume labels, Recycle Bin, shell open, `.lnk` shortcuts, free disk space, config folder. `linux.rs`: `/`, home and removable-media mounts, freedesktop Trash, `xdg-open`, free disk space, XDG config folder; shortcuts are unsupported.
- `settings.rs`: persisted JSON settings in the platform config folder (`%APPDATA%\PadCommander` or `~/.config/PadCommander`), including UI scaling.
- `theme.rs`: Dark and Light colors and fonts.

## Core Flow

Gamepad, keyboard, and UI events are translated into `CommanderAction` values. `AppState::dispatch_action` applies them, starting background tasks through the task manager when work is long-running. UI modules render the latest `AppState` each frame and return actions; they do not mutate state or perform filesystem operations directly.

A pane whose location starts with `ftp://` or `ftps://` has an FTP session in `AppState`. Its listings and small edits (create, rename) run on background threads and return through a channel that `poll_tasks` drains each frame; uploads, downloads, and deletes are regular cancellable tasks with their own connection, so browsing stays responsive. Local listings remain synchronous.

## Testing Focus

Pure logic and service modules are tested in place with `#[cfg(test)]` modules, especially pane navigation, selection behavior, action handling, path handling, filesystem behavior, archive path safety, settings parsing, and task state transitions.
