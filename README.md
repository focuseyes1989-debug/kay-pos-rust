# KAY POS Rust

Windows desktop point-of-sale client built with Rust, Dioxus, SQLite and PostgreSQL.
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
- For server mode: a compatible KAY POS PostgreSQL database.
- Standalone mode bundles SQLite in the EXE; no database server installation is needed.

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

Use **Database connection** on the login screen to select **SQLite** (standalone)
or **PostgreSQL** (server). For a new SQLite file, enter an admin username and a
password of at least eight characters, then **Save & Test**. Existing PostgreSQL
configurations continue to use server mode automatically.

Modes have separate records and accounts. There is no automatic sync or import.
See [database modes and backups](docs/DATABASE-MODES.md).
Never commit `kay-pos-db.json`, passwords, `.env`, SQLite files or shop data.
PostgreSQL mode connects to an existing compatible schema; its migrations extend
that schema. SQLite mode initializes its own complete empty standalone schema.
Review [deployment notes](docs/P1-DEPLOYMENT.md) before applying server migrations.

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
- `migrations`: PostgreSQL extensions, stock preflight checks and standalone SQLite schema.
- `scripts`: receipt preview tests and release tooling.
- `docs`: deployment and feature notes.
- `src-tauri`: historical experimental shell; not part of the supported Cargo
  workspace or the Dioxus desktop build above.
