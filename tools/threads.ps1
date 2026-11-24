# Wakeups (context switches) per second for each thread of the widget, with the thread name.
# Rust sets a named thread's name as its Windows thread description, which GetThreadDescription reads.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File tools\threads.ps1 [-Seconds 10] [-Name strom-widget]
param([int]$Seconds = 10, [string]$Name = 'strom-widget')

Add-Type -Namespace W -Name T -MemberDefinition @'
[DllImport("kernel32.dll")] public static extern System.IntPtr OpenThread(int access, bool inherit, int id);
[DllImport("kernel32.dll")] public static extern bool CloseHandle(System.IntPtr h);
[DllImport("kernel32.dll")] public static extern int GetThreadDescription(System.IntPtr h, out System.IntPtr desc);
[DllImport("kernel32.dll")] public static extern System.IntPtr LocalFree(System.IntPtr p);
'@

function Get-ThreadName([int]$tid) {
    $h = [W.T]::OpenThread(0x0800, $false, $tid) # THREAD_QUERY_LIMITED_INFORMATION
    if ($h -eq [IntPtr]::Zero) { return '' }
    $p = [IntPtr]::Zero
    $name = ''
    if ([W.T]::GetThreadDescription($h, [ref]$p) -ge 0 -and $p -ne [IntPtr]::Zero) {
        $name = [Runtime.InteropServices.Marshal]::PtrToStringUni($p)
        [void][W.T]::LocalFree($p)
    }
    [void][W.T]::CloseHandle($h)
    return $name
}

$procId = (Get-Process $Name -ErrorAction Stop | Select-Object -First 1).Id
function Get-Switches {
    $m = @{}
    Get-CimInstance Win32_PerfRawData_PerfProc_Thread | Where-Object { $_.IDProcess -eq $procId } |
        ForEach-Object { $m[[int]$_.IDThread] = [double]$_.ContextSwitchesPerSec }
    return $m
}
$a = Get-Switches
Start-Sleep -Seconds $Seconds
$b = Get-Switches
$b.Keys | ForEach-Object {
    $before = if ($a.ContainsKey($_)) { $a[$_] } else { $b[$_] }
    [pscustomobject]@{ Tid = $_; PerSec = [math]::Round(($b[$_] - $before) / $Seconds, 1); Name = Get-ThreadName $_ }
} | Sort-Object PerSec -Descending | Format-Table -AutoSize
