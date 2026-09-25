-- Apply as the database owner, then grant the existing client role the required
-- SELECT, INSERT, UPDATE table privileges and sequence USAGE. No device is seeded.
CREATE TABLE IF NOT EXISTS zkteco_devices (
    id SERIAL PRIMARY KEY,
    device_no INTEGER UNIQUE NOT NULL,
    name TEXT,
    ip_address TEXT NOT NULL,
    port INTEGER DEFAULT 4370,
    comm_key INTEGER DEFAULT 0,
    serial_no TEXT,
    last_sync_at TIMESTAMP,
    is_active INTEGER DEFAULT 1,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
