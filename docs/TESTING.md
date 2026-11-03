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

## FTP and FTPS

Manual test against a local server (Python):

```bash
python -m pip install pyftpdlib
python -m pyftpdlib -i 127.0.0.1 -p 2121 -w -u test -P test -d <folder to serve>
```

1. Press Ctrl+F (or R3), choose **Add server**, enter `ftp://test@127.0.0.1:2121/` and the password `test`.
2. Confirm the pane lists the served folder and the **Save password?** prompt appears; choose **Save password**.
3. Copy a nested local folder to the server pane and back to another local folder; check progress shows and the contents match.
4. Cancel a large transfer part-way and confirm the progress dialog reports it cancelled.
5. Create a folder, create a text file, rename it, and delete a non-empty folder on the server pane.
6. Stop the server during a transfer and confirm the error is shown.
7. Leave the pane idle past the server timeout, then navigate; the pane should reconnect without an error.
8. Restart PadCommander and connect to the saved server; it should not ask for the password.
9. Delete `secret.key` from the settings folder and connect again; it should ask for the password.
10. Press Back at the server root and confirm the pane returns to the local folder it was on before connecting.

For FTPS, use a server with a certificate signed by a public CA; self-signed certificates are rejected.
