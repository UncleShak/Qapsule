# Packs packaging/identity into target/identity/Qapsule.Dev.msix and signs it
# /nv: makeappx otherwise rejects the manifest because qapsule-app.exe is not in the package (it lives in the external location).
# with the CN=Qapsule Dev certificate from Cert:\CurrentUser\My.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '../..')
$out = Join-Path $repo 'target/identity'
New-Item -ItemType Directory -Force $out | Out-Null

$bin = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Directory | Where-Object { $_.Name -match '^\d+\.' -and (Test-Path (Join-Path $_.FullName 'x64\makeappx.exe')) } | Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
if (-not $bin) { Write-Error 'Windows SDK (makeappx.exe) not found.' }
$tools = Join-Path $bin.FullName 'x64'
$cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq 'CN=Qapsule Dev' -and $_.NotAfter -gt (Get-Date) } | Sort-Object NotAfter -Descending | Select-Object -First 1
if (-not $cert) { Write-Error 'No CN=Qapsule Dev certificate. Run scripts/identity/new-dev-cert.ps1 first.' }
$msix = Join-Path $out 'Qapsule.Dev.msix'
& (Join-Path $tools 'makeappx.exe') pack /d (Join-Path $repo 'packaging/identity') /p $msix /nv /o
if ($LASTEXITCODE -ne 0) { Write-Error 'makeappx failed.' }
& (Join-Path $tools 'signtool.exe') sign /fd SHA256 /sha1 $cert.Thumbprint $msix
if ($LASTEXITCODE -ne 0) { Write-Error 'signtool failed.' }
Write-Host "Built and signed $msix"
Write-Host 'Next: run scripts/identity/trust-dev-cert.ps1 from an elevated shell (once), then scripts/identity/register-identity.ps1.'
