# Database modes

KAY POS Rust supports two independent database modes. No automatic sync, automatic migration
of shop records, or fallback from an unavailable server to a local file occurs.

## Choose a mode

Open **Database connection** on the login screen, or **Settings > Database** as an
administrator. Select **SQLite** or **PostgreSQL**, then **Save & Test**.

- **SQLite / Standalone:** choose a local `.db` file. The default is
  `%LOCALAPPDATA%/KAY POS Rust/standalone/pos.db`. A new file starts with an empty
  shop, Cash/Credit payment methods and a Shop location. Enter an initial admin
  username and a password of at least eight characters. Existing files retain
  their accounts; setup cannot replace their passwords. SQLite is bundled in the
  executable and needs no PostgreSQL service or internet connection.
- **PostgreSQL / Server:** enter the existing host, port, database, username and
  password. Existing `kay-pos-db.json` files default to this mode, retaining the
  same connection and pending-request identities. Server SQL and typed bindings
  continue through the PostgreSQL driver unchanged.

Changing mode signs you out. Sign in with an account belonging to the selected
database. Finish the current cart and resolve pending checkout, purchase,
employee or held-sale requests first. Recovery journals are scoped to each
server database or standalone file so records cannot be replayed into another
shop. Switching modes does not move products, customers, debts or other records.

Use SQLite for one computer with a local file. Do not place the database on a
network share or cloud-sync folder. Use PostgreSQL for shared shop terminals.
Keep the standalone file protected with normal Windows account/file permissions.

## Back up and restore standalone data

From **Settings > Database**, choose **Back up standalone database** and a new
backup filename. The online backup includes committed WAL transactions. Do not
copy only the `.db` file while the app is running; pending writes may be in its
`-wal` file. Backups contain shop records and account hashes; keep them private.

To restore, close the active shop session and choose the backup `.db` file in
**Database connection**. Sign in with its existing account. Keep the original
file until the restored records are verified. A backup is a snapshot; changes
made after it are not included. PostgreSQL continues to use your server backup
workflow.

## Manual PostgreSQL export and standalone import

Sign in as an administrator and open **Settings > Database > Import / Export shop data**.

1. While connected to PostgreSQL, choose **Export shop data** and a new `.db` filename.
   The server is read with a single repeatable-read snapshot. Tables, account hashes,
   images/attachments, IDs and invoice/batch sequence high-water marks are copied into
   a complete SQLite standalone file. The server is not modified.
2. On the destination computer, choose **Import to new standalone file**. Select the
   export, then a NEW local destination filename. Existing databases are never
   overwritten or merged. An existing standalone backup can also be imported.
3. Open **DB Settings**, select **SQLite**, choose the imported file and **Save & Test**.
   Sign in with the imported account and its existing password. Your current database
   selection stays unchanged until you explicitly select the imported file.

The export itself is also a usable standalone database. Import makes a validated
independent copy, which lets you keep the original export as a backup. An export
contains shop records and account hashes; keep it private. SQLite mode can use the
same Export button, and the existing online Backup button remains available.

Complete or return all active held-sale claims before exporting. Local pending
request journals are not transferred. Exports are snapshots of committed data;
server changes after export are not copied later. Source and destination are
independent shops after the transfer. There is no background connection or sync.

Copies are written to a temporary file, checked, and made visible at the selected
filename only on success. Checks include supported schema, active admin account,
row counts, SQLite integrity and foreign-key references. Unknown server tables or
columns, broken references, active held claims and future SQLite schema versions
cancel the transfer instead of silently dropping data. If schema differs from the
supported POS schema, update/support that schema first and export again.

## Implementation and verification

SQLite writes use `BEGIN IMMEDIATE`, foreign keys, WAL, FULL synchronous mode and
a bounded busy timeout. PostgreSQL queries are passed through; only SQLite
queries use the parsed dialect adapter. Parameters stay bound, including repeated
and out-of-order placeholders. JSON, dates, batch identities and sequences have
SQLite equivalents. Unknown schemas and unsupported future schema versions are
rejected. Schema initialization contains structure/defaults only, never shop data.

Run SQLite integration scenarios without a database server:

```powershell
cargo test -p pos_core --test standalone --locked
cargo test -p pos_core --lib sqlite_dialect --locked
python scripts/generate-sqlite-query-corpus.py --check
cargo test -p pos_desktop --bin pos_desktop --locked
```

The SQL corpus prepares 477 static POS statements; functional tests additionally
exercise dynamically generated reports, employees, held carts and stock batches.
They cover PostgreSQL-to-SQLite transfer, imported account login, binary data,
invoice sequence continuity, overwrite refusal, failure cleanup, and credit collection/refunds, purchase receipt/payment retries, expenses
and attachments, payroll, loyalty, variants, transfers, rollback, read-only
transactions, foreign keys, file isolation, backups and fractional allocations.

PostgreSQL regressions must run on a disposable local PostgreSQL cluster on
`127.0.0.1:55487`, never the shop server. Set `P1_TEST_DATABASE_URL`,
`EMPLOYEE_TEST_DATABASE_URL`, `REFUND_TEST_DATABASE_URL` and
`CATALOG_TEST_DATABASE_URL` to that cluster. Set `KAY_TRANSFER_TEST_CONFIG` to a
JSON fixture with host, **numeric** port 55487, database, username and password.
Then run `cargo test -p pos_core --locked -- --include-ignored`. Database tests
use separate schemas or temporary tables. Hardware printers/drawers and physical
attendance devices require their normal device checks separately.
