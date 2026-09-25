# KAY POS Rust 0.2.7

## Changes
- Embedded project SVG icons in navigation and common actions, including product/customer/inventory editing, printing, exports, login, updates and dialogs.
- Icons inherit the control's theme color; no separate icons folder is required on client PCs.
- Sidebar page labels use regular 14px text. Group-heading styles no longer leak into nested icon labels.
- Add buttons with an add icon no longer repeat a textual plus sign.

## Install
Uses the same signing key as 0.2.6. Finish sales and resolve pending checkouts, sign out, then Check for Updates on the login screen and follow Download & Install / Restart.

Clients on 0.2.4 or 0.2.5 require one manual installation because their signing key differs. For any older client, the supported migration is to close the app, back up the EXE and replace only pos_desktop.exe from this official ZIP. Keep database configuration and pending files. Do not bypass signature checks.

No database migrations or balance corrections are included. Existing 0.2.6 reporting scope and database prerequisites still apply. GitHub rate limits can temporarily block update checks; wait for the stated reset time. WebView2 is required; ZIP signing is not Windows Authenticode signing. Pilot on one PC first.

## Verification
Desktop unit tests, SVG layout checks, dashboard scrolling, page-refresh and keyboard shortcut checks; executable version self-check, signed package verification, tamper rejection and staged updater tests.
