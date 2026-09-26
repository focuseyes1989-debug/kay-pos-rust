param(
    [string]$ConfigFile = (Join-Path $PSScriptRoot '../kay-pos-db.json'),
    [string]$PgBin = 'C:/Program Files/PostgreSQL/18/bin',
    [switch]$ApplyEmployeeLedger
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$config = Get-Content -LiteralPath $ConfigFile -Raw | ConvertFrom-Json
$savedPassword = $env:PGPASSWORD
$savedOptions = $env:PGOPTIONS
$savedTimeout = $env:PGCONNECT_TIMEOUT
$connection = @('-h', $config.host, '-p', $config.port, '-U', $config.username, '-d', $config.database)
try {
    $env:PGPASSWORD = $config.password
    $env:PGCONNECT_TIMEOUT = '10'
    $env:PGOPTIONS = '-c default_transaction_read_only=on -c statement_timeout=15000'
    $exists = & "$PgBin/psql.exe" -X -At -v ON_ERROR_STOP=1 @connection -c "SELECT to_regclass('public.rust_employee_requests') IS NOT NULL"
    if ($LASTEXITCODE) { throw 'Database preflight failed' }
    if ($exists -eq 't') { Write-Output 'Employee request ledger already exists. No schema changes made.'; return }
    if (!$ApplyEmployeeLedger) { Write-Output 'ACTION: rust_employee_requests is missing. Run with -ApplyEmployeeLedger after reviewing migration 003.'; return }

    $backupDir = Join-Path $root 'database-backups'
    New-Item -ItemType Directory -Path $backupDir -Force | Out-Null
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    & icacls.exe $backupDir /inheritance:r /grant:r "${identity}:(OI)(CI)F" 'SYSTEM:(OI)(CI)F' | Out-Null
    if ($LASTEXITCODE) { throw 'Could not restrict backup directory permissions' }
    $backup = Join-Path $backupDir ("before-phase1-{0}.backup" -f (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
    & "$PgBin/pg_dump.exe" @connection --format=custom --file=$backup --lock-wait-timeout=15000
    if ($LASTEXITCODE) { throw 'Backup failed. Migration was not applied.' }
    & "$PgBin/pg_restore.exe" --list $backup | Out-Null
    if ($LASTEXITCODE) { throw 'Backup archive validation failed. Migration was not applied.' }
    Write-Output "Backup: $backup"
    Write-Output ("SHA256: " + (Get-FileHash -LiteralPath $backup -Algorithm SHA256).Hash)

    $env:PGOPTIONS = '-c lock_timeout=5000 -c statement_timeout=30000'
    $migration = Get-Content -LiteralPath (Join-Path $root 'migrations/003_employee_requests.sql') -Raw
    $sql = "BEGIN; SET LOCAL search_path=public; SELECT pg_advisory_xact_lock(721003);`n$migration`nCOMMIT;"
    $sql | & "$PgBin/psql.exe" -X -v ON_ERROR_STOP=1 @connection
    if ($LASTEXITCODE) { throw 'Migration failed; transaction rolled back. Keep the backup and inspect the error.' }
    Write-Output 'Employee ledger created. Existing business records were not modified. Run the compatibility checker with each client database role.'
} finally {
    $env:PGPASSWORD = $savedPassword
    $env:PGOPTIONS = $savedOptions
    $env:PGCONNECT_TIMEOUT = $savedTimeout
}
