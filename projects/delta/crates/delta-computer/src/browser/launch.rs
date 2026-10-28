//! Finding and starting the browser Delta drives.
//!
//! Delta uses the Edge or Chrome already installed (Edge ships with Windows), in a profile of its
//! own under Delta's state folder, never the user's everyday profile: their cookies, history and
//! saved passwords are not reachable from here, and a login made in Delta's window stays in
//! Delta's profile only.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// How the browser is started. The product uses [`Settings::from_env`]; tests set fields directly.
#[derive(Debug, Clone)]
pub struct Settings {
    /// A specific browser program. When unset the usual install locations are searched.
    pub executable: Option<PathBuf>,
    /// Run without a window. The product shows the window so the user can watch and log in.
    pub headless: bool,
    /// Extra command-line flags (tests pass `--no-sandbox` when running as root).
    pub extra_args: Vec<String>,
    /// Allow pages on 127.0.0.1. Tests only; the product never relaxes the network guard.
    pub allow_loopback: bool,
    /// Close the browser after this long without a browser tool call.
    pub idle_timeout: Duration,
    /// How long a cold start may take before the launch is reported as failed.
    pub launch_timeout: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            executable: None,
            headless: false,
            extra_args: Vec::new(),
            allow_loopback: false,
            idle_timeout: Duration::from_secs(10 * 60),
            launch_timeout: Duration::from_secs(20),
        }
    }
}

impl Settings {
    pub fn from_env() -> Settings {
        Settings {
            executable: std::env::var_os("DELTA_BROWSER_PATH")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            ..Settings::default()
        }
    }
}

fn candidates() -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        for var in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(var) {
                let base = PathBuf::from(base);
                list.push(base.join(r"Microsoft\Edge\Application\msedge.exe"));
                list.push(base.join(r"Google\Chrome\Application\chrome.exe"));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        list.push("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge".into());
        list.push("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into());
        list.push("/Applications/Chromium.app/Contents/MacOS/Chromium".into());
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        for name in [
            "microsoft-edge",
            "microsoft-edge-stable",
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
        ] {
            if let Some(path) = std::env::var_os("PATH") {
                for dir in std::env::split_paths(&path) {
                    list.push(dir.join(name));
                }
            }
        }
    }
    list
}

pub fn find_browser(settings: &Settings) -> Result<PathBuf, String> {
    if let Some(path) = &settings.executable {
        return if path.is_file() {
            Ok(path.clone())
        } else {
            Err(format!(
                "the browser program was not found: {}",
                path.display()
            ))
        };
    }
    candidates()
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            "no browser was found: Microsoft Edge or Google Chrome needs to be installed".to_owned()
        })
}

/// A running browser. Dropping it ends the browser.
pub struct Process {
    child: Child,
    #[cfg(windows)]
    _job: Option<job::Job>,
}

impl Process {
    pub fn has_exited(&mut self) -> bool {
        !matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Where the browser said it is listening.
pub struct Endpoint {
    pub port: u16,
    /// Path of the browser-level websocket, e.g. `/devtools/browser/<id>`.
    pub path: String,
}

fn read_endpoint(file: &Path) -> Option<Endpoint> {
    let text = std::fs::read_to_string(file).ok()?;
    let mut lines = text.lines();
    let port = lines.next()?.trim().parse().ok()?;
    let path = lines.next()?.trim().to_owned();
    path.starts_with('/').then_some(Endpoint { port, path })
}

pub fn launch(settings: &Settings, profile: &Path) -> Result<(Process, Endpoint), String> {
    let program = find_browser(settings)?;
    std::fs::create_dir_all(profile)
        .map_err(|error| format!("could not create the browser profile: {error}"))?;
    // A stale file from an earlier run would point at a port nobody listens on.
    let port_file = profile.join("DevToolsActivePort");
    let _ = std::fs::remove_file(&port_file);

    let mut command = Command::new(&program);
    command
        .arg("--remote-debugging-port=0")
        .arg("--remote-debugging-address=127.0.0.1")
        .arg(format!("--user-data-dir={}", profile.display()))
        .args([
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-extensions",
            "--disable-sync",
            "--disable-default-apps",
            "--disable-background-networking",
            "--disable-component-update",
            "--deny-permission-prompts",
            "--password-store=basic",
            "--disable-features=Translate,PasswordManagerOnboarding,AutofillServerCommunication",
            "--window-size=1280,860",
        ]);
    if settings.headless {
        command.arg("--headless=new");
    }
    command.args(&settings.extra_args);
    command.arg("about:blank");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = command
        .spawn()
        .map_err(|error| format!("could not start the browser: {error}"))?;

    #[cfg(windows)]
    let job = job::Job::bind(&child);
    let mut process = Process {
        child,
        #[cfg(windows)]
        _job: job,
    };

    let deadline = Instant::now() + settings.launch_timeout;
    loop {
        if let Some(endpoint) = read_endpoint(&port_file) {
            return Ok((process, endpoint));
        }
        if process.has_exited() {
            return Err(
                "the browser closed as it started. If a Delta browser window is \
                already open, close it and try again"
                    .to_owned(),
            );
        }
        if Instant::now() >= deadline {
            return Err("the browser did not start in time".to_owned());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// On Windows the browser is tied to a job object that kills it when Delta exits, however Delta
/// exits, so no browser window is left behind.
#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    pub struct Job(HANDLE);

    // The handle is only closed, never shared.
    unsafe impl Send for Job {}

    impl Job {
        pub fn bind(child: &Child) -> Option<Job> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return None;
                }
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let set = SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                let assigned = AssignProcessToJobObject(handle, child.as_raw_handle() as HANDLE);
                if set == 0 || assigned == 0 {
                    CloseHandle(handle);
                    return None;
                }
                Some(Job(handle))
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}
