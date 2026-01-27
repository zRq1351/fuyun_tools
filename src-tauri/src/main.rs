#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use fuyun_tool_lib::run;

fn main() {
    // 使用跨平台的单实例锁名称
    let instance = single_instance::SingleInstance::new("fuyun_shear_manager")
        .expect("未能创建单实例锁");

    if !instance.is_single() {
        #[cfg(windows)]
        unsafe {
            use std::ffi::OsStr;
            use std::os::windows::ffi::OsStrExt;
            use winapi::um::winuser::{MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND};

            let msg: Vec<u16> = OsStr::new("剪贴板管理器已经在运行中！请检查系统托盘。")
                .encode_wide()
                .chain(Some(0).into_iter())
                .collect();
            let caption: Vec<u16> = OsStr::new("提示")
                .encode_wide()
                .chain(Some(0).into_iter())
                .collect();

            MessageBoxW(
                std::ptr::null_mut(),
                msg.as_ptr(),
                caption.as_ptr(),
                MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND,
            );
        }
        
        #[cfg(not(windows))]
        eprintln!("剪贴板管理器已经在运行中！");
        
        std::process::exit(0);
    }

    run();
}