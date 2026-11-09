@echo off
chcp 65001 >nul
title THIET LAP PAGEFILE SANG O C (SSD) - CDCEDIT
cls

echo ============================================================
echo   KIEM TRA QUYEN ADMINISTRATOR...
echo ============================================================

net session >nul 2>&1
if %errorlevel% neq 0 (
    echo Đang yêu cầu quyền Administrator (UAC), vui lòng chọn Yes...
    powershell -Command "Start-Process cmd -ArgumentList '/c \"\"%~f0\"\"' -Verb RunAs"
    exit /b
)

echo ============================================================
echo   ĐANG THIẾT LẬP PAGEFILE SANG Ổ C: (SSD)...
echo ============================================================

powershell -NoProfile -ExecutionPolicy Bypass -Command ^
    "$cs = Get-CimInstance Win32_ComputerSystem; " ^
    "Set-CimInstance -InputObject $cs -Property @{AutomaticManagedPagefile = $false}; " ^
    "$regPath = 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management'; " ^
    "Set-ItemProperty -Path $regPath -Name 'PagingFiles' -Value @('C:\pagefile.sys 0 0') -Type MultiString; " ^
    "Get-CimInstance Win32_PageFileSetting -ErrorAction SilentlyContinue | Where-Object { $_.Name -like '*E:*' } | Remove-CimInstance -ErrorAction SilentlyContinue; " ^
    "$cSetting = Get-CimInstance Win32_PageFileSetting -ErrorAction SilentlyContinue | Where-Object { $_.Name -like '*C:*' }; " ^
    "if (-not $cSetting) { New-CimInstance -ClassName Win32_PageFileSetting -Property @{Name = 'C:\pagefile.sys'; InitialSize = 0; MaximumSize = 0} -ErrorAction SilentlyContinue | Out-Null }; " ^
    "Write-Host 'ĐÃ CẬP NHẬT XONG REGISTRY VÀ WMI!' -ForegroundColor Green;"

if %errorlevel% equ 0 (
    echo.
    echo ============================================================
    echo   [THÀNH CÔNG] ĐÃ THIẾT LẬP PAGEFILE SANG Ổ C: (SSD)!
    echo ============================================================
    echo.
    echo   - Ổ E: (HDD cơ) sẽ không còn bị nghẽn làm RAM ảo nữa.
    echo   - Ổ C: (SSD tốc độ cao) sẽ đảm nhiệm RAM ảo cho hệ thống.
    echo.
    echo   LƯU Ý QUAN TRỌNG:
    echo   Bạn cần KHỞI ĐỘNG LẠI MÁY TÍNH (Restart PC) để thay đổi có hiệu lực.
    echo.
) else (
    echo.
    echo [LỖI] Không thể thiết lập được.
    echo.
)

pause
