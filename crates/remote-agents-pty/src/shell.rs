//! Cross-platform shell command utilities.

use std::{
    collections::HashSet,
    env::{join_paths, split_paths},
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use tokio::runtime::Handle;

/// Returns the appropriate shell command and argument for the current platform.
///
/// Returns `(shell_program, shell_arg)` where:
/// - Windows: `("cmd", "/C")`
/// - Unix-like: `("sh", "-c")` or `("bash", "-c")` if available
#[must_use]
pub fn get_shell_command() -> (String, &'static str) {
    if cfg!(windows) {
        ("cmd".into(), "/C")
    } else {
        UnixShell::current_shell().get_shell_command()
    }
}

/// Returns the path to an interactive shell for the current platform.
///
/// On Windows, prefers PowerShell if available, falling back to cmd.exe.
/// On Unix, returns the user's configured shell from `$SHELL`.
pub async fn get_interactive_shell() -> PathBuf {
    if cfg!(windows) {
        if let Some(powershell) = resolve_executable_path("powershell.exe").await {
            powershell
        } else {
            PathBuf::from("cmd.exe")
        }
    } else {
        UnixShell::current_shell().path().to_path_buf()
    }
}

/// Resolve an executable by name, falling back to a refreshed PATH if needed.
///
/// The search order is:
/// 1. Explicit paths (absolute or containing a separator).
/// 2. The current process PATH via `which`.
/// 3. A platform-specific refresh of PATH.
pub async fn resolve_executable_path(executable: &str) -> Option<PathBuf> {
    if executable.trim().is_empty() {
        return None;
    }

    let path = Path::new(executable);
    if path.is_absolute() && path.is_file() {
        return Some(path.to_path_buf());
    }

    if let Some(found) = which_async(executable).await {
        return Some(found);
    }

    if refresh_path().await {
        if let Some(found) = which_async(executable).await {
            return Some(found);
        }
    }

    None
}

/// Blocking version of `resolve_executable_path`.
#[must_use]
pub fn resolve_executable_path_blocking(executable: &str) -> Option<PathBuf> {
    block_on(resolve_executable_path(executable))
}

/// Merge two PATH strings into a single, de-duplicated PATH.
#[must_use]
pub fn merge_paths(primary: impl AsRef<OsStr>, secondary: impl AsRef<OsStr>) -> OsString {
    let mut seen = HashSet::<PathBuf>::new();
    let mut merged = Vec::<PathBuf>::new();

    for p in split_paths(primary.as_ref()).chain(split_paths(secondary.as_ref())) {
        if !p.as_os_str().is_empty() && seen.insert(p.clone()) {
            merged.push(p);
        }
    }

    join_paths(merged).unwrap_or_default()
}

async fn refresh_path() -> bool {
    let Some(refreshed) = get_fresh_path().await else {
        return false;
    };
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let refreshed_os = OsString::from(&refreshed);
    let merged = merge_paths(&existing, refreshed_os);
    if merged == existing {
        return false;
    }
    tracing::debug!(?existing, ?refreshed, ?merged, "Refreshed PATH");
    // SAFETY: We're only modifying the current process's environment.
    unsafe {
        std::env::set_var("PATH", &merged);
    }
    true
}

async fn which_async(executable: &str) -> Option<PathBuf> {
    let executable = executable.to_string();
    tokio::task::spawn_blocking(move || which::which(executable))
        .await
        .ok()
        .and_then(Result::ok)
}

fn block_on<F>(future: F) -> F::Output
where
    F: std::future::Future + Send,
    F::Output: Send,
{
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

    if let Ok(handle) = Handle::try_current() {
        // We're inside a Tokio runtime, use a thread to avoid blocking
        std::thread::scope(|s| {
            s.spawn(|| {
                let _guard = handle.enter();
                futures::executor::block_on(future)
            })
            .join()
            .expect("thread panicked")
        })
    } else {
        // No runtime, create one
        RT.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create runtime")
        })
        .block_on(future)
    }
}

/// Unix shell types.
#[derive(Debug, Clone, PartialEq)]
pub enum UnixShell {
    Zsh(PathBuf),
    Bash(PathBuf),
    Sh(PathBuf),
    Other(PathBuf),
}

impl UnixShell {
    /// Get the shell path.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::Zsh(p) | Self::Bash(p) | Self::Sh(p) | Self::Other(p) => p,
        }
    }

    /// Whether this shell supports login mode.
    #[must_use]
    pub const fn login(&self) -> bool {
        matches!(self, Self::Zsh(_) | Self::Bash(_))
    }

    /// Get the config file for this shell.
    #[must_use]
    pub fn config_file(&self) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        let config_file = match self {
            Self::Zsh(_) => Some(home.join(".zshrc")),
            Self::Bash(_) => Some(home.join(".bashrc")),
            Self::Sh(_) | Self::Other(_) => None,
        };
        config_file.filter(|p| p.is_file())
    }

    /// Get the source command for the config file.
    #[must_use]
    pub fn source_command(&self) -> Option<String> {
        if let Some(source_file) = self.config_file() {
            if let Ok(escaped) = shlex::try_quote(source_file.to_string_lossy().as_ref()) {
                return Some(format!("source {escaped}"));
            }
        }
        None
    }

    /// Get the current shell from `$SHELL`.
    #[must_use]
    pub fn current_shell() -> Self {
        if let Ok(shell) = std::env::var("SHELL") {
            if let Some(shell) = Self::from_path(Path::new(&shell)) {
                return shell;
            }
        }
        Self::Sh(PathBuf::from("/bin/sh"))
    }

    /// Create from a path.
    #[must_use]
    pub fn from_path(path: &Path) -> Option<Self> {
        if path.is_absolute() && path.is_file() {
            let path_buf = path.to_path_buf();
            if path.file_name() == Some(OsStr::new("zsh")) {
                Some(Self::Zsh(path_buf))
            } else if path.file_name() == Some(OsStr::new("bash")) {
                Some(Self::Bash(path_buf))
            } else if path.file_name() == Some(OsStr::new("sh")) {
                Some(Self::Sh(path_buf))
            } else {
                Some(Self::Other(path_buf))
            }
        } else {
            None
        }
    }

    /// Get shell command tuple.
    #[must_use]
    pub fn get_shell_command(&self) -> (String, &'static str) {
        (self.path().to_string_lossy().into_owned(), "-c")
    }
}

