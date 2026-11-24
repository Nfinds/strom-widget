# Measures the running widget: CPU share, memory, threads and wakeups (context switches per
# second over all its threads). Same method before and after a change, so the numbers compare.
#   powershell -File tools\measure.ps1 [-Seconds 30] [-Name strom-widget]
param([int]$Seconds = 30, [string]$Name = "strom-widget")

$p = Get-Process -Name $Name -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $p) { Write-Output "Process '$Name' is not running."; exit 1 }

# Raw thread counters (English WMI class names, independent of the Windows language).
function Get-Switches([int]$procId) {
    $sum = 0
    Get-CimInstance Win32_PerfRawData_PerfProc_Thread -Filter "IDProcess = $procId" |
        ForEach-Object { $sum += [uint64]$_.ContextSwitchesPerSec }
    return $sum
}

$cpu0 = $p.TotalProcessorTime.TotalMilliseconds
$sw0 = Get-Switches $p.Id
Start-Sleep -Seconds $Seconds
$p.Refresh()
$cpu1 = $p.TotalProcessorTime.TotalMilliseconds
$sw1 = Get-Switches $p.Id

$cores = [Environment]::ProcessorCount
[pscustomobject]@{
    Seconds        = $Seconds
    CpuPercentAll  = [math]::Round(($cpu1 - $cpu0) / ($Seconds * 1000) * 100 / $cores, 3)
    WakeupsPerSec  = [math]::Round(($sw1 - $sw0) / $Seconds, 1)
    WorkingSetMB   = [math]::Round($p.WorkingSet64 / 1MB, 1)
    PrivateMB      = [math]::Round($p.PrivateMemorySize64 / 1MB, 1)
    Threads        = $p.Threads.Count
} | Format-List
