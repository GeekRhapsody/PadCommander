# Testing Notes

## Create EXE Shortcut

Manual Windows test:

1. Open PadCommander on Windows.
2. Navigate one pane to a folder containing an `.exe` file.
3. Navigate the other pane to a writable folder.
4. Select the `.exe` file and invoke `Shortcut`.
5. Confirm the on-screen keyboard opens with the EXE parent folder name as the default shortcut name.
6. Submit the name and confirm that `[entered name].lnk` appears in the other pane.
7. Open the shortcut in Windows Explorer and verify it launches the selected EXE.
8. Repeat the action with the same target and verify the overwrite/cancel prompt is usable with gamepad or keyboard.
