# Removes every CN=Qapsule Dev certificate from Cert:\LocalMachine\TrustedPeople.
# Requires an elevated shell.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Error 'This script needs an elevated (Run as administrator) PowerShell.'
}
$certs = @(Get-ChildItem Cert:\LocalMachine\TrustedPeople | Where-Object { $_.Subject -eq 'CN=Qapsule Dev' })
$certs | ForEach-Object { Remove-Item $_.PSPath }
Write-Host "Removed $($certs.Count) CN=Qapsule Dev certificate(s) from LocalMachine\TrustedPeople."
