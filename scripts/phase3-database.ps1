param(
    [string]$ConfigFile = (Join-Path $PSScriptRoot '../kay-pos-db.json'),
    [string]$PgBin = 'C:/Program Files/PostgreSQL/18/bin',
    [switch]$Apply
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$config = Get-Content -LiteralPath $ConfigFile -Raw | ConvertFrom-Json
$oldPassword = $env:PGPASSWORD
$oldOptions = $env:PGOPTIONS
$oldTimeout = $env:PGCONNECT_TIMEOUT
$connection = @('-h', $config.host, '-p', $config.port, '-U', $config.username, '-d', $config.database)
try {
    $env:PGPASSWORD = $config.password
    $env:PGCONNECT_TIMEOUT = '10'
    $env:PGOPTIONS = '-c default_transaction_read_only=on -c statement_timeout=15000'
    & "$PgBin/psql.exe" -X -At -v ON_ERROR_STOP=1 @connection -c "SELECT n,COALESCE(to_regclass('public.'||n)::text,'MISSING') FROM unnest(ARRAY['purchase_orders','purchase_order_items','supplier_payments','rust_purchase_drafts','rust_purchase_requests','rust_purchase_items','rust_purchase_movements']) n"
    if ($LASTEXITCODE) { throw 'Preflight failed' }
    if (!$Apply) { Write-Output 'Read-only check complete. Review migration 006 before applying with -Apply.'; return }
    $backupDir = Join-Path $root 'database-backups'
    New-Item -ItemType Directory -Path $backupDir -Force | Out-Null
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    & icacls.exe $backupDir /inheritance:r /grant:r "${identity}:(OI)(CI)F" 'SYSTEM:(OI)(CI)F' | Out-Null
    if ($LASTEXITCODE) { throw 'Could not secure backup directory' }
    $backup = Join-Path $backupDir ("before-phase3-{0}.backup" -f (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
    & "$PgBin/pg_dump.exe" @connection --format=custom --file=$backup --lock-wait-timeout=15000
    if ($LASTEXITCODE) { throw 'Backup failed; migration not applied' }
    & "$PgBin/pg_restore.exe" --list $backup | Out-Null
    if ($LASTEXITCODE) { throw 'Backup archive validation failed; migration not applied' }
    Write-Output "Backup: $backup"
    Write-Output ("SHA256: " + (Get-FileHash -LiteralPath $backup -Algorithm SHA256).Hash)
    $env:PGOPTIONS = '-c lock_timeout=5000 -c statement_timeout=30000'
    $migration = Get-Content -LiteralPath (Join-Path $root 'migrations/006_purchases.sql') -Raw
    "BEGIN; SET LOCAL search_path=public; SELECT pg_advisory_xact_lock(721006);`n$migration`nCOMMIT;" | & "$PgBin/psql.exe" -X -v ON_ERROR_STOP=1 @connection
    if ($LASTEXITCODE) { throw 'Migration failed and was rolled back' }
    Write-Output 'Purchase support tables are available. Verify grants for the actual client database role before deployment.'
} finally {
    $env:PGPASSWORD=$oldPassword
    $env:PGOPTIONS=$oldOptions
    $env:PGCONNECT_TIMEOUT=$oldTimeout
}
