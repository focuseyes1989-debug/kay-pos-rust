# Local appearance

Theme, Follow system theme, window resolution preference and all eight shop
color preferences are stored per Windows user on each client, at:

`%LOCALAPPDATA%/KAY POS Rust/appearance.json`

On the first successful settings load, the existing server appearance is copied
once into this file. Subsequent database refreshes cannot change that local
appearance, including keys that were absent during the first import. Saving
Appearance writes only this local file; it does not update PostgreSQL settings.
The login screen also uses the local preferences, including while offline.

Other shop settings remain shared as before. Existing older clients still write
appearance to the database; upgrade all clients for consistent behavior.
An upgraded client with its local snapshot already initialized ignores those
legacy appearance changes. An uninitialized client imports the server's current
appearance on its first successful load.

Updates replace only the executable and retain this local file. Corrupt local
files are not silently overwritten; saving reports an error. Preserve a copy
before repairing a damaged file.
