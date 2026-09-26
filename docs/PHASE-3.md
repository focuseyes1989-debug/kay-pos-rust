# Phase 3: purchases

## Deployment state

Source implementation only. No application EXE build, package, release, or client
installation is performed for this phase. Migration 006 is tested on an isolated
database, not automatically applied to the production database.

Before deployment, review `migrations/006_purchases.sql`, run the read-only
`scripts/phase3-database.ps1` preflight, then apply with `-Apply` during a controlled
deployment. The script requires a successful full custom-format backup and archive
listing before applying this additive migration in a transaction. An archive listing
is not a full restore rehearsal. Securely rehearse recovery separately.

The client role needs existing stock/order/ledger privileges plus SELECT, INSERT,
UPDATE on rust_purchase_drafts; SELECT, INSERT on rust_purchase_requests,
rust_purchase_items, rust_purchase_movements; and sequence USAGE on
rust_purchase_drafts_id_seq. Other Main sequences must retain their normal insert
permissions. The script does not automatically grant broad privileges or change
existing financial data. Missing migration/permissions fail closed.

## Workflow

- Purchases navigation requires manager/admin. Every core read and write reauthorizes
  the active account. Excel export reauthorizes before writing the file.
- Purchase Orders: create, edit, cancel a pending order. Supplier is fixed after
  creation; to change it, cancel and create another order. Revision checks reject
  stale edits. Cancelled drafts are retained, not deleted.
- Drafts use rust_purchase_drafts and do not create Main purchase_orders, stock
  movements or supplier debt. This avoids Main's legacy ledger fallback treating
  unreceived draft orders as purchases.
- Receive entire order creates Main purchase_orders (completed), its items,
  stock/batch/location updates and one supplier_payments Purchase debit in one
  transaction. Main history sees the purchase after receipt. Main order_date is
  receipt date; original order date is retained in the draft and notes.
- Products use positive integer BASE units, Each or Variants. An active variant
  belonging to that product and a registered location are required. Service,
  Restaurant and fractional-weight receipt entry are not supported here.
- Discount is an amount and tax a percentage. Stock acquisition cost allocates the
  final bill across line values using cumulative decimal rounding. Saved PO item
  prices remain pre-discount/tax, matching Main's header adjustment model. Parent
  stock costs use the existing weighted-average receiving implementation; variant
  costs use that implementation's latest receipt cost.
- Purchase History shows existing Main and new Rust receipts, date/supplier/status
  filters, item details, linked paid/remaining amounts and Excel export of every
  filtered row (not just the displayed 25-row page). Old variant/location metadata
  is shown as unrecorded, never guessed.
- Supplier payment supports a completed order or an explicit unallocated payment /
  advance. Linked payments require a matching Purchase debit, cannot exceed the
  remaining order balance and update Paid/Partial atomically. Unallocated advances
  reduce the supplier ledger but are NOT silently allocated to individual orders.
  No expense entry is duplicated automatically. Existing ambiguous legacy debts
  must be reconciled in Main before linked payment; this feature does not repair them.
- Supplier Ledger shows date-filtered entries and a separately labelled current
  balance. Negative current balance represents an advance.

## Safety

Every mutation has a unique request ID and exact-payload fingerprint. A transaction
advisory lock serializes retries; supplier, draft/order and sorted product locks
protect concurrent Rust operators. The request result commits with its stock and
money effects. Reusing a request with changed content/operator is rejected.

A durable per-database local journal under LocalAppData/KAY POS Rust/purchase-pending
is saved before sending. An uncertain result blocks new purchase actions. Retry
uses the same command. Check and cancel unsaved either recovers a completed result
or commits a tombstone under the same lock, fencing a delayed original request.
Never delete journals manually. The original operator must resolve the request.

Received purchase movements cannot use Rust's stock-only Reverse action: that would
leave supplier debt unchanged. Existing Main or manual database edits are outside
this guard. Pilot concurrent Main/Rust workflows before rollout; old Main writers
may not follow the same locking protocol.

## Deliberate limits

Initial support is full receipt only. Partial deliveries, supplier returns,
receipt cancellation, attachment uploads, automatic advance allocation and editing
already received orders are not implemented. Drafts are not visible in Main's PO
history until received. Restaurant and network printing remain excluded.

## Verification

Isolated PostgreSQL tests cover no stock/debt for drafts, atomic receipt with
Each/Variant lines, duplicate/concurrent retry, revision conflicts, payment
overpayment races, request cancellation tombstones, purchase-linked reversal guard
and a forced ledger failure rolling back all stock/PO writes. Existing checkout and
inventory tests exercise the extracted receiving helper. Local-journal tests cover
exact-payload persistence. Compile checking does not build the application EXE.

`scripts/test-purchases-layout.cjs` is a representative markup/CSS/combo harness
for light/dark desktop/tablet/mobile layouts, one combo per select and dialog scroll
reachability. It is not a logged-in native-app E2E test.
