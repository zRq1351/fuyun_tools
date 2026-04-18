#[cfg(target_os = "windows")]
use std::os::windows::io::AsRawHandle;
#[cfg(target_os = "windows")]
use winapi::um::handleapi::CloseHandle;
#[cfg(target_os = "windows")]
use winapi::um::jobapi2::{AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject};
#[cfg(target_os = "windows")]
use winapi::um::winnt::{
    JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

#[cfg(target_os = "windows")]
lazy_static::lazy_static! {
    static ref GLOBAL_JOB_OBJECT: winapi::um::winnt::HANDLE = {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null_mut());
            if !job.is_null() {
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &mut info as *mut _ as *mut _,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
            }
            job
        }
    };
}

#[cfg(target_os = "windows")]
pub fn assign_to_global_job_object(child: &std::process::Child) {
    unsafe {
        let job = *GLOBAL_JOB_OBJECT;
        if !job.is_null() {
            AssignProcessToJobObject(job, child.as_raw_handle() as *mut _);
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn assign_to_global_job_object(_child: &std::process::Child) {
    // No-op on non-Windows
}
