# Service Order

The Rust client uses the existing KAY POS Lite PostgreSQL `service_orders`
and `service_order_design_prompts` tables. It does not create a separate job database.

## Functions

- Jobs: search name/details/notes/number, status filter, paginated loading,
  five-second refresh, appointment urgency, staff/timestamps, and status history.
- New/edit: received time, job name, details, optional appointment, notes.
  Short job numbers use the existing serial sequence. Cancelled dialog reservations
  can leave gaps; numbers are not reused.
- Start, complete, collect, cancel, and delete cancelled jobs. Collection displays
  job notes for review. Completion adds the existing Lite notification queue entry;
  sending SMS or other messages still requires the existing queue processor.
- Design prompts: title/category/text, ordering, active toggle, search/category
  filter, PNG/JPEG/WebP sample images up to 3.5 MB, copy template or copy with the
  selected job's fields. Select a job before switching to Design Prompts.
- Substitution: `{job_title}`, `{details}`, `{notes}`, `{appointment}`, `{status}`.
  Substituted values are not recursively expanded.

This follows Lite's simple Jobs workflow, not the separate full service-order
billing workflow. Payment/deposit notes do not post cash, change stock, or create
a POS sale. Use the existing checkout for those operations.

## Deployment

Initialize/update the service-order schema using the current KAY POS Lite version
against the intended database before opening the page. This includes the staff
audit columns, history, payments, notifications and design-prompt tables from
`server/service_order_service.py::ensure_service_order_schema`.
Older schema failures are shown, not silently converted into an empty job list.
No automatic production migration or privilege escalation is performed by this page.
Back up production before any schema update. Grant the existing app database role
the same table and sequence access already used by Lite; not once per workstation.

Cashier/manager/admin can manage jobs and prompts; permanent deletion requires
manager/admin. Each mutation rechecks the session against the database. Jobs linked
to payments or checkout cannot be deleted even if cancelled. Status changes use a
row lock and version check; concurrent edits require refresh rather than overwriting
another workstation's changes. A repeated new-job save uses the same reserved ID.

## Verification

`cargo check -p pos_desktop --locked`

`cargo test -p pos_core`

Integration tests require a disposable PostgreSQL database (never production):

```powershell
$env:P1_TEST_DATABASE_URL='postgresql://postgres@127.0.0.1:55439/kay_pos_p1_tests'
cargo test -p pos_core --test service_orders -- --ignored
```

Tests create isolated schemas. They cover lifecycle/audit, simultaneous start,
stale updates, repeated creation, role revocation, deletion safeguards, legacy ready
statuses, and prompt version conflicts.
