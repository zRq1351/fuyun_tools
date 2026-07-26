@echo off
set "VCPKG_ROOT=D:\vcpkg"
set "VCPKGRS_TRIPLET=x64-windows-static-md"
set "VCPKGRS_DYNAMIC=0"
set "OpenCV_DIR=D:\vcpkg\installed\x64-windows-static-md\share\opencv4"
set "OPENCV_DISABLE_PROBES=environment,cmake,vcpkg_cmake,pkg_config"
set "OPENCV_MSVC_CRT=dynamic"
set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"
set "CLANG_PATH=C:\Program Files\LLVM\bin\clang.exe"
set "PATH=C:\Program Files\LLVM\bin;C:\Program Files\CMake\bin;C:\Users\ZRQ\AppData\Local\Microsoft\WinGet\Packages\Ninja-build.Ninja_Microsoft.Winget.Source_8wekyb3d8bbwe;%PATH%"
cd /d "%~dp0"
npm run tauri -- build --features longshot-opencv
