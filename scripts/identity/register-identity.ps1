# Registers the development identity with target/debug (or, with -Release,
# target/release) as its external location, replacing any earlier registration.
# Default: unsigned loose registration, which needs Developer Mode and no
# certificate. -Signed: registers the signed msix instead (needs the dev cert
# trusted via trust-dev-cert.ps1). -Release needs a release build made with the
# identity, for local measurement only; never ship that build:
#   $env:QAPSULE_DEV_IDENTITY='1'; bun run tauri build --no-bundle
param([switch]$Signed, [switch]$Release)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '../..')
$out = Join-Path $repo 'target/identity'
if ($Release) {
    $location = Join-Path $repo 'target/release'
    $build = "`$env:QAPSULE_DEV_IDENTITY='1'; bun run tauri build --no-bundle"
} else {
    $location = Join-Path $repo 'target/debug'
    $build = 'cargo build -p qapsule-app'
}
if (-not (Test-Path $location)) { Write-Error "Missing $location. Build the app first ($build)." }

if ($Signed) {
    $msix = Join-Path $out 'Qapsule.Dev.msix'
    if (-not (Test-Path $msix)) { Write-Error "Missing $msix. Run scripts/identity/build-identity.ps1 first." }
} else {
    $dev = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock' -Name AllowDevelopmentWithoutDevLicense -ErrorAction SilentlyContinue
    if (-not $dev -or $dev.AllowDevelopmentWithoutDevLicense -ne 1) {
        Write-Error 'Turn on Settings -> System -> For developers -> Developer Mode, or use -Signed'
    }
}

Get-AppxPackage -Name 'Qynetik.Qapsule.Dev' | Remove-AppxPackage
if ($Signed) {
    Add-AppxPackage -Path $msix -ExternalLocation $location
} else {
    Add-AppxPackage -Register (Join-Path $repo 'packaging/identity/AppxManifest.xml') -ExternalLocation $location
}
Write-Host "Registered Qynetik.Qapsule.Dev with external location $location"
Write-Host "Next: & '$location/qapsule-app.exe' --identity-check"
