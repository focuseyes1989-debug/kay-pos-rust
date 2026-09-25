# KAY POS Rust 0.2.5

Windows x64 signed update using the same signing key as 0.2.4.

## Changes
- Add/Edit Product now includes a multiline Description field for all product types.
- Reads and writes Main POS's existing `products.description` TEXT column. Existing descriptions can be edited or cleared; Unicode and line breaks are preserved.
- Description edits leave stock unchanged. Older serialized product records without this field remain readable.
- Responsive description layout matches the existing product editor.

## Install
Clients on 0.2.4 can use the built-in updater: finish sales and resolve pending checkouts, sign out, then Check for Updates on the login screen and follow Download & Install / Restart.

Clients on 0.2.0 through 0.2.3 still require one manual installation because the signing key changed in 0.2.4. Close KAY POS, back up the existing EXE, and replace only `pos_desktop.exe` from this release's ZIP. Keep database configuration and pending-data files. Do not bypass signature verification.

No database migration is required for this change; the existing Main POS `products.description` column is used. Employee/device database prerequisites and known limits from release 0.2.4 still apply. Back up the shared database and pilot on one client first. WebView2 is required. ZIP signing is not Windows Authenticode signing.

## Verification
Core/desktop unit tests, isolated PostgreSQL description round-trip/edit/clear tests, desktop/mobile layout checks, signed-package tamper rejection and staged updater tests. No production data is changed by these tests.
