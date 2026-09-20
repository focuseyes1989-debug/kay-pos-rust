# P1 rollout checklist

This change is not a migration of historical stock. Do not deploy the executable
alone to production. The live database has not been modified by this work.

## Server preparation

For short numbers, also apply `migrations/002_short_numbers.sql` as the
application role before deploying this version. If another role creates the
sequences, grant the application role USAGE on both sequences. Invoice numbers
are separate from durable checkout request IDs; existing pending requests remain
valid. Batch reservations and rolled-back transactions can leave numbering gaps.
Never reset the sequences. Existing invoice, batch and SKU values are preserved.
New products with a blank SKU receive P + their padded database ID; manually
entered SKUs are preserved.

1. Take a verified database backup and stop sales on all clients during rollout.
2. Apply `migrations/001_checkout_requests.sql` once using the application's
   PostgreSQL role. If an administrator applies it, grant the application role
   SELECT and INSERT on `rust_checkout_requests`. Keep this table permanently:
   deleting its rows removes duplicate-retry protection.
3. Run `migrations/stock_preflight.sql` read-only. Reconcile mismatched stock in
   Main POS before enabling sales. This client intentionally refuses to guess
   stock allocations or silently repair existing quantities.
4. Verify existing active users have the intended Admin, Manager or Cashier role.
   Authentication uses the existing PBKDF2-SHA256 hashes, not a new password store.
5. Rebuild with `cargo build --release -p pos_desktop`. Deploy the new executable
   and the existing client database configuration; do not copy pending-sale files
   between PCs. Set a shortcut's working directory to the configuration directory.

## Permissions

- Cashier: Sales, receipt viewing/printing, customer selection during checkout.
- Manager: cashier access plus refunds, customers, suppliers, expenses,
  products, inventory, categories, locations and reports.
- Admin: all of the above plus Settings, database configuration and Users.
- Sale records and sale/refund stock movements record the authenticated operator.
- Sale and refund writes revalidate active status, role and password hash against
  the database. A periodic session check signs out revoked/changed accounts.
- Management pages are role-gated in the client. This is **not** protection against
  someone who has extracted the shared PostgreSQL credentials and runs SQL outside
  the app. Use least-privilege database credentials, OS access control and LAN
  firewall rules. A trusted server API/RLS is needed for a hostile-client boundary.

## Checkout recovery

Before the first network write the full checkout is flushed to
`%LOCALAPPDATA%/KAY POS Rust/pending/<database-fingerprint>.json`.
The request uses a random 128-bit ID and a SHA-256 payload fingerprint.
The sale, stock, credit and request ledger are committed in one transaction.
Retrying an identical request retrieves the original sale, including after an
application restart. A different payload with the same ID is rejected.

An unresolved checkout blocks further checkout on that Windows account/database.
Sign in as its original operator and use Retry original sale, or Check and cancel
unsaved sale. Cancellation takes the same server-side advisory lock and leaves a
tombstone, so a delayed Save cannot sell a cancelled request. If it already saved,
the original receipt is recovered instead. Recovery does not automatically print
or open the drawer again; print the recovered receipt explicitly if necessary.

Do not delete a pending file to bypass an error. If it is damaged or the original
operator is unavailable, an administrator must reconcile the invoice on the server
before clearing local recovery state. Keep the PC/user account and recovery files
until every pending checkout has been resolved. Losing local disk contents is not
covered by network retry protection.

## Stock and concurrency

- Non-service items allocate earliest-expiry stock across existing locations,
  matching the Touch allocation approach. Each consumed batch is recorded on a
  separate sale item with its source location/batch/expiry.
- Master, variant and batch quantities must agree. Empty/mismatched batch data
  fails closed; Service/Restaurant items do not deduct or restore stock.
- Refund restores the recorded source and adjusts credit in the same transaction.
- Checkout locks all products in ascending ID order before variants and batches.
  This matches the product-before-variant order in Inventory and Refund.
- A lock timeout fails safely into checkout recovery. Other/older applications
  that use different lock orders can still cause contention; deploy and test all
  writers together. Do not claim arbitrary external writers are deadlock-free.

## Verification

Run ordinary tests with `cargo test --workspace`.
For database tests, explicitly point `P1_TEST_DATABASE_URL` and
`REFUND_TEST_DATABASE_URL` at a disposable local test database, then run
`cargo test --workspace -- --include-ignored`.
Checkout fixtures create isolated `p1_*` schemas; refund fixtures use temp tables.
Never supply the production URL for integration testing.

Before production, test two real client PCs: simultaneous sales of the same
variant, sale versus stock adjustment/refund, unplug/reconnect during Save,
restart with an unresolved sale, Cashier refund denial, deactivated accounts,
and printer/cash drawer/customer display with the actual hardware.
