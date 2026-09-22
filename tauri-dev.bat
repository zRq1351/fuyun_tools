@echo off
title fuyun_tools - dev

call "%~dp0env.bat" x64-windows 1
if not "%ERRORLEVEL%"=="0" goto :fail

set "TAURI_ARGS=dev"
if "%HAS_OPENCV%"=="1" set "TAURI_ARGS=dev --features longshot-opencv"
if "%HAS_OPENCV%"=="0" echo [WARN] longshot-opencv disabled: vcpkg/OpenCV not found. Long-screenshot stitching will be unavailable.

cd /d "%~dp0src"
call npm run tauri -- %TAURI_ARGS%
if not "%ERRORLEVEL%"=="0" goto :fail
exit /b 0

:fail
echo.
echo [FAILED] See log above. Attach the log when reporting issues.
pause
exit /b 1
