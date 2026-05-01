#[cfg(target_os = "windows")]
use std::os::windows::io::AsRawHandle;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::CloseHandle;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HANDLE;
#[cfg(target_os = "windows")]
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

#[cfg(target_os = "windows")]
fn global_job_object() -> HANDLE {
    use std::sync::atomic::{AtomicPtr, Ordering};
    static JOB_OBJECT: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

    let current = JOB_OBJECT.load(Ordering::Acquire);
    if !current.is_null() {
        return HANDLE(current);
    }

    let new_job = unsafe {
        let job = CreateJobObjectW(None, None);
        if let Ok(job_handle) = job {
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let result = SetInformationJobObject(
                job_handle,
                JobObjectExtendedLimitInformation,
                &mut info as *mut _ as *mut _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if result.is_err() {
                log::error!("SetInformationJobObject 失败: {}", std::io::Error::last_os_error());
                let _ = CloseHandle(job_handle);
                return HANDLE(std::ptr::null_mut());
            }
            job_handle
        } else {
            return HANDLE(std::ptr::null_mut());
        }
    };

    let new_job_ptr = new_job.0;
    match JOB_OBJECT.compare_exchange(
        std::ptr::null_mut(),
        new_job_ptr,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => new_job,
        Err(existing) => {
            if !new_job_ptr.is_null() {
                unsafe {
                    let _ = CloseHandle(new_job);
                }
            }
            HANDLE(existing)
        }
    }
}

#[cfg(target_os = "windows")]
pub fn assign_to_global_job_object(child: &std::process::Child) {
    unsafe {
        let job = global_job_object();
        if !job.0.is_null() {
            let result = AssignProcessToJobObject(job, HANDLE(child.as_raw_handle() as *mut _));
            if result.is_err() {
                log::warn!(
                    "AssignProcessToJobObject 失败(pid={}): {}",
                    child.id(),
                    std::io::Error::last_os_error()
                );
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn assign_to_global_job_object(_child: &std::process::Child) {}
