param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$cargo = Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe'
$signer = Join-Path $env:USERPROFILE '.cargo/bin/zipsign.exe'
$key = Join-Path $env:LOCALAPPDATA 'KAY POS Release Keys/release.key'
$public = Join-Path $root 'crates/pos_desktop/assets/update-public.key'
if (!(Test-Path -LiteralPath $key)) { throw 'Release signing key is missing. Do not generate a replacement for existing clients.' }
Push-Location $root
try {
    $metadata = & $cargo metadata --format-version 1 --no-deps --locked | ConvertFrom-Json
    if ($LASTEXITCODE) { throw 'Cargo metadata failed' }
    $version = ($metadata.packages | Where-Object name -eq 'pos_desktop').version
    if (!$SkipBuild) {
        & $cargo build -p pos_desktop --release --locked
        if ($LASTEXITCODE) { throw 'Release build failed' }
    }
    $exe = Join-Path $root 'target/release/pos_desktop.exe'
    $process = Start-Process -FilePath $exe -ArgumentList '--update-self-check',$version -PassThru -Wait -WindowStyle Hidden
    if ($process.ExitCode -ne 0) { throw 'Built EXE version does not match Cargo version' }
    $output = Join-Path $root "target/client-updates/v$version"
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    $zip = Join-Path $output "kay-pos-$version-x86_64-pc-windows-msvc.zip"
    if (Test-Path -LiteralPath $zip) { throw 'Package already exists. Increment the version; do not overwrite published releases.' }
    # Explicit allowlist: never archive the project directory or database settings.
    Compress-Archive -LiteralPath $exe -DestinationPath $zip
    & $signer sign zip $zip $key
    if ($LASTEXITCODE) { throw 'Package signing failed' }
    & $signer verify zip $zip $public
    if ($LASTEXITCODE) { throw 'Signature verification failed' }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($zip)
    try {
        if ($archive.Entries.Count -ne 1 -or $archive.Entries[0].FullName -ne 'pos_desktop.exe') { throw 'Unexpected release contents' }
    } finally { $archive.Dispose() }
    Get-FileHash -LiteralPath $zip -Algorithm SHA256
    Write-Output "Signed update package: $zip"
} finally { Pop-Location }
