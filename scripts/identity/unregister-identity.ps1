# Removes the Qynetik.Qapsule.Dev package registration if present.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '../..')
$out = Join-Path $repo 'target/identity'
New-Item -ItemType Directory -Force $out | Out-Null

$pkg = Get-AppxPackage -Name 'Qynetik.Qapsule.Dev'
if ($pkg) { $pkg | Remove-AppxPackage; Write-Host 'Removed Qynetik.Qapsule.Dev.' } else { Write-Host 'Not registered.' }
Write-Host 'If you trusted the dev cert, run untrust-dev-cert.ps1 elevated.'
