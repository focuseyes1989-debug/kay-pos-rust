# Serial numbers and warranty

For new sales, click the inventory icon beside the cart quantity controls to open
**Serial & Warranty**. Enter one serial per sold unit and a warranty term in months,
then Apply. Choose a customer at checkout. The sale, stock deduction, credit entry
and serial registrations commit together; any serial conflict rolls everything back.
The receipt shows the serials, warranty months and start/end dates in previews and
58/80 mm print layouts, including later reprints. Held carts and durable checkout
recovery retain serials. After changing quantity, update the serial list to match.

Ordinary items may still be sold without serials. Use **Sales > Serial & Warranty**
for historical receipt registration and claims.
Find the completed receipt by its exact invoice number, select its product line,
enter the physical serial number and warranty length (1–120 months), then register.
Repeat once per sold unit. Existing ordinary sales continue without serial registration.
Registration is also available after checkout, including historical completed receipts.
This module tracks sold units; it does not reserve serial numbers when receiving inventory.

Serials are trimmed and normalized to uppercase, unique across this database.
The receipt line must have a whole-unit quantity. Registration cannot exceed its
unrefunded quantity. Anonymous and refunded receipts cannot register warranties.
The warranty starts on the original sale date. Its end date is inclusive; month
addition clamps to the last day of the destination month (Jan 31 + 1 month = Feb 28/29).

Search by serial, invoice, customer name or product. Select a unit to see its claims.
Recording a fault checks coverage on the claim date and the current receipt status.
Out-of-period or refunded-sale claims are saved as rejected with the reason in history.
Only one active claim per serial is allowed. Cashiers may register units and record
claims; managers/admins may process, resolve or reject them with a required note.
Closed claims cannot be reopened; a later issue can be recorded as another claim.
Coverage at intake is retained in history. Refunding an active claim's receipt prevents
further processing/resolution; reject that claim with a note instead.

Each status entry records the user and time. Claims do not change stock, payments,
customer balance, or create a replacement sale. Process exchanges/replacement sales
using the normal authorized sales/refund flow; automatic replacement stock handling
is not included.

## Storage and compatibility

SQLite adds three optional extension tables transactionally when opening the file.
PostgreSQL creates the same extension tables and indexes on first module use
(`migrations/warranty.sql`); the connection needs schema creation permission for
that first use. The migration only adds tables/indexes and references existing
receipt, receipt-item, and customer IDs. Existing sales and product rows are untouched.
Subsequent use does not repeat DDL or require creation permission.

Manual PostgreSQL/SQLite export/import includes units, claims and their histories.
Older standalone backups without these extension tables remain importable; opening
them adds empty warranty tables. A partially missing warranty schema is rejected.
These extension tables retain schema version 1 because they are additive and optional.
Older app releases cannot export the new PostgreSQL tables; use the new app for transfers.
No automatic sync is introduced.

## Validation

Integration tests cover serial uniqueness/case normalization, concurrent registration,
quantity limits, anonymous/fractional/refunded receipts, month boundaries, expiry,
duplicate active claims, status history, account/role changes, exports/imports and
legacy backups. PostgreSQL validation runs on the isolated port 55487 test cluster.

## A4 warranty forms

After registering a serial, its unit is selected automatically. Use **Export PDF**
or **Print A4** in the A4 Warranty Form section. Existing serials can also be selected
from lookup. Each form groups every product and registered serial from the selected unit's original receipt, shows customer details once, and includes the original
invoice, customer contact details, product, serial, warranty term/dates, and signature
and shop-stamp spaces. Shop name, contact details and logo come from Receipt settings.
Refunded/non-completed receipts carry a visible warning. The form records the
warranty; it does not introduce new coverage terms or change any shop records.

PDF export uses Windows **Microsoft Print to PDF** and a save dialog. A private
temporary file is checked before replacing the destination. A4 printing opens the
Windows printer chooser and enforces portrait A4. Long content flows to additional
A4 pages rather than dropping text. Myanmar text uses the Windows Myanmar Text font.
