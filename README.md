# KAY POS Rust

Windows desktop point-of-sale client built with Rust, Dioxus and PostgreSQL.
This repository contains only the Rust client, its assets, migrations and tests.
The original Python application is not required to build the desktop executable.

## Repositories

- Source: https://github.com/focuseyes1989-debug/kay-pos-rust
- Signed Windows releases: https://github.com/focuseyes1989-debug/kay-pos-updates/releases

The release repository remains separate. Publishing source here does not change
the update URL embedded in existing clients.

## Windows prerequisites

- Rust 1.88 or later with the `x86_64-pc-windows-msvc` toolchain.
- Visual Studio Build Tools with the C++ desktop workload and Windows SDK.
- Microsoft Edge WebView2 Runtime to run the application.
- A compatible KAY POS PostgreSQL database to sign in and use the POS.

## Build and run

```powershell
git clone https://github.com/focuseyes1989-debug/kay-pos-rust.git
cd kay-pos-rust
cargo check -p pos_desktop --locked
cargo run -p pos_desktop --locked
cargo build -p pos_desktop --release --locked
```

The release executable is `target/release/pos_desktop.exe`. Release builds do not
open a console window. All desktop UI assets are included in this repository and
embedded in the executable.

Use **Database connection** on the login screen to configure your server locally.
Never commit the resulting `kay-pos-db.json`, passwords, `.env`, or shop data.
This is a client of an existing compatible database, not a server installer.
The migrations extend that schema; they do not create a complete production
database. Review [deployment notes](docs/P1-DEPLOYMENT.md) and take a verified
backup before applying migrations.

## Tests

```powershell
cargo test -p pos_core -p pos_desktop --lib --bins --locked
```

Database integration tests are opt-in and must use a disposable local database,
never the live shop server. See [deployment notes](docs/P1-DEPLOYMENT.md).
Some updater tests are also opt-in and require a signed package or a read-only
GitHub request; normal unit tests do not download or install updates.

## Releases and signing

See [UPDATES.md](docs/UPDATES.md) for packaging, signing, publishing and client
installation. The public verification key is intentionally committed. The
private signing key is not in this repository and is needed only by the release
publisher, not for ordinary builds. Builds from source cannot publish trusted
updates without that key.

Do not upload database configurations, backups, pending sales, receipts, private
keys, or generated EXEs to source control. Signed binary packages belong in the
separate release repository.

## Layout

- `crates/pos_core`: database access, business rules and integration fixtures.
- `crates/pos_desktop`: Dioxus desktop UI, embedded assets and updater.
- `migrations`: incremental PostgreSQL changes and stock preflight checks.
- `scripts`: receipt preview tests and release tooling.
- `docs`: deployment and feature notes.
- `src-tauri`: historical experimental shell; not part of the supported Cargo
  workspace or the Dioxus desktop build above.
