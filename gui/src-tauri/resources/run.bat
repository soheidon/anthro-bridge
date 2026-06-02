@echo off
REM Anthropic Provider Gateway — 起動スクリプト
REM 既存プロセスをkillしてから起動する

cd /d "%~dp0"

echo Stopping existing proxy on port 4000...
for /f "tokens=5" %%a in ('netstat -ano ^| findstr ":4000.*LISTENING"') do (
    echo Killing PID %%a
    taskkill /F /PID %%a >nul 2>&1
)

echo Starting proxy...
python -m uvicorn proxy_server:app --host 127.0.0.1 --port 4000 --log-level info
