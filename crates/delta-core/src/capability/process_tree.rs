//! A command and everything it starts, as one unit that can be ended together.
//!
//! The shell tool runs a command line through the platform shell, and that line may start
//! further processes (`npm run dev`, `sleep 600 &`). Ending only the shell leaves them running
//! with nobody watching. Each command therefore gets its own process group (Unix) or Job Object
//! (Windows), and a timeout or a cancel ends the whole group.
//!
//! A command that finishes on its own is left as it is: a background process it started on
//! purpose keeps running, as it always did.

use std::process::{Child, Command};

pub(super) struct ProcessTree {
    #[cfg(windows)]
    job: windows::Job,
}

impl ProcessTree {
    /// Put the command in a process group of its own. Call before `spawn`.
    pub(super) fn prepare(command: &mut Command) {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(not(unix))]
        let _ = command;
    }

    /// Start tracking the spawned `child` and everything it starts from here on.
    pub(super) fn attach(child: &Child) -> Self {
        #[cfg(windows)]
        {
            Self {
                job: windows::Job::around(child),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = child;
            Self {}
        }
    }

    /// End the child and every process it started. Safe to call on a child that already exited.
    pub(super) fn kill(&self, child: &mut Child) {
        #[cfg(unix)]
        {
            // SAFETY: `kill` takes plain integers. The negative pid names the process group that
            // `prepare` made, whose id is the child's pid; the child has not been waited for yet,
            // so the id cannot have been handed to another process.
            unsafe {
                libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
            }
        }
        #[cfg(windows)]
        self.job.terminate();
        let _ = child.kill();
    }

    /// The command finished on its own: whatever it left running is no longer this tool's to end.
    pub(super) fn release(&self) {
        #[cfg(windows)]
        self.job.release();
    }
}

#[cfg(windows)]
mod windows {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// A Job Object holding the command. While it is open, closing its last handle (which also
    /// happens when Delta itself dies) ends every process in it.
    pub(super) struct Job(HANDLE);

    impl Job {
        /// A job around `child`, or an empty one when Windows refuses; ending the tree then falls
        /// back to ending the child alone. A process the child started in the instant before it
        /// joined the job is not covered.
        pub(super) fn around(child: &Child) -> Self {
            // SAFETY: plain Win32 calls with valid arguments; the handles are owned by `Job`.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    return Self(std::ptr::null_mut());
                }
                let job = Self(job);
                if !job.set_flags(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)
                    || AssignProcessToJobObject(job.0, child.as_raw_handle() as HANDLE) == 0
                {
                    return Self(std::ptr::null_mut());
                }
                job
            }
        }

        fn set_flags(&self, flags: u32) -> bool {
            // SAFETY: `info` is a zeroed plain-data struct of the size passed with it.
            unsafe {
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = flags;
                SetInformationJobObject(
                    self.0,
                    JobObjectExtendedLimitInformation,
                    std::ptr::addr_of!(info).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                ) != 0
            }
        }

        pub(super) fn terminate(&self) {
            if !self.0.is_null() {
                // SAFETY: `self.0` is a job handle owned by this value.
                unsafe {
                    TerminateJobObject(self.0, 1);
                }
            }
        }

        pub(super) fn release(&self) {
            if !self.0.is_null() {
                self.set_flags(0);
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: closes the handle this value owns, once.
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }
}
