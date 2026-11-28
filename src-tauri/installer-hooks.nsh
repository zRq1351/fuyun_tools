; NSIS 安装钩子：安装/升级前结束可能占用安装目录文件的进程。
;
; 背景（2026-11-28 实测）：投屏功能使用的 adb.exe 会以守护进程形式常驻，
; 即使应用退出也会继续运行，从而锁住安装目录下的
;   _up_\third_party\scrcpy\AdbWinApi.dll
; 导致覆盖安装失败并弹出：
;   Error opening file for writing: ...\AdbWinApi.dll
;
; 因此这里在写文件之前强制结束 adb 与应用本体；
; 应用侧也在 RunEvent::Exit 中执行 adb kill-server，两层兜底。

!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Stopping processes that lock install files (adb / fuyun_tools)..."
  nsExec::Exec 'taskkill /F /IM adb.exe'
  Pop $0
  nsExec::Exec 'taskkill /F /IM fuyun_tools.exe'
  Pop $0
!macroend
