param(
    [string]$ConfigFile = (Join-Path $PSScriptRoot '../kay-pos-db.json'),
    [string]$PgBin = 'C:/Program Files/PostgreSQL/18/bin',
    [switch]$Apply
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
    $exists = & "$PgBin/psql.exe" -X -At -v ON_ERROR_STOP=1 @connection -c "SELECT to_regclass('public.rust_erp_outbox') IS NOT NULL AND EXISTS(SELECT 1 FROM pg_trigger WHERE tgname='trg_rust_erp_stock_movement' AND NOT tgisinternal)"
    if ($LASTEXITCODE) { throw 'ERP migration preflight failed' }
    if ($exists -eq 't') { Write-Output 'ERP outbox and inventory trigger already exist. No schema changes made.'; return }
    if (!$Apply) { Write-Output 'ACTION: ERP outbox is missing. Review migration 010, then rerun with -Apply.'; return }

    $backupDir = Join-Path $root 'database-backups'
    New-Item -ItemType Directory -Path $backupDir -Force | Out-Null
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    & icacls.exe $backupDir /inheritance:r /grant:r "${identity}:(OI)(CI)F" 'SYSTEM:(OI)(CI)F' | Out-Null
    if ($LASTEXITCODE) { throw 'Could not secure backup directory' }
    $backup = Join-Path $backupDir ("before-erp-phase1-{0}.backup" -f (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
    & "$PgBin/pg_dump.exe" @connection --format=custom --file=$backup --lock-wait-timeout=15000
    if ($LASTEXITCODE) { throw 'Backup failed; migration was not applied' }
    & "$PgBin/pg_restore.exe" --list $backup | Out-Null
    if ($LASTEXITCODE) { throw 'Backup validation failed; migration was not applied' }
    Write-Output "Backup: $backup"
    Write-Output ("SHA256: " + (Get-FileHash -LiteralPath $backup -Algorithm SHA256).Hash)

    $env:PGOPTIONS = '-c lock_timeout=5000 -c statement_timeout=30000'
    $migration = Get-Content -LiteralPath (Join-Path $root 'migrations/010_erp_outbox.sql') -Raw
    "SET search_path=public; SELECT pg_advisory_lock(721010);`n$migration`nSELECT pg_advisory_unlock(721010);" |
        & "$PgBin/psql.exe" -X -v ON_ERROR_STOP=1 @connection
    if ($LASTEXITCODE) { throw 'ERP migration failed. Keep the backup and inspect the error.' }
    Write-Output 'ERP Phase 1 outbox installed. Configure the webhook only after its receiver is ready.'
} finally {
    $env:PGPASSWORD = $savedPassword
    $env:PGOPTIONS = $savedOptions
    $env:PGCONNECT_TIMEOUT = $savedTimeout
}
