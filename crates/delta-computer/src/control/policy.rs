//! Consent policy for desktop input control (ADR-0062). Every decision here is pure: which tier a
//! program falls in, whether one step is allowed, whether a screenshot may be taken, and when a run
//! must stop. No operating-system call is made, so these rules are tested on every platform.
//!
//! The lists below are the first version, kept in code and versioned with the release. They are
//! not exhaustive, and a window title can be chosen by the program that shows it, so title matching
//! is a second line of defence behind the process name.

use std::collections::BTreeSet;

/// Most actions one run may take (ADR-0062 §6).
pub const MAX_ACTIONS_PER_RUN: u32 = 50;
/// The same action failing this many times in a row stops the run (ADR-0062 §6).
pub const MAX_REPEATED_FAILURES: u32 = 3;

/// Programs that are never operated, whatever the user approves (ADR-0062 §3). Matched by file name.
const REFUSED_PROCESSES: [&str; 11] = [
    "systemsettings.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "windowsterminal.exe",
    "wt.exe",
    "regedit.exe",
    "wscript.exe",
    "cscript.exe",
    "mshta.exe",
    "sechealthui.exe",
];

/// Password managers, by file name: each action needs the user's confirmation, and the model never
/// reads their screen (ADR-0062 §3).
const PER_ACTION_PROCESSES: [&str; 6] = [
    "1password.exe",
    "keepass.exe",
    "keepassxc.exe",
    "bitwarden.exe",
    "lastpass.exe",
    "dashlane.exe",
];

/// Window-title words that mark a banking or payment window, whatever program shows it. The title is
/// chosen by the program, so this only adds to the process-name list above.
const PER_ACTION_TITLE_WORDS: [&str; 8] = [
    "银行",
    "网银",
    "支付宝",
    "微信支付",
    "bank",
    "paypal",
    "payment",
    "wallet",
];

/// Words in a control's name or text that mark an action as sensitive: sending, submitting,
/// deleting, paying. Each such click needs confirmation even in an authorized program (ADR-0062 §3).
const SENSITIVE_WORDS: [&str; 22] = [
    "发送",
    "提交",
    "发布",
    "删除",
    "清空",
    "覆盖",
    "购买",
    "付款",
    "支付",
    "转账",
    "确认订单",
    "确认付款",
    "send",
    "submit",
    "publish",
    "delete",
    "remove",
    "overwrite",
    "empty",
    "purchase",
    "pay",
    "transfer",
];

/// Keyboard shortcuts that submit or destroy. Pressing one needs confirmation.
const SENSITIVE_KEYS: [&str; 4] = ["enter", "ctrl+enter", "delete", "shift+delete"];

/// The file name of a program's executable, lowercased. Matching uses this, not the full path.
fn file_name(process: &str) -> String {
    process
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(process)
        .to_ascii_lowercase()
}

/// A program the action would act in: the file name of its executable, and its window title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    /// The executable's file name, lowercased (for example `notepad.exe`).
    pub process: String,
    pub title: String,
}

impl Program {
    /// Build a program from the executable path or name the operating system reports.
    pub fn new(process: &str, title: &str) -> Self {
        Self {
            process: file_name(process),
            title: title.to_owned(),
        }
    }
}

/// How a program is treated, from the most restricted to the least.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Never operated.
    Refused,
    /// Banking or password manager: each action confirmed, and the model does not read the screen.
    PerAction,
    /// Ordinary program: operated only after the user authorizes it for this session.
    Ordinary,
}

/// The tier of a program, from its file name and window title.
pub fn tier_of(program: &Program) -> Tier {
    if REFUSED_PROCESSES.contains(&program.process.as_str()) {
        return Tier::Refused;
    }
    let title = program.title.to_lowercase();
    if PER_ACTION_PROCESSES.contains(&program.process.as_str())
        || PER_ACTION_TITLE_WORDS
            .iter()
            .any(|word| title.contains(word))
    {
        return Tier::PerAction;
    }
    Tier::Ordinary
}

/// A UI Automation control the action targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    /// The control's accessible name, as the program reports it.
    pub name: String,
    /// The control type, for example `Button` or `Edit`.
    pub kind: String,
    /// Whether the control masks its text (a password field).
    pub is_password: bool,
}

/// One step the model asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Invoke a named control.
    Click { control: Control },
    /// Click at screen coordinates: a fallback when no control is named. Always confirmed.
    Coordinates { x: i32, y: i32 },
    /// Enter text into a control.
    Type { control: Control, text: String },
    /// A keyboard shortcut such as `ctrl+s` or `enter`.
    Key { combo: String },
    /// Scroll the focused window.
    Scroll,
}

