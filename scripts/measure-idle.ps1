<#
.SYNOPSIS
    Measures Qapsule's CPU and memory against the budgets in docs/SPEC.md.

.DESCRIPTION
    Samples qapsule-app.exe and every WebView2 process it started (found by walking
    the process tree, so other apps' WebView2 processes are ignored). CPU is the
    average share of the whole machine, like Task Manager; memory is the private
    working set, also like Task Manager.

    Measure a release build (`bun run tauri build --no-bundle`), not `tauri dev`:
    a debug build and the Vite dev server both cost far more than the real app.

.PARAMETER Scenario
    idle      Nothing happening; the peek shows the time. Budget: CPU < 1%, memory < 150 MB.
    activity  Pushes a progress update 10 times a second. Budget: CPU < 3%.
    hidden    Measures as things are; put Qapsule out of sight first (Peek on demand,
              or a fullscreen app in Auto). Budget: CPU < 0.5%, memory < 30 MB.

.PARAMETER Seconds
    How long to measure. The spec's numbers use 600 (idle, hidden) and 120 (activity).

.EXAMPLE
    ./scripts/measure-idle.ps1 -Scenario idle -Seconds 600
#>
param(
    [ValidateSet("idle", "activity", "hidden")]
    [string]$Scenario = "idle",
    [int]$Seconds = 600,
    [int]$SampleSeconds = 5,
    [string]$Cli = (Join-Path $PSScriptRoot "..\target\release\qapsule.exe")
)

$ErrorActionPreference = "Stop"

$budgets = @{
    idle     = @{ Cpu = 1.0; MemoryMB = 150 }
    activity = @{ Cpu = 3.0; MemoryMB = $null }
    hidden   = @{ Cpu = 0.5; MemoryMB = 30 }
}

function Get-QapsuleProcesses {
    $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name
    $app = $all | Where-Object Name -eq "qapsule-app.exe"
    if (-not $app) { throw "qapsule-app.exe isn't running." }
    $ids = [System.Collections.Generic.HashSet[int]]::new()
    $queue = [System.Collections.Generic.Queue[int]]::new()
    foreach ($a in $app) { [void]$ids.Add($a.ProcessId); $queue.Enqueue($a.ProcessId) }
    while ($queue.Count) {
        $parent = $queue.Dequeue()
        foreach ($child in $all | Where-Object ParentProcessId -eq $parent) {
            if ($ids.Add($child.ProcessId)) { $queue.Enqueue($child.ProcessId) }
        }
    }
    $ids
}

function Get-PrivateMB([System.Collections.Generic.HashSet[int]]$ids) {
    $total = 0
    foreach ($p in Get-CimInstance Win32_PerfRawData_PerfProc_Process) {
        if ($ids.Contains([int]$p.IDProcess)) { $total += $p.WorkingSetPrivate }
    }
    [math]::Round($total / 1MB, 1)
}

$cores = [Environment]::ProcessorCount
$pusher = $null
if ($Scenario -eq "activity") {
    if (-not (Test-Path $Cli)) { throw "No CLI at $Cli. Build it with: cargo build --release -p qapsule-cli" }
    # One process holds a pipe connection and pushes 10 updates a second.
    $pusher = Start-Job -ArgumentList (Resolve-Path $Cli).Path, $Seconds -ScriptBlock {
        param($cli, $seconds)
        $pipe = New-Object System.IO.Pipes.NamedPipeClientStream(".", "qapsule", [System.IO.Pipes.PipeDirection]::InOut)
        $pipe.Connect(2000)
        $writer = New-Object System.IO.StreamWriter($pipe); $writer.AutoFlush = $true
        $reader = New-Object System.IO.StreamReader($pipe)
        $end = (Get-Date).AddSeconds($seconds)
        $i = 0
        while ((Get-Date) -lt $end) {
            $p = ($i % 100) / 100
            $writer.WriteLine("{`"v`":1,`"type`":`"upsert`",`"activity`":{`"id`":`"measure`",`"source`":`"measure`",`"title`":`"Measuring`",`"state`":`"running`",`"progress`":$p}}")
            [void]$reader.ReadLine()
            $i++
            Start-Sleep -Milliseconds 100
        }
        $writer.WriteLine('{"v":1,"type":"clear","source":"measure"}'); [void]$reader.ReadLine()
        $pipe.Dispose()
    }
}

Write-Host "Measuring '$Scenario' for $Seconds s ($cores logical processors)..."
$cpuSeen = @{}     # pid -> CPU time at the last sample
$cpuUsed = 0.0     # seconds of CPU used in the window
$memory = [System.Collections.Generic.List[double]]::new()
$processCounts = [System.Collections.Generic.List[int]]::new()
$clock = [System.Diagnostics.Stopwatch]::StartNew()

while ($true) {
    $ids = Get-QapsuleProcesses
    foreach ($id in $ids) {
        $p = Get-Process -Id $id -ErrorAction SilentlyContinue
        if (-not $p) { continue }
        $now = $p.TotalProcessorTime.TotalSeconds
        # A process seen for the first time after the start only counts from now on,
        # except new WebView2 processes, whose start-up cost is real cost.
        if ($cpuSeen.ContainsKey($id)) { $cpuUsed += $now - $cpuSeen[$id] }
        elseif ($clock.Elapsed.TotalSeconds -gt 0.5) { $cpuUsed += $now }
        $cpuSeen[$id] = $now
    }
    $memory.Add((Get-PrivateMB $ids))
    $processCounts.Add($ids.Count)
    if ($clock.Elapsed.TotalSeconds -ge $Seconds) { break }
    Start-Sleep -Seconds $SampleSeconds
}

if ($pusher) { Receive-Job $pusher -Wait -AutoRemoveJob | Out-Null }

$elapsed = $clock.Elapsed.TotalSeconds
$cpu = [math]::Round(100 * $cpuUsed / ($elapsed * $cores), 2)
$memAvg = [math]::Round(($memory | Measure-Object -Average).Average, 1)
$memMax = ($memory | Measure-Object -Maximum).Maximum
$budget = $budgets[$Scenario]
$cpuOk = $cpu -lt $budget.Cpu
$memOk = (-not $budget.MemoryMB) -or ($memAvg -lt $budget.MemoryMB)

[pscustomobject]@{
    Scenario        = $Scenario
    Seconds         = [math]::Round($elapsed)
    "CPU %"         = $cpu
    "CPU budget %"  = $budget.Cpu
    "Memory MB avg" = $memAvg
    "Memory MB max" = $memMax
    "Memory budget" = if ($budget.MemoryMB) { "$($budget.MemoryMB) MB" } else { "-" }
    Processes       = "{0}-{1}" -f ($processCounts | Measure-Object -Minimum).Minimum, ($processCounts | Measure-Object -Maximum).Maximum
    Result          = if ($cpuOk -and $memOk) { "PASS" } else { "FAIL" }
} | Format-List
