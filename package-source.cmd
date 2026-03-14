@echo off
setlocal

set "SCRIPT_DIR=%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%package-source.ps1"

if errorlevel 1 (
  echo Packaging failed.
  exit /b 1
)

echo Packaging completed.
exit /b 0
