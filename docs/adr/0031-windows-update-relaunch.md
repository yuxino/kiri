# ADR 0031: Windows update completion and download feedback

- Status: Accepted
- Date: 2026-09-12
- Amends: ADR 0025 (Windows relaunch only)

The Windows update flow closed Kiri after installation without reopening it.
Users also need to distinguish downloading from signature verification.

Keep manual, separate check, download and install actions. On Windows the install
button explicitly says **Install and Restart** and passes `restartAfterInstall:
true` to the updater. The passive NSIS updater replaces the application in its
existing location and reopens it. macOS keeps its explicit post-install restart
button. Neither platform checks or downloads updates in the background.

Display downloaded bytes and the total/percentage when known. Without a total,
show bytes and indeterminate progress. The transport Finished event enters a
verification state; only successful completion of `update.download()` makes the
install action available. Failed signature verification cannot proceed to install.

An ordinary manually launched NSIS installer can offer uninstall/reinstall.
The in-app NSIS updater supplies `/P /UPDATE`; an existing NSIS installation is
updated without that choice. Migration from a legacy MSI installation is separate.
Updater signatures authenticate release payloads; they are independent of
Windows Authenticode and Apple Developer ID/notarization.
