$ErrorActionPreference = 'Stop'

try {
    # 1. Disable Automatic Managed Pagefile across all drives
    $cs = Get-CimInstance Win32_ComputerSystem
    Set-CimInstance -InputObject $cs -Property @{AutomaticManagedPagefile = $false}

    # 2. Update Registry to only have C:\pagefile.sys (System Managed: 0 0)
    $regPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management"
    Set-ItemProperty -Path $regPath -Name "PagingFiles" -Value @("C:\pagefile.sys 0 0") -Type MultiString

    # 3. Clean up WMI PageFileSetting for E: and ensure C: exists
    Get-CimInstance Win32_PageFileSetting -ErrorAction SilentlyContinue | Where-Object { $_.Name -like "*E:*" } | Remove-CimInstance -ErrorAction SilentlyContinue

    $cSetting = Get-CimInstance Win32_PageFileSetting -ErrorAction SilentlyContinue | Where-Object { $_.Name -like "*C:*" }
    if (-not $cSetting) {
        New-CimInstance -ClassName Win32_PageFileSetting -Property @{Name = "C:\pagefile.sys"; InitialSize = 0; MaximumSize = 0} -ErrorAction SilentlyContinue | Out-Null
    }

    # 4. Write success status to temp
    $confirmPath = Join-Path $env:TEMP "pagefile_ssd_status.txt"
    "SUCCESS: Pagefile configured on C: (SSD) successfully at $(Get-Date)" | Out-File -FilePath $confirmPath -Encoding utf8

    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show(
        "Đã thiết lập Pagefile sang ổ C: (SSD) thành công!`n`nSau khi bạn khởi động lại máy tính (Restart), Windows sẽ sử dụng RAM ảo trên ổ SSD C:, giúp tăng tốc độ build lên nhiều lần!`n`nHãy bấm OK rồi khởi động lại máy tính bất kỳ lúc nào bạn thuận tiện.",
        "Thiết lập hoàn tất - CDCedit SSD Pagefile",
        [System.Windows.Forms.MessageBoxButtons]::OK,
        [System.Windows.Forms.MessageBoxIcon]::Information
    ) | Out-Null
} catch {
    $errPath = Join-Path $env:TEMP "pagefile_ssd_status.txt"
    "ERROR: $($_.Exception.Message)" | Out-File -FilePath $errPath -Encoding utf8
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show(
        "Có lỗi khi thiết lập: $($_.Exception.Message)",
        "Lỗi thiết lập Pagefile",
        [System.Windows.Forms.MessageBoxButtons]::OK,
        [System.Windows.Forms.MessageBoxIcon]::Error
    ) | Out-Null
}
