param([Parameter(Mandatory=$true)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version, [string]$NotesFile, [switch]$Publish)
$ErrorActionPreference = 'Stop'
if ($Version -ne '0.2.0' -and !$NotesFile) { throw 'Provide release notes for this version using -NotesFile.' }
if ($NotesFile) { $customNotes = Get-Content -LiteralPath $NotesFile -Raw }
$repo = 'focuseyes1989-debug/kay-pos-updates'
$root = Split-Path $PSScriptRoot -Parent
$name = "kay-pos-$Version-x86_64-pc-windows-msvc.zip"
$zip = Join-Path $root "target/client-updates/v$Version/$name"
& "$env:USERPROFILE/.cargo/bin/zipsign.exe" verify zip $zip (Join-Path $root 'crates/pos_desktop/assets/update-public.key')
if ($LASTEXITCODE) { throw 'Invalid package signature' }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive=[IO.Compression.ZipFile]::OpenRead($zip)
try {
    if ($archive.Entries.Count -ne 1 -or $archive.Entries[0].FullName -ne 'pos_desktop.exe') { throw 'Package contains unexpected files' }
} finally { $archive.Dispose() }
# Resolve a publisher credential only in this process. Never include it in the package.
$env:GIT_TERMINAL_PROMPT='0'
$env:GCM_INTERACTIVE='never'
$credential=@{}
$lines="protocol=https`nhost=github.com`n`n" | git credential fill
foreach($line in $lines) { if($line -match '^([^=]+)=(.*)$') { $credential[$matches[1]]=$matches[2] } }
if (!$credential.password) { throw 'Sign in to GitHub using Git Credential Manager first' }
$headers=@{Authorization="Bearer $($credential.password)";Accept='application/vnd.github+json';'User-Agent'='KAY-POS-Publisher';'X-GitHub-Api-Version'='2022-11-28'}
$base="https://api.github.com/repos/$repo"
$info=Invoke-RestMethod $base -Headers $headers
if ($info.private -or !$info.permissions.push) { throw 'Expected the public update repository with publisher access' }
$existing=Invoke-RestMethod "$base/releases" -Headers $headers
if ($existing | Where-Object tag_name -eq "v$Version") { throw 'This release already exists. Do not overwrite it.' }
$readmeExists = $true
try {
    Invoke-RestMethod "$base/contents/README.md" -Headers $headers | Out-Null
} catch {
    if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 404) {
        $readmeExists = $false
    } else { throw }
}
if (!$readmeExists) {
    $readme=@'
# KAY POS Updates

Official Windows x64 binary releases for KAY POS Rust.

Download a signed ZIP from Releases and extract pos_desktop.exe into your existing client folder after closing KAY POS. Keep your existing database configuration. Microsoft Edge WebView2 Runtime is required.

Version 0.2.0 is the first updater-enabled baseline. Install it once manually. Future compatible patch updates can be checked in Settings > App Updates and installed after signing out, from the login screen.

This repository does not contain application source, shop data, database credentials, or private signing keys. Never upload those files here.
'@
    $body=@{message='Initialize binary update repository';content=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($readme))} | ConvertTo-Json
    Invoke-RestMethod "$base/contents/README.md" -Method Put -Headers $headers -ContentType 'application/json' -Body $body | Out-Null
}
$notes=@"
KAY POS Rust $Version for Windows x64.

- Signed update support in Settings > App Updates and on the login screen.
- Finish work and sign out before installing an update. Unresolved checkout blocks installation.
- Receipt print width fix and regular-weight product names.
- Windows Printer and Paper size aligned on one row.

Initial installation: close KAY POS, extract the ZIP into the existing client folder, keep existing database configuration, and reopen. Version 0.2.0 requires this one manual installation; later compatible patch updates use the built-in updater.

No database migration is included. Microsoft Edge WebView2 Runtime is required. Pilot on one client first. Update signatures are not an Authenticode certificate; Windows SmartScreen may still warn.
"@
if ($NotesFile) { $notes = $customNotes }
$body=@{tag_name="v$Version";target_commitish=$info.default_branch;name="KAY POS $Version";body=$notes;draft=$true;prerelease=$false} | ConvertTo-Json
$release=Invoke-RestMethod "$base/releases" -Method Post -Headers $headers -ContentType 'application/json' -Body $body
$asset=Invoke-RestMethod "https://uploads.github.com/repos/$repo/releases/$($release.id)/assets?name=$name" -Method Post -Headers $headers -ContentType 'application/zip' -InFile $zip -TimeoutSec 600
if ($asset.size -ne (Get-Item -LiteralPath $zip).Length) { throw 'Uploaded size does not match local package. Release remains draft.' }
$hash=(Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($asset.digest -and $asset.digest -ne "sha256:$hash") { throw 'Uploaded digest does not match. Release remains draft.' }
if ($Publish) {
    $release=Invoke-RestMethod "$base/releases/$($release.id)" -Method Patch -Headers $headers -ContentType 'application/json' -Body '{"draft":false,"make_latest":"true"}'
}
[pscustomobject]@{URL=$release.html_url;Draft=$release.draft;Asset=$asset.name;Bytes=$asset.size;SHA256=$hash}
