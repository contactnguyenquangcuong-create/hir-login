//! One-click "allow this app through the Windows firewall".
//!
//! The team server listens on every interface, so the first time it starts
//! Windows asks whether to let it through — and if that box is dismissed other
//! machines silently can't reach it. A *program* rule covers every port the app
//! (or the browser engine) will ever use, so there is nothing to open one by one.
//!
//! Adding a rule needs administrator rights; the installer is per-user, so the
//! rule is added on demand through a single UAC prompt instead.  Only Windows
//! has this firewall — elsewhere every call is a no-op.

use anyhow::Result;
use std::path::PathBuf;

#[cfg_attr(not(windows), allow(dead_code))]
const APP_RULE: &str = "Hir-Login";
#[cfg_attr(not(windows), allow(dead_code))]
const ENGINE_RULE: &str = "Hir-Login browser engine";

#[derive(Debug, Clone, serde::Serialize)]
pub struct FirewallStatus {
    /// False off Windows: nothing to do there.
    pub supported: bool,
    /// Every program rule is already present.
    pub granted: bool,
}

/// The programs that get a rule: this app, and the browser engine when present.
#[cfg_attr(not(windows), allow(dead_code))]
fn programs() -> Vec<(&'static str, PathBuf)> {
    let mut v = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        v.push((APP_RULE, exe));
    }
    if let Ok(engine) = crate::launch::resolve_binary() {
        if engine.exists() {
            v.push((ENGINE_RULE, engine));
        }
    }
    v
}

/// The PowerShell the elevated child runs: replace each rule, then report netsh's
/// exit code.  Paths and names go in single quotes with `'` doubled, so nothing
/// in a folder name can end the string.
#[cfg_attr(not(windows), allow(dead_code))]
fn elevated_script(rules: &[(&str, PathBuf)]) -> String {
    let q = |s: &str| format!("'{}'", s.replace('\'', "''"));
    let mut out = String::from("$ErrorActionPreference='Stop'\n$code=0\n");
    for (name, path) in rules {
        let (n, p) = (q(name), q(&path.display().to_string()));
        out.push_str(&format!(
            "netsh advfirewall firewall delete rule name={n} | Out-Null\n\
             netsh advfirewall firewall add rule name={n} dir=in action=allow program={p} enable=yes profile=any | Out-Null\n\
             if ($LASTEXITCODE -ne 0) {{ $code=$LASTEXITCODE }}\n"
        ));
    }
    out.push_str("exit $code\n");
    out
}

/// UTF-16LE + base64, which is what `powershell -EncodedCommand` takes — it keeps
/// the script clear of every quoting rule on the way through the outer shell.
#[cfg_attr(not(windows), allow(dead_code))]
fn encode_command(script: &str) -> String {
    use base64::Engine as _;
    let bytes: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(windows)]
fn run_hidden(program: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new(program).args(args).creation_flags(0x08000000).output()
}

#[cfg(windows)]
pub fn status() -> FirewallStatus {
    // `show rule` needs no elevation; it exits non-zero when nothing matches.
    let has = |name: &str, path: &std::path::Path| {
        run_hidden("netsh", &["advfirewall", "firewall", "show", "rule", &format!("name={name}"), "verbose"])
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains(&path.display().to_string().to_lowercase()))
            .unwrap_or(false)
    };
    let list = programs();
    FirewallStatus { supported: true, granted: !list.is_empty() && list.iter().all(|(n, p)| has(n, p)) }
}

#[cfg(not(windows))]
pub fn status() -> FirewallStatus {
    FirewallStatus { supported: false, granted: true }
}

/// Adds (or refreshes) the rules; Windows shows one UAC prompt.  Declining it is
/// an error the caller shows, not a silent success.
#[cfg(windows)]
pub fn grant() -> Result<()> {
    let list = programs();
    anyhow::ensure!(!list.is_empty(), "không tìm thấy chương trình để cấp quyền");
    let enc = encode_command(&elevated_script(&list));
    // Outer, hidden: start the elevated copy and hand back its exit code.  The
    // base64 text holds only [A-Za-z0-9+/=], so single quotes are enough.
    let outer = format!(
        "try {{ $p = Start-Process powershell -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand','{enc}'; exit $p.ExitCode }} catch {{ exit 1223 }}"
    );
    let out = run_hidden("powershell", &["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &outer])?;
    match out.status.code() {
        Some(0) => Ok(()),
        Some(1223) => anyhow::bail!("Bạn đã từ chối hộp xin quyền quản trị — chưa cấp được quyền mạng"),
        c => anyhow::bail!("netsh trả mã lỗi {c:?}"),
    }
}

#[cfg(not(windows))]
pub fn grant() -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_replaces_each_rule_and_quotes_awkward_paths() {
        let s = elevated_script(&[
            ("Hir-Login", PathBuf::from(r"C:\Users\O'Neil\App Data\Hir-Login.exe")),
            ("Hir-Login browser engine", PathBuf::from(r"C:\x\chrome.exe")),
        ]);
        assert_eq!(s.matches("delete rule").count(), 2);
        assert_eq!(s.matches("add rule").count(), 2);
        assert!(s.contains(r"program='C:\Users\O''Neil\App Data\Hir-Login.exe'"), "{s}");
        assert!(s.contains("dir=in action=allow") && s.contains("profile=any"));
        assert!(s.trim_end().ends_with("exit $code"));
    }

    #[test]
    fn the_encoded_command_round_trips_as_utf16() {
        use base64::Engine as _;
        let enc = encode_command("exit 0 # ví dụ");
        assert!(enc.chars().all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c)));
        let raw = base64::engine::general_purpose::STANDARD.decode(enc).unwrap();
        let units: Vec<u16> = raw.chunks(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        assert_eq!(String::from_utf16(&units).unwrap(), "exit 0 # ví dụ");
    }

    #[test]
    #[cfg(not(windows))]
    fn elsewhere_there_is_nothing_to_grant() {
        let s = status();
        assert!(!s.supported && s.granted);
        assert!(grant().is_ok());
    }
}
