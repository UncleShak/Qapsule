# Creates (or reuses) the self-signed code-signing certificate "CN=Qapsule Dev"
# in Cert:\CurrentUser\My and exports only the public .cer to target/identity.
# The private key never leaves the certificate store.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '../..')
$out = Join-Path $repo 'target/identity'
New-Item -ItemType Directory -Force $out | Out-Null

$cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq 'CN=Qapsule Dev' -and $_.NotAfter -gt (Get-Date) } | Sort-Object NotAfter -Descending | Select-Object -First 1
if (-not $cert) {
    $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=Qapsule Dev' -KeyExportPolicy NonExportable -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddYears(2)
}
$all = @(Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq 'CN=Qapsule Dev' })
if ($all.Count -gt 1) { Write-Warning "$($all.Count) CN=Qapsule Dev certificates exist in CurrentUser\My; remove the extras." }
$cer = Join-Path $out 'qapsule-dev.cer'
Export-Certificate -Cert $cert -FilePath $cer -Type CERT | Out-Null
Write-Host "Thumbprint: $($cert.Thumbprint)"
Write-Host "Public cert: $cer"
Write-Host 'Next: build with scripts/identity/build-identity.ps1, then run scripts/identity/trust-dev-cert.ps1 from an elevated shell.'
