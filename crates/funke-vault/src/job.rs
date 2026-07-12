//! Kill-on-close Job Object: the kernel-side bound on `bw serve`'s lifetime.
//!
//! [`Vault::shutdown`](crate::Vault::shutdown) kills the server on a *graceful* exit —
//! but a crashed or force-killed funke would leave an unlocked `bw serve`, with its
//! unauthenticated loopback REST API, listening indefinitely. Assigning the child to a
//! job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` makes the kernel close it when
//! the last handle dies — and the last handle lives in this process, so the server can
//! no longer outlive funke, however funke ends.
//!
//! Failure to create or assign the job degrades to today's behavior (graceful-exit kill
//! only): the job is belt and braces, never a reason to refuse to start the vault.

use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::process::Child;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

/// An anonymous job object that kills its members when the handle closes. Owning it is
/// owning the child's lifetime: drop the job (or die without dropping it — the kernel
/// closes leaked handles either way) and every assigned process is terminated.
pub struct KillOnDropJob(HANDLE);

// A job object handle is process-global kernel state; the wrapped pointer is a kernel
// handle, not shared memory.
unsafe impl Send for KillOnDropJob {}
unsafe impl Sync for KillOnDropJob {}

impl KillOnDropJob {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let job = CreateJobObjectW(None, PCWSTR::null()).map_err(|e| format!("CreateJobObjectW: {e}"))?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let set = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if let Err(e) = set {
                let _ = CloseHandle(job);
                return Err(format!("SetInformationJobObject: {e}"));
            }
            Ok(Self(job))
        }
    }

    /// Bind a spawned child to the job. Called right after `spawn()` — the gap in which
    /// the child runs unassigned is unavoidable and irrelevant here (the job exists to
    /// cover funke dying later, not in these microseconds).
    pub fn assign(&self, child: &Child) -> Result<(), String> {
        unsafe {
            AssignProcessToJobObject(self.0, HANDLE(child.as_raw_handle()))
                .map_err(|e| format!("AssignProcessToJobObject: {e}"))
        }
    }
}

impl Drop for KillOnDropJob {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
