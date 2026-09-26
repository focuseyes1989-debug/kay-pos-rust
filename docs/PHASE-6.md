# Phase 6: Audit and Remaining Gaps

This is a source-level increment, not a claim of complete Main POS parity or client readiness.
No production migration, executable packaging, Git push or release publication was performed.
Restaurant and network printing remain intentionally excluded.

## Implemented

### Activity Log

- Manager/admin navigation shows existing Main POS and new Rust entries from `user_activity_log`.
- Inclusive From/To dates, literal search over username/action/details, 50-row pagination and
  global header refresh. SQL search parameters are bound; `%` is not treated as a wildcard.
- Read-only UI: no log editing/deletion. User, action, details, timestamp and recorded IP are shown.
- New Rust entries use `rust.*` actions and authenticated usernames. No passwords, connection strings,
  attachment contents or customer phone numbers are copied into these new log details. IP is NULL:
  the shared database connection cannot reliably identify the physical cashier workstation.
- Business writes and their audit entry commit in one transaction. Failure to insert the audit
  entry aborts the write. An idempotent replay of a saved checkout/held/purchase/management request
  does not append a duplicate success entry.
- Coverage: sale completion/full refund, credit creation/collection, stock in/out/adjustment/reversal,
  category save/delete, category group save, purchase save/receive/cancel/payment, discount/budget/
  expense settings/attachment management, expense save/delete, held save/resume, service job
  creation/edit/status/delete. This does not cover every existing Rust or Main POS write path.
- Audit is an application history, not tamper-proof evidence. Existing shared database roles may
  still be able to edit tables directly. No role permissions are silently revoked from Main POS.

### Category Groups

- Categories / Category groups tabs follow the existing receipt tab styling.
- Group create/edit, name, description, display order, color, favorite and active/inactive state.
  Existing Main icon metadata is preserved when editing. No hard delete is exposed.
- Category editor assigns a category to an active group or No group; an existing inactive
  assignment is preserved unless explicitly changed. Favorites/order are stored in Main's fields.
- Normalized duplicate-name rejection and stale-edit checks protect concurrent changes. Reserved
  group IDs make retrying an unchanged group creation safe; sequence gaps after Cancel are normal.
- Category reads/writes and group reads/writes revalidate manager permission on the server.
  System-category and hierarchy protections remain. Category edits reject stale original values.

### Service Orders and Existing Writes

- Customer name/phone can be edited, searched and seen in the job detail. Appointment cannot precede
  received time. Creation retries compare contact values; stale edits fail without overwriting.
- Ready-for-pickup queue uses the stored current customer phone. This queues a notification; it does
  not send SMS or WhatsApp. Existing lifecycle/status history remains and now has central audit entries.
- Stock In now revalidates manager permission and replaces the submitted `received_by` with the
  authenticated operator. Manual credit creation/collection also revalidate manager permission.
- Settings > Database compatibility checks now include Phase 4-6 support tables, group/contact
  columns, audit INSERT permission and audit/group sequence USAGE. Checks remain read-only.

## Deployment Prerequisites

1. Back up and restore-test a separate staging copy of the Main database.
2. Review/apply migration `009_audit_groups.sql` as the schema owner. Existing Main tables/rows are
   retained. Older partially initialized tables require a reviewed Main schema upgrade, not blind
   replacement. Phase 4 and 5 separately require migrations 007 and 008, still not production-applied.
3. Grant the actual client role required table and sequence privileges. Audit requires SELECT/INSERT
   and sequence USAGE; groups require SELECT/INSERT/UPDATE and sequence USAGE. Category writes need
   existing permissions. Do not grant audit UPDATE/DELETE merely to make the viewer work.
4. Run Settings > Database > Check compatibility using each distinct client role.
5. Test native app on staging and multiple PCs, including network interruption and revoked accounts.

## Remaining Gaps / Follow-Up Register

| Area | Remaining work |
| --- | --- |
| Service Orders | Full estimates/items, deposits/refunds, checkout hand-off, attachments and actual notification delivery need their own reviewed transactional workflow. This increment does not invent financial values or send messages. |
| Service schema variants | Rust currently targets the Main Lite `order_no`/timestamp/status-history schema. Main's older `job_no`/text-date schema is different. Preflight must reject incompatibility; no automatic conversion is performed. |
| Audit breadth | Login/logout, employee, product, supplier, customer-master/settings and some legacy read/write paths still need comprehensive instrumentation/permission review. No retrospective activity is fabricated. |
| Legacy retries | Direct expense/category/credit mutation paths do not yet have the durable command journals used by checkout/purchases. Do not blindly retry after an uncertain connection failure; reconcile first. |
| Category presentation | Sales group navigation and group-level reporting parity, icon selection/import/export and bulk reassignment remain separate UI work. Group assignments are stored in the shared schema now. |
| Phase 4 | Final deployment review, automatic expense-alert scheduling and Main POS opening database-backed attachments remain pending. |
| Phase 5 | Points redemption/expiry jobs and Main/Rust simultaneous hold-resume interoperability remain as documented in PHASE-5.md. |
| Reports | Historical balances, partial-refund financial mapping and unknown historical costs require reconciliation/design decisions, not guessed totals. |
| Client rollout | Migrations 007/008/009, restore rehearsal, real native workflow tests and release signing/update verification precede a new pilot release. |

## Tests

`phase6` PostgreSQL tests exercise group assignment, duplicates, stale writes, revoked roles,
stock actor attribution, literal audit search, idempotent checkout logging and forced audit-failure
rollback of sales and credit collections. Service tests cover contact edits, stale versions,
notification queue recipient and audit count. Existing checkout, purchase, expense, loyalty and
service tests are rerun. Tests use only the isolated loopback test cluster.

`scripts/test-phase6-layout.cjs` uses real CSS and representative markup, light/dark at
1366/768/390 widths. It checks title visibility, table scrolling, pagination and editor bounds.
It is not native-app end-to-end verification. Native minimum window size remains 1280x720.
