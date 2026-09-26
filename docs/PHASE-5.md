# Phase 5: Held Sales and Loyalty

Source implementation only. No application executable, release, or deployment was produced.
Phase 4 source changes are preserved; its final verification/deployment remains separate.

## Database prerequisites

Keep the existing Main POS `held_sales`, `customers`, `customer_points_log` and `settings` tables.
After a verified database backup, the database administrator must apply
`migrations/008_held_loyalty.sql` and grant the client role access to its three tables.
The script adds request/recovery/award tracking; it does not rewrite existing balances or sales.
It is safe to re-run. It has only been applied to the isolated test database, not the shop database.
Phase 4 management features separately require `migrations/007_discounts_expenses.sql`.

## Held sales

- Sales toolbar: Hold sale (with note), Held sales, Resume sale.
- A hold stores the cart, customer, payment type and note. It does not reserve/deduct stock,
  create a receipt, collect money or award points.
- Resume requires an empty cart. It rechecks products/variants and total requested stock,
  uses current catalogue/promotion/wholesale prices, and preserves quoted service prices.
  Final checkout still validates stock transactionally. Credit resumes start with received money at zero.
- Claiming moves the shared row into a durable Rust session. Concurrent Rust claims cannot both win.
  Checkout closes that session in the same transaction as the sale. Request retries are idempotent.
- Re-holding a resumed cart returns it to the shared held list. Clear/removal of its last line is
  disabled to avoid abandoning the recovery session; re-hold or complete the sale instead.
- A per-database journal in `%LOCALAPPDATA%/KAY POS Rust/held-carts` preserves request IDs and
  the active token. On restart, sign in as the original operator and recover or return the saved hold.
  Cancel checks the server and fences an uncommitted request before clearing the local journal.
  A committed request must be retried/recovered, not cancelled locally.
- Recovery restores the last saved held snapshot. Edits after resuming are not crash-durable until
  re-held or submitted through checkout. Do not delete the recovery files to dismiss an error.
- Restaurant, Main POS explicit batch/location, and expiry-clearance holds must be resumed in Main POS.
- Main POS uses a separate read-then-delete resume implementation. Its race with a Rust claim cannot
  be fully prevented by Rust row locking alone. Do not simultaneously resume the same shared hold
  from Main and Rust; use one application for held-sale operations during the pilot.

## Customer points

- Customers detail: Points history displays the authoritative balance, dates, raw movement types,
  points, receipt/reference and expiry; searchable and paginated. Manager permission is required.
- Customer non-credit sales earn `floor(net sale total * loyalty_points_per_dollar)`.
  Uses Main POS settings; missing/zero rate disables earning. Expiry uses `points_expiry_months * 30`
  days, default 12 months. Credit and anonymous sales do not earn points.
- Earning and history insert occur inside the checkout transaction. A failure rolls everything back.
- Rust full-refund reverses the recorded award, not the current earning rate. Refund history stores
  a negative movement. A refund can make points negative if previously awarded points were spent.
- No inferred repairs to historical sales/points. Only awards recorded by this implementation are
  reversed by it. Refund new Rust-awarded receipts in Rust to use this exact-award reversal.
- Points redemption, automatic expiry processing, reward catalogue and editing loyalty settings in
  Rust are not part of this step. Existing Main POS loyalty settings remain authoritative.

## Verification

- Core transaction tests cover concurrent hold saves/resumes, retries, cancellation fencing,
  re-hold recovery, checkout failure rollback, earning, credit exclusion, permissions and refund.
- Desktop unit tests cover current price/aggregate stock validation, incompatible/missing variants,
  service quotations and atomic journal replacement.
- `scripts/test-held-loyalty-layout.cjs` checks the real CSS at 1366/768/390 widths in light/dark.
  Smaller widths are a modal/layout stress test; the existing native application minimum is 1280x720.
  This is a static layout harness, not a live native UI or Main POS interoperability test.
- Before client deployment, run a backed-up staging database/native-app pilot with actual Main POS
  held rows, cash/credit receipts, refund, restart recovery and multiple Rust PCs.
