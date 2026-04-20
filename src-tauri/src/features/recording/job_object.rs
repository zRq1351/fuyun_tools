#[cfg(target_os = "windows")]
use std::os::windows::io::AsRawHandle;
#[cfg(target_os = "windows")]
use winapi::um::jobapi2::{AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject};
#[cfg(target_os = "windows")]
use winapi::um::winnt::{
    JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

#[cfg(target_os = "windows")]
fn global_job_object() -> winapi::um::winnt::HANDLE {
    use std::sync::atomic::{AtomicPtr, Ordering};
    static JOB_OBJECT: AtomicPtr<winapi::ctypes::c_void> = AtomicPtr::new(std::ptr::null_mut());

    let current = JOB_OBJECT.load(Ordering::Acquire);
    if !current.is_null() {
        return current;
    }

    
    let new_job = unsafe {
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
    };

    
    match JOB_OBJECT.compare_exchange(
        std::ptr::null_mut(),
        new_job,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => new_job,
        Err(existing) => {
            
            if !new_job.is_null() {
                unsafe { winapi::um::handleapi::CloseHandle(new_job); }
            }
            existing
        }
    }
}

#[cfg(target_os = "windows")]
pub fn assign_to_global_job_object(child: &std::process::Child) {
    unsafe {
        let job = global_job_object();
        if !job.is_null() {
            AssignProcessToJobObject(job, child.as_raw_handle() as *mut _);
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn assign_to_global_job_object(_child: &std::process::Child) {
    
}
