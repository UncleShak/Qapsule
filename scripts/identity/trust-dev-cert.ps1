# Imports the public dev certificate into Cert:\LocalMachine\TrustedPeople so
# Windows accepts the signed identity package. Requires an elevated shell.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '../..')
$out = Join-Path $repo 'target/identity'
New-Item -ItemType Directory -Force $out | Out-Null

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Error 'This script needs an elevated (Run as administrator) PowerShell.'
}
$cer = Join-Path $out 'qapsule-dev.cer'
if (-not (Test-Path $cer)) { Write-Error "Missing $cer. Run scripts/identity/new-dev-cert.ps1 first." }
Import-Certificate -FilePath $cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople | Out-Null
Write-Host 'Trusted CN=Qapsule Dev in LocalMachine\TrustedPeople.'
Write-Host 'Next: cargo build -p qapsule-app, then scripts/identity/register-identity.ps1, then target/debug/qapsule-app.exe --identity-check'
