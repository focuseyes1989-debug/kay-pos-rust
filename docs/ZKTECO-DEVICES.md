# ZKTeco Device Settings

Settings > ZKTeco Device reads and edits Main POS's shared `zkteco_devices` table.
The page supports new devices, selecting existing records for editing, active
status, IP/port/key validation and masked Comm Key input. Header Refresh reloads
the list without replacing unsaved form values. Server-side Admin authorization
is rechecked for reads, saves and connection probes.

Initialize this table in Main POS, or apply `migrations/004_zkteco_devices.sql`
as database owner and grant the existing client role SELECT/INSERT/UPDATE and
sequence USAGE. No production migration is automatically performed.

Test TCP Connection only establishes a TCP socket with a five-second timeout.
It does not validate the ZKTeco protocol or Comm Key.

Attendance Sync is available in this settings page and the Employees > Attendance
tab. It authenticates using the saved Comm Key and reads the selected active device
using the pinned MIT-licensed [rustzk connector](https://github.com/vkaylee/rustzk).
No Python installation or separate HTTP server is required. The importer uses
TCP first and falls back to UDP if the TCP connection/handshake fails. Explicit
authentication rejection stops immediately and reports a Comm Key error. A device
appearing in the list indicates a saved database record, not live connectivity.
The TCP-only settings probe is not evidence that UDP sync is unavailable.
The importer uses
device-local wall-clock timestamps, matching Main POS rather than converting dates
using the client PC's timezone.

Apply `migrations/005_zkteco_attendance.sql` after Main POS Employee initialization
and migration 004 if the shared mapping/log tables do not exist. Grant the existing
client role the table/sequence privileges needed for importing logs and updating
attendance/device metadata. Production migrations are not automatic.

Configure employee-to-device-user mappings in Main POS first. The Rust sync dialog
shows the active mapping count and rejects unmapped devices. It never invents
employee accounts or guesses device user IDs. Access requires the same named
permissions as Main POS sync (settings, employees, manage_attendance,
manage_employees), with the existing Admin bypass.

Each device import is atomic, checks that mappings/configuration did not change
while reading, and deduplicates raw logs using Main POS's unique key. Repeating a
sync is safe after an interrupted response. Manually corrected attendance rows
remain unchanged. Timestamps more than one day beyond device time are stored as
invalid and excluded from attendance, matching Main POS. Unmapped logs are skipped
and counted. Raw device logs are never cleared and device time/users are not changed.
Serial and Last Sync update only after a successful import transaction.

Attendance date presets are Today, This week (Monday through today), and This month
(first day through today). They update From/To and immediately apply the range.

No live device was contacted during development. Tests use isolated PostgreSQL,
synthetic device snapshots and an ephemeral loopback TCP listener, including
permission rejection, duplicate IDs, concurrent import retries, future timestamps,
unmapped punches, manual-correction preservation and stale-mapping rejection.
Hardware compatibility still requires an acceptance run on the actual K20.
