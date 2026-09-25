-- Shared Main POS schema. Apply after 004 and Main POS Employee initialization.
CREATE TABLE IF NOT EXISTS zkteco_employee_mappings (
    id SERIAL PRIMARY KEY,
    device_id INTEGER NOT NULL REFERENCES zkteco_devices(id) ON DELETE CASCADE,
    employee_id INTEGER NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    device_user_id TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(device_id,device_user_id), UNIQUE(device_id,employee_id)
);
CREATE TABLE IF NOT EXISTS zkteco_attendance_logs (
    id SERIAL PRIMARY KEY,
    device_id INTEGER NOT NULL REFERENCES zkteco_devices(id) ON DELETE CASCADE,
    device_user_id TEXT NOT NULL,
    employee_id INTEGER REFERENCES employees(id) ON DELETE SET NULL,
    punch_time TIMESTAMP NOT NULL,
    status INTEGER, punch INTEGER, verification_type INTEGER,
    imported_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    is_valid INTEGER DEFAULT 1, validation_note TEXT,
    UNIQUE(device_id,device_user_id,punch_time,punch)
);
CREATE INDEX IF NOT EXISTS idx_zkteco_logs_employee_time ON zkteco_attendance_logs(employee_id,punch_time);