impl Action {
    /// A short identity used to tell repeated failures of the same step apart from different ones.
    pub fn key(&self) -> String {
        match self {
            Action::Click { control } => format!("click:{}:{}", control.kind, control.name),
            Action::Coordinates { x, y } => format!("coords:{x},{y}"),
            Action::Type { control, .. } => format!("type:{}:{}", control.kind, control.name),
            Action::Key { combo } => format!("key:{combo}"),
            Action::Scroll => "scroll".to_owned(),
        }
    }
}

/// What the runtime does with one step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Not allowed. The reason is shown to the user and to the model.
    Refuse(String),
    /// Allowed only after the user confirms this step.
    Confirm(String),
    /// The program must first be authorized for this session.
    NeedsGrant,
    /// Allowed without asking.
    Allow,
}

/// Programs the user has authorized for the current session. Never persisted (ADR-0062 §3).
#[derive(Debug, Default, Clone)]
pub struct SessionGrants {
    granted: BTreeSet<String>,
}

impl SessionGrants {
    pub fn grant(&mut self, program: &Program) {
        self.granted.insert(program.process.clone());
    }

    /// Withdraw one program's authorization. The user can do this at any time.
    pub fn revoke(&mut self, program: &Program) {
        self.granted.remove(&program.process);
    }

    /// The session ended: everything is withdrawn.
    pub fn clear(&mut self) {
        self.granted.clear();
    }

    pub fn is_granted(&self, program: &Program) -> bool {
        self.granted.contains(&program.process)
    }
}

/// Decide one step, given the program it would act in and the session's grants.
pub fn decide(program: &Program, action: &Action, grants: &SessionGrants) -> Decision {
    let tier = tier_of(program);
    if tier == Tier::Refused {
        return Decision::Refuse(format!("{} 不允许 Delta 操作", program.process));
    }
    if let Action::Type { control, .. } = action {
        if control.is_password {
            return Decision::Refuse("密码字段需要你自己输入".to_owned());
        }
    }
    if tier == Tier::PerAction {
        return Decision::Confirm("银行或密码类程序：每个动作都需要你确认".to_owned());
    }
    match action {
        Action::Coordinates { .. } => {
            return Decision::Confirm("坐标点击每次都需要你确认".to_owned());
        }
        Action::Key { combo } if SENSITIVE_KEYS.contains(&combo.to_ascii_lowercase().as_str()) => {
            return Decision::Confirm(format!("按键 {combo} 可能提交或删除内容，需要你确认"));
        }
        Action::Click { control } if is_sensitive(&control.name) => {
            return Decision::Confirm(format!("“{}” 可能提交或删除内容，需要你确认", control.name));
        }
        _ => {}
    }
    if !grants.is_granted(program) {
        return Decision::NeedsGrant;
    }
    Decision::Allow
}

/// Whether a screenshot may be taken with these programs visible. Banking and password-manager
/// windows on any display stop it, because the model would otherwise read their content
/// (ADR-0062 §3).
pub fn capture_allowed(visible: &[Program]) -> bool {
    !visible
        .iter()
        .any(|program| tier_of(program) == Tier::PerAction)
}

/// Whether a control's name or text names a sensitive action.
pub fn is_sensitive(text: &str) -> bool {
    let lowered = text.to_lowercase();
    SENSITIVE_WORDS.iter().any(|word| lowered.contains(word))
}

/// Why a run must stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// The run used its full action allowance.
    ActionLimit,
    /// The same action failed too many times in a row.
    RepeatedFailures { action: String },
}

/// Counts the actions of one run and stops it at the limits (ADR-0062 §6).
#[derive(Debug, Default)]
pub struct RunBudget {
    actions: u32,
    last_key: Option<String>,
    streak: u32,
}

impl RunBudget {
    /// Call before each action. Fails once the allowance is spent.
    pub fn begin_action(&mut self) -> Result<(), Stop> {
        if self.actions >= MAX_ACTIONS_PER_RUN {
            return Err(Stop::ActionLimit);
        }
        self.actions += 1;
        Ok(())
    }

    /// Call after each action with its outcome. A success clears the streak; a failure of the same
    /// action extends it, and a failure of a different action starts a new one.
    pub fn record(&mut self, action: &Action, is_success: bool) -> Result<(), Stop> {
        let key = action.key();
        if is_success {
            self.streak = 0;
            self.last_key = None;
            return Ok(());
        }
        if self.last_key.as_deref() == Some(key.as_str()) {
            self.streak += 1;
        } else {
            self.last_key = Some(key.clone());
            self.streak = 1;
        }
        if self.streak >= MAX_REPEATED_FAILURES {
            return Err(Stop::RepeatedFailures { action: key });
        }
        Ok(())
    }
}
