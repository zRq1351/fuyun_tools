@echo off
title fuyun_tools - build

call "%~dp0env.bat" x64-windows-static-md 0 dynamic
if not "%ERRORLEVEL%"=="0" goto :fail

set "TAURI_ARGS=build"
if "%HAS_OPENCV%"=="1" set "TAURI_ARGS=build --features longshot-opencv"
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