#[cfg(not(windows))]
async fn get_fresh_path() -> Option<String> {
    use std::{process::Stdio, time::Duration};

    use tokio::process::Command;

    async fn run(shell: &UnixShell) -> Option<String> {
        let mut cmd = Command::new(shell.path());
        if shell.login() {
            cmd.arg("-l");
        }
        if let Some(source_command) = shell.source_command() {
            cmd.arg("-c")
                .arg(format!("{source_command}; printf '%s' \"$PATH\""));
        } else {
            cmd.arg("-c").arg("printf '%s' \"$PATH\"");
        }
        cmd.env("TERM", "dumb")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        const PATH_REFRESH_TIMEOUT: Duration = Duration::from_secs(5);

        let child = cmd.spawn().ok()?;
        let output = match tokio::time::timeout(PATH_REFRESH_TIMEOUT, child.wait_with_output()).await
        {
            Ok(Ok(output)) => output,
            Ok(Err(err)) => {
                tracing::debug!(
                    shell = %shell.path().display(),
                    ?err,
                    "Failed to retrieve PATH from login shell"
                );
                return None;
            }
            Err(_) => {
                tracing::warn!(
                    shell = %shell.path().display(),
                    "Timed out retrieving PATH from login shell"
                );
                return None;
            }
        };

        if !output.status.success() {
            return None;
        }
        let path = String::from_utf8(output.stdout).ok()?.trim().to_string();
        if path.is_empty() {
            None
        } else {
            Some(path)
        }
    }

    let mut paths = Vec::new();

    let current_shell = UnixShell::current_shell();
    if let Some(path) = run(&current_shell).await {
        paths.push(path);
    }

    let shells: Vec<UnixShell> = ["/bin/zsh", "/bin/bash", "/bin/sh"]
        .into_iter()
        .filter_map(|p| UnixShell::from_path(Path::new(p)))
        .collect();

    for shell in shells {
        if shell != current_shell {
            if let Some(path) = run(&shell).await {
                paths.push(path);
            }
        }
    }

    if paths.is_empty() {
        return None;
    }

    paths
        .into_iter()
        .map(OsString::from)
        .reduce(|a, b| merge_paths(&a, &b))
        .map(|merged| merged.to_string_lossy().into_owned())
}

#[cfg(windows)]
async fn get_fresh_path() -> Option<String> {
    tokio::task::spawn_blocking(get_fresh_path_blocking)
        .await
        .ok()
        .flatten()
}

#[cfg(windows)]
fn get_fresh_path_blocking() -> Option<String> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    use winreg::{enums::*, RegKey, HKEY};

    fn expand_env_vars(input: &OsStr) -> OsString {
        use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;

        let wide: Vec<u16> = input.encode_wide().chain(Some(0)).collect();
        unsafe {
            let needed = ExpandEnvironmentStringsW(wide.as_ptr(), std::ptr::null_mut(), 0);
            if needed == 0 {
                return input.to_os_string();
            }
            let mut buf = vec![0u16; needed as usize];
            let written = ExpandEnvironmentStringsW(wide.as_ptr(), buf.as_mut_ptr(), needed);
            if written == 0 {
                return input.to_os_string();
            }
            OsString::from_wide(&buf[..(written as usize).saturating_sub(1)])
        }
    }

    fn read_registry_path(root: HKEY, subkey: &str) -> Option<OsString> {
        let key = RegKey::predef(root)
            .open_subkey_with_flags(subkey, KEY_READ)
            .ok()?;
        key.get_value::<String, _>("Path").ok().map(OsString::from)
    }

    let mut paths: Vec<OsString> = Vec::new();

    if let Some(user_path) = read_registry_path(HKEY_CURRENT_USER, "Environment") {
        paths.push(expand_env_vars(&user_path));
    }

    if let Some(machine_path) = read_registry_path(
        HKEY_LOCAL_MACHINE,
        r"System\CurrentControlSet\Control\Session Manager\Environment",
    ) {
        paths.push(expand_env_vars(&machine_path));
    }

    if paths.is_empty() {
        return None;
    }

    paths
        .into_iter()
        .reduce(|a, b| merge_paths(&a, &b))
        .map(|merged| merged.to_string_lossy().into_owned())
}
