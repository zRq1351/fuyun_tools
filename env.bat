@echo off
set "TRIPLET=%~1"
set "VCPKGRS_DYNAMIC=%~2"
set "OPENCV_MSVC_CRT=%~3"
set "HAS_OPENCV=0"
set "VCPKG_CANDIDATE="

echo [1/4] Checking Node.js / npm ...
where npm >nul 2>&1
if not "%ERRORLEVEL%"=="0" goto :no_npm

echo [2/4] Checking Rust toolchain ...
where cargo >nul 2>&1
if not "%ERRORLEVEL%"=="0" goto :no_cargo

echo [3/4] Checking frontend dependencies ...
if exist "%~dp0src\node_modules\@tauri-apps\cli\tauri.js" goto :probe
echo       First run: installing frontend dependencies via npm install ...
pushd "%~dp0src"
if not "%ERRORLEVEL%"=="0" goto :no_src
call npm install
if not "%ERRORLEVEL%"=="0" goto :npm_fail
popd
goto :probe

:no_npm
echo       [ERROR] npm not found. Install Node.js first: https://nodejs.org
exit /b 1

:no_cargo
echo       [ERROR] cargo not found. Install Rust first: https://rustup.rs
exit /b 1

:no_src
echo       [ERROR] src\ directory not found. Run this script inside the repository.
exit /b 1

:npm_fail
popd
echo       [ERROR] npm install failed. Check your network and retry.
exit /b 1

:probe
echo [4/4] Detecting optional tools: LLVM / CMake / Ninja / vcpkg+OpenCV ...

if not defined LIBCLANG_PATH if exist "C:\Program Files\LLVM\bin\libclang.dll" set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"
if not defined LIBCLANG_PATH for /f "delims=" %%i in ('where clang.exe 2^>nul') do if not defined LIBCLANG_PATH for %%j in ("%%i") do if exist "%%~dpjlibclang.dll" set "LIBCLANG_PATH=%%~dpj"
if defined LIBCLANG_PATH if not defined CLANG_PATH if exist "%LIBCLANG_PATH%\clang.exe" set "CLANG_PATH=%LIBCLANG_PATH%\clang.exe"

if exist "%ProgramFiles%\CMake\bin\cmake.exe" set "PATH=%ProgramFiles%\CMake\bin;%PATH%"
for /d %%d in ("%LOCALAPPDATA%\Microsoft\WinGet\Packages\Ninja-build.Ninja_*") do (
    if exist "%%d\ninja.exe" set "PATH=%%d;%PATH%"
    for /d %%e in ("%%d\*") do if exist "%%e\ninja.exe" set "PATH=%%e;%PATH%"
)

if defined VCPKG_ROOT if exist "%VCPKG_ROOT%\scripts\buildsystems\vcpkg.cmake" set "VCPKG_CANDIDATE=%VCPKG_ROOT%"
if not defined VCPKG_CANDIDATE if exist "D:\vcpkg\scripts\buildsystems\vcpkg.cmake" set "VCPKG_CANDIDATE=D:\vcpkg"
if not defined VCPKG_CANDIDATE for /f "delims=" %%i in ('where vcpkg.exe 2^>nul') do if not defined VCPKG_CANDIDATE for %%j in ("%%i") do if exist "%%~dpjscripts\buildsystems\vcpkg.cmake" set "VCPKG_CANDIDATE=%%~dpj"
if defined VCPKG_CANDIDATE set "VCPKG_ROOT=%VCPKG_CANDIDATE%"

set "OPENCV_DISABLE_PROBES=environment,cmake,vcpkg_cmake,pkg_config"
if defined OpenCV_DIR if not exist "%OpenCV_DIR%" set "OpenCV_DIR="
if not defined OpenCV_DIR if defined VCPKG_ROOT if exist "%VCPKG_ROOT%\installed\%TRIPLET%\share\opencv4" set "OpenCV_DIR=%VCPKG_ROOT%\installed\%TRIPLET%\share\opencv4"
if defined OpenCV_DIR if exist "%OpenCV_DIR%" set "HAS_OPENCV=1"

if "%HAS_OPENCV%"=="1" echo       OpenCV: enabled - %OpenCV_DIR%
if "%HAS_OPENCV%"=="0" echo       [TIP] vcpkg/OpenCV not found. longshot-opencv will be disabled: long-screenshot stitching unavailable.
if "%HAS_OPENCV%"=="0" echo       [TIP] To enable: install vcpkg and set VCPKG_ROOT, or set OpenCV_DIR, then re-run.
if defined LIBCLANG_PATH echo       LLVM: %LIBCLANG_PATH%
if not defined LIBCLANG_PATH echo       LLVM: not found; only required when the OpenCV feature is enabled.
exit /b 0
