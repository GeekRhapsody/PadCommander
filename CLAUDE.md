# CLAUDE.md

PadCommander is a full-screen, gamepad-first two-pane file manager (Total Commander style) written in Rust with `eframe`/`egui`, for Windows and Linux (including Steam Deck). Every feature must work with a gamepad alone; the keyboard stays fully supported. Product rules, architecture preferences and quality gates live in [AGENTS.md](AGENTS.md) — read and follow it too.

## Commands

```powershell
cargo build
cargo run
cargo test
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
```

[.github/workflows/ci.yml](.github/workflows/ci.yml) runs `cargo fmt --check` and `cargo test` on Windows and Ubuntu for pushes to `main` and PRs. [.github/workflows/release.yml](.github/workflows/release.yml) runs on `v*` tags and publishes a Windows zip, a Linux tarball and an AppImage. Linux builds need `libudev-dev` and the X11/Wayland dev packages (see [docs/BUILD_LINUX.md](docs/BUILD_LINUX.md)); this Windows machine cannot cross-check Linux, so CI is the Linux compile check.

## How the app is wired

- [src/main.rs](src/main.rs) → [src/app.rs](src/app.rs): `PadCommanderApp` owns `AppState` and `InputManager`. Each frame it polls background tasks, collects input actions, renders the UI, and dispatches every action through `AppState::dispatch_action`.
- [src/action.rs](src/action.rs): `CommanderAction` / `TextInputAction`. Gamepad, keyboard and UI clicks all produce these. New features get a variant here, not a separate input path.
- [src/input/mod.rs](src/input/mod.rs): keyboard (egui) and gamepad (`gilrs`) mapping to actions, including key repeat and context-dependent mappings (dialog open, settings, favourites, modal).
- [src/state/](src/state/): `mod.rs` holds `AppState` (two `PaneState`s, dialog, settings, favourites, task manager, status line) and all action handling, with most tests at the bottom. `pane.rs` has `PaneState`, `FileEntry`, selection/marking and sorting; `dialogs.rs` the `DialogState` enum and per-modal state; `text_input.rs` the on-screen keyboard state and filename validation. `mod.rs` re-exports their public items, so callers use `crate::state::X`.
- [src/ui/](src/ui/): pure rendering. Functions take `&AppState` and return `Vec<CommanderAction>`; they never mutate state or touch the filesystem. `panes.rs` draws the panes, `text_input.rs` the on-screen keyboard, `mod.rs` the top/hint bars, command sheet and modals, `button_images.rs` per-controller button glyphs.
- [src/fs/mod.rs](src/fs/mod.rs): `VirtualFileSystem` trait + `LocalFileSystem`, directory listing, drive roots (`ROOTS_LOCATION` is `""`), create/rename/delete, `VfsError` with `user_message()`.
- [src/tasks/mod.rs](src/tasks/mod.rs): `TaskManager` runs long work on threads (`spawn_threaded`), reports progress via `TaskReporter`, supports cancellation, and returns `TaskCompletion`s that `AppState::poll_tasks` consumes.
- [src/ftp/mod.rs](src/ftp/mod.rs): FTP/FTPS service via `suppaftp` (rustls). `FtpServer` (saved in settings), `parse_address`, `ftp://host:port/path` location helpers, `FtpConnection` (a pane's session behind a mutex; reconnects once when the server dropped an idle connection), listing, and the upload/download/delete task bodies. Pane listings and edits run on threads and report back through `FtpEvent`s that `AppState::poll_tasks` drains; transfers and deletes open their own connection through `TaskManager::spawn_threaded`.
- [src/secrets.rs](src/secrets.rs): seals saved FTP passwords with XChaCha20-Poly1305 using a random key in `secret.key` beside `settings.json`.
- [src/archive.rs](src/archive.rs): zip via the `zip` crate, extraction via `unarc-rs`, and 7z/rar creation through external `7z`/`rar` executables found on disk.
- [src/platform/](src/platform/): `mod.rs` has the shared window options and re-exports the OS module. `windows.rs` (Win32: drives, volume labels, Recycle Bin, shell open, `.lnk` shortcuts, free space, `%APPDATA%`) and `linux.rs` (`/`, home and `/run/media` mounts, freedesktop Trash, `xdg-open`, `statvfs`, `~/.config`) expose the same functions and constants. Keep OS API usage here; callers use `crate::platform::x`.
- [src/settings.rs](src/settings.rs): JSON settings at `%APPDATA%\PadCommander\settings.json` (Windows) or `~/.config/PadCommander/settings.json` (Linux) (UI scale, theme, controller override, recycle bin, hidden files).
- [src/theme.rs](src/theme.rs): colors and fonts for Dark/Light.

FTP, upload and download actions exist in the enum but are stubs that set a status message.

## Code-writing rules

- Keep code small, flat and easy to follow.
- Don't add single-use helpers. Only create a new function or method if it's called from 2+ places or the request asks for one. Never wrap one statement in a method.
- Don't handle edge cases nobody mentioned. Only code for cases the request names or that the existing code already handles. If you think something else is a real risk, list it in your reply and ask. Don't code it in.
- Only comment the why. Don't add comments that restate the code.
- Keep nesting to 3 levels or fewer. Use early returns and guard clauses instead of nested `if`/`else`.
- No speculative abstractions.
- Stay inside the request. No drive-by refactors, renames or reformatting of code the request doesn't touch. Mention anything worth changing in your reply instead.
