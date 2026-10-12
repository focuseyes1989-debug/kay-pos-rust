# KAY POS 0.2.25

- Fix Credit checkout initialization so opening or reopening checkout keeps Received at zero instead of automatically recording the full sale total as paid. This also applies after customer and settings loading and when restoring a held Credit sale.
- Show the outstanding Credit balance in checkout so the amount owed is visible before saving. Partial payments can still be entered explicitly.
- Preserve existing customer, receipt, appearance and database configuration.

Finish sales and resolve pending checkout requests, then sign out and use Check for Updates on the login screen to download and install. Restart KAY POS after installation.

This signed Windows x64 package contains only pos_desktop.exe. No database migration or automatic historical balance changes are included. Existing incorrectly paid Credit invoices require a reviewed correction. Microsoft Edge WebView2 Runtime is required.

Validation: desktop unit tests, PostgreSQL credit checkout/retry/refund integration test, signed package verification and executable version self-check.
