# PadCommander Agent Guide

## Project Goal

PadCommander is a Rust full-screen graphical two-pane file manager inspired by Total Commander and Double Commander, for Windows and Linux (including Steam Deck). It must be gamepad-first while remaining fully usable by keyboard.

## Non-Negotiable Product Requirements

- Build a two-pane commander-style file manager.
- Target Windows and Linux (including Steam Deck).
- Use Rust.
- Build a full-screen graphical application, not a terminal TUI.
- Every feature must be reachable and usable with a gamepad.
- Keyboard support must remain available.
- Any text input must use a built-in on-screen keyboard that is fully gamepad usable.
- Long-running actions must not block the UI.
- Copy, move, upload, download, compress, and decompress operations must show progress.
- Settings must include UI scaling.

## Preferred Architecture

- Prefer `eframe`/`egui` for the GUI unless there is a strong reason not to.
- Use a central `AppState` with two `PaneState` instances.
- Use an action/command enum so gamepad input, keyboard input, and UI buttons all trigger the same actions.
- Keep file operations out of UI code.
- Use a background task manager for long-running operations.
- Create a `VirtualFileSystem` abstraction so local filesystem and FTP can share pane and navigation concepts.
- Keep OS-specific behavior behind small platform modules.
- Prefer safe Rust and explicit error handling.
- Avoid panics in app logic.
- Use tests for pure logic and service modules where practical.

## Expected Module Boundaries

- UI code should render state and dispatch actions; it should not perform filesystem work directly.
- Input handling should translate gamepad, keyboard, and pointer/UI events into shared actions.
- File operations should live in service modules and report progress through background task state.
- VFS implementations should share navigation, listing, metadata, and operation concepts through traits or explicit interfaces.
- Platform-specific Windows and Linux behavior should be isolated in narrow modules (`src/platform/windows.rs`, `src/platform/linux.rs`) that expose the same API.

## Gamepad-First UX Rules

- Treat gamepad access as a primary requirement, not an enhancement.
- Do not add a workflow that requires a mouse or physical keyboard.
- Use predictable focus, selection, and confirmation behavior.
- Make destructive actions confirmable through gamepad navigation.
- Ensure dialogs, settings, progress views, context menus, and text input are fully gamepad usable.
- Keep keyboard shortcuts available and map them to the same action enum used by gamepad controls.

## Long-Running Work

- Never block the UI thread with copy, move, upload, download, compress, decompress, scanning, or network operations.
- Long-running operations must run through the background task manager.
- Progress-capable operations must expose visible progress, status, and error information.
- Prefer cancellable tasks where practical.

## Quality Expectations

Before finishing any implementation task:

- Run `cargo fmt`.
- Run `cargo test` where applicable.
- Run `cargo clippy --all-targets --all-features -- -D warnings` when dependencies and platform support allow it.
- If a command cannot run in the current environment, explain exactly why.
- Keep changes scoped to the requested milestone.
- Do not implement unrelated future features early unless a small interface is needed to avoid rework.

## Error Handling

- Prefer explicit errors over panics in app logic.
- Surface user-relevant errors in the UI.
- Keep low-level error details available for diagnostics where practical.
- Avoid `unwrap` and `expect` outside tests, prototypes, and carefully justified initialization paths.

## Scope Discipline

- Support only Windows and Linux unless the user explicitly changes it.
- Avoid adding cross-platform abstractions beyond what is useful for keeping OS-specific behavior isolated.
- Keep each change focused on the requested milestone.
- Favor small, testable service and state modules over broad rewrites.
