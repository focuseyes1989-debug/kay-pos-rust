# KAY POS Windows updates

Update repository: https://github.com/focuseyes1989-debug/kay-pos-updates
It contains public binary releases only, NOT application source or shop data.

## Client workflow

- Settings > App Updates checks GitHub on demand, without requiring a GitHub account.
- Finish work and sign out. Check for Updates on the login screen, then Download & Install.
- Sign-in and database editing are disabled during the operation. An unresolved checkout or another running client on the same Windows account/database blocks installation.
- The downloader requires a signed ZIP with the exact version/platform filename. It checks the GitHub digest when present, Ed25519 signature, and the extracted EXE's version self-check before replacement.
- Restart KAY POS after installation. The previous executable is retained as `pos_desktop.previous.exe` beside the installed file. To roll back manually, close KAY POS and replace the EXE with that backup.
- There is no automatic post-launch health rollback. The self-check verifies launchability/version, not live database or printer behavior.
- Keep the application in a writable local folder, not Program Files or a shared/network EXE. WebView2 and existing database configuration remain required.
- No automatic database migrations. Only stable patch upgrades within the same major/minor series are accepted. Schema-breaking changes require a managed server backup/migration and a new baseline.
- Network/API errors do not prevent using the installed version. No automatic installation or forced restart.

## Publisher workflow

1. Increment `crates/pos_desktop/Cargo.toml` version for each new release; update Cargo.lock.
2. Run unit/integration tests and validate printing on a test client.
3. Run `scripts/package-update.ps1` in a PowerShell environment allowed to execute local scripts. It builds, performs the EXE self-check, archives only `pos_desktop.exe`, signs and verifies the package. It never uploads anything.
4. Create a GitHub release tagged exactly `v<version>`, upload only the signed `kay-pos-<version>-x86_64-pc-windows-msvc.zip`, and include release notes. Do not rename the ZIP after signing. Draft/prerelease builds are not for the stable update channel.
5. Pilot the update on one client before rolling out to all clients. Do not edit/replace an already published version.

`scripts/publish-update.ps1 -Version 0.2.1 -NotesFile <notes.md>` can create a draft and upload the allowlisted, verified package using the publisher's existing Git Credential Manager login. Add `-Publish` only when ready for clients. It does not publish source code. The initial 0.2.0 release has built-in notes; later versions require their own notes file.

The first updater-enabled baseline is 0.2.0 and needs one manual installation on existing clients. Subsequent compatible patch releases use the built-in updater. A 0.2.0 installation will not offer itself as an update.

## Signing key

The public key is embedded from `crates/pos_desktop/assets/update-public.key`.
The private key is outside the repository at `%LOCALAPPDATA%/KAY POS Release Keys/release.key`, with restricted directory ACLs. Back it up securely offline. Never upload it, include it in a client package, or silently regenerate it: existing clients will reject a different key.

Publishing an executable makes it publicly downloadable. The package must never contain `.env`, `kay-pos-db.json`, pending checkout files, database backups, credentials, or signing keys. This update signature is not a Windows Authenticode certificate; SmartScreen reputation warnings may still appear.
