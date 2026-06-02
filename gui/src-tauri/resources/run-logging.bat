@echo off
setlocal

cd /d "%~dp0"

if not exist logs mkdir logs

echo ==========================================
echo Anthropic Provider Gateway
echo ==========================================
echo Working directory:
echo %CD%
echo.

echo Checking existing process on port 4000...

for /f "tokens=5" %%a in ('netstat -ano ^| findstr /R /C:":4000 .*LISTENING"') do (
    echo Found existing process using port 4000. PID=%%a
    echo Killing PID %%a ...
    taskkill /PID %%a /F
)

echo.
echo Starting proxy with logging...
echo.

powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$ts = Get-Date -Format 'yyyyMMdd-HHmmss';" ^
  "$log = Join-Path 'logs' ('proxy-' + $ts + '.log');" ^
  "Write-Host ('Log file: ' + $log);" ^
  "python proxy_server.py 2>&1 | Tee-Object $log"

echo.
echo Proxy stopped.
pause
