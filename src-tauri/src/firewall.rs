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

/// The PowerShell the elevated child runs: remove every existing inbound rule for each program
/// (including Windows-auto-created Block rules under any name such as `hir-login.exe`, which
/// otherwise override an Allow rule) and every rule under our own name, then add one clean Allow
/// rule covering Domain, Private, and Public (Tailscale's virtual adapter defaults to Public).
/// Uses native NetSecurity cmdlets first (immune to `netsh` quoting quirks with spaces/Unicode)
/// and falls back to `netsh` if WMI/NetSecurity is unavailable.
#[cfg_attr(not(windows), allow(dead_code))]
fn elevated_script(rules: &[(&str, PathBuf)]) -> String {
    let q = |s: &str| format!("'{}'", s.replace('\'', "''"));
    let mut out = String::from("$ErrorActionPreference='Continue'\n$code=0\n");
    for (name, path) in rules {
        let (n, p) = (q(name), q(&path.display().to_string()));
        out.push_str(&format!(
            "$n={n}; $p={p}\n\
             try {{\n\
               Get-NetFirewallApplicationFilter -ErrorAction Stop | Where-Object {{\n\
                 $_.Program -and (($_.Program -ieq $p) -or ([Environment]::ExpandEnvironmentVariables($_.Program) -ieq $p))\n\
               }} | ForEach-Object {{\n\
                 foreach ($r in @(Get-NetFirewallRule -AssociatedNetFirewallApplicationFilter $_ -ErrorAction SilentlyContinue)) {{\n\
                   if ($r -and $r.Direction -eq 'Inbound') {{ Remove-NetFirewallRule -InputObject $r -ErrorAction SilentlyContinue }}\n\
                 }}\n\
               }}\n\
               Remove-NetFirewallRule -DisplayName $n -ErrorAction SilentlyContinue\n\
             }} catch {{}}\n\
             netsh advfirewall firewall delete rule name=all dir=in program=$p | Out-Null\n\
             netsh advfirewall firewall delete rule name=$n | Out-Null\n\
             $added=$false\n\
             try {{\n\
               New-NetFirewallRule -DisplayName $n -Direction Inbound -Action Allow -Program $p -Enabled True -Profile Any -ErrorAction Stop | Out-Null\n\
               $added=$true\n\
             }} catch {{}}\n\
             if (-not $added) {{\n\
               netsh advfirewall firewall add rule name=$n dir=in action=allow program=$p enable=yes profile=any | Out-Null\n\
               if ($LASTEXITCODE -ne 0) {{ $code=$LASTEXITCODE }}\n\
             }}\n"
        ));
    }
    out.push_str("exit $code\n");
    out
}

/// Non-elevated PowerShell check: for every program, require an enabled Inbound Allow rule that
/// covers Public and Private (or Any), AND zero enabled Inbound Block rules under *any* display
/// name (Windows creates Block rules when a user clicks Cancel or unchecks Public on the Security
/// Alert popup, and an explicit Block rule always overrides an Allow rule).
/// Exit codes: 0 = granted, 2 = missing/incomplete/blocked, 3 = NetSecurity query unavailable.
#[cfg_attr(not(windows), allow(dead_code))]
fn status_script(rules: &[(&str, PathBuf)]) -> String {
    let q = |s: &str| format!("'{}'", s.replace('\'', "''"));
    let mut out = String::from(
        "$ErrorActionPreference='Stop'\n\
         try {\n\
           $filters = @(Get-NetFirewallApplicationFilter)\n\
         } catch { exit 3 }\n\
         $ok = $true\n",
    );
    for (_, path) in rules {
        let p = q(&path.display().to_string());
        out.push_str(&format!(
            "$p={p}; $hasAllow=$false; $hasBlock=$false\n\
             foreach ($f in $filters) {{\n\
               if ($f.Program -and (($f.Program -ieq $p) -or ([Environment]::ExpandEnvironmentVariables($f.Program) -ieq $p))) {{\n\
                 foreach ($r in @(Get-NetFirewallRule -AssociatedNetFirewallApplicationFilter $f -ErrorAction SilentlyContinue)) {{\n\
                   if ($r -and ($r.Enabled -eq 'True') -and ($r.Direction -eq 'Inbound')) {{\n\
                     if ($r.Action -eq 'Block') {{ $hasBlock=$true }}\n\
                     elseif ($r.Action -eq 'Allow') {{\n\
                       $prof = \"$($r.Profile)\"\n\
                       if (($prof -eq 'Any') -or (($prof -match 'Private') -and ($prof -match 'Public'))) {{ $hasAllow=$true }}\n\
                     }}\n\
                   }}\n\
                 }}\n\
               }}\n\
             }}\n\
             if ($hasBlock -or (-not $hasAllow)) {{ $ok=$false }}\n"
        ));
    }
    out.push_str("if ($ok) { exit 0 } else { exit 2 }\n");
    out
}

/// Fallback parser for `netsh advfirewall firewall show rule name=<n> verbose` when PowerShell's
/// CIM/NetSecurity query isn't available. Unlike a raw substring check on the path, this refuses
/// rules that are Block, disabled, or missing Public/Private coverage.
#[cfg_attr(not(windows), allow(dead_code))]
fn netsh_output_grants_path(stdout: &str, path: &str) -> bool {
    let needle = path.to_lowercase();
    let mut has_allow = false;
    let mut block_buf: Vec<&str> = Vec::new();
    let mut check_block = |lines: &[&str]| -> bool {
        if lines.is_empty() {
            return true;
        }
        let joined = lines.join("\n").to_lowercase();
        if !joined.contains(&needle) {
            return true;
        }
        let mut enabled = true;
        let mut is_block = false;
        let mut is_allow = false;
        let mut covers_all = false;
        for line in lines {
            let Some((k, v)) = line.split_once(':') else { continue };
            let (k, v) = (k.trim().to_lowercase(), v.trim().to_lowercase());
            if k == "enabled" {
                enabled = v == "yes" || v == "true";
            } else if k == "action" {
                if v == "block" {
                    is_block = true;
                } else if v == "allow" {
                    is_allow = true;
                }
            } else if k == "profiles" {
                covers_all = v == "any" || (v.contains("private") && v.contains("public"));
            }
        }
        if enabled && is_block {
            return false;
        }
        if enabled && is_allow && covers_all {
            has_allow = true;
        }
        true
    };
    for line in stdout.lines() {
        if line.trim().is_empty() {
            if !check_block(&block_buf) {
                return false;
            }
            block_buf.clear();
        } else {
            block_buf.push(line);
        }
    }
    if !check_block(&block_buf) {
        return false;
    }
    has_allow
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
    let list = programs();
    if list.is_empty() {
        return FirewallStatus { supported: true, granted: false };
    }
    let enc = encode_command(&status_script(&list));
    if let Ok(out) = run_hidden(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-EncodedCommand", &enc],
    ) {
        match out.status.code() {
            Some(0) => return FirewallStatus { supported: true, granted: true },
            Some(2) => return FirewallStatus { supported: true, granted: false },
            _ => {} // 3 or unexpected failure: fall back to netsh below
        }
    }
    // Fallback when CIM/NetSecurity isn't reachable: check via netsh, rejecting Block / Private-only rules.
    let has = |name: &str, path: &std::path::Path| {
        let path_str = path.display().to_string();
        run_hidden("netsh", &["advfirewall", "firewall", "show", "rule", &format!("name={name}"), "verbose"])
            .ok()
            .filter(|o| o.status.success())
            .map(|o| netsh_output_grants_path(&String::from_utf8_lossy(&o.stdout), &path_str))
            .unwrap_or(false)
    };
    FirewallStatus { supported: true, granted: list.iter().all(|(n, p)| has(n, p)) }
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
        assert_eq!(s.matches("delete rule name=all dir=in").count(), 2, "must purge inbound Block/Allow rules under any name");
        assert_eq!(s.matches("delete rule name=$n").count(), 2);
        assert_eq!(s.matches("New-NetFirewallRule").count(), 2);
        assert_eq!(s.matches("add rule name=$n").count(), 2, "netsh fallback must stay present");
        assert!(s.contains(r"$p='C:\Users\O''Neil\App Data\Hir-Login.exe'"), "{s}");
        assert!(s.contains("-Direction Inbound -Action Allow -Program $p -Enabled True -Profile Any"), "{s}");
        assert!(s.contains("dir=in action=allow") && s.contains("profile=any"));
        assert!(s.trim_end().ends_with("exit $code"));
    }

    #[test]
    fn status_script_checks_for_block_rules_and_all_profiles() {
        let s = status_script(&[("Hir-Login", PathBuf::from(r"C:\Users\A'B\Hir-Login.exe"))]);
        assert!(s.contains(r"$p='C:\Users\A''B\Hir-Login.exe'"), "{s}");
        assert!(s.contains("$r.Action -eq 'Block'"), "{s}");
        assert!(s.contains("$prof -match 'Public'"), "{s}");
        assert!(s.contains("exit 3"), "must signal CIM unavailability for netsh fallback");
    }

    #[test]
    fn netsh_parser_rejects_block_and_private_only_rules() {
        let exe = r"C:\Users\admin\AppData\Local\Hir-Login\Hir-Login.exe";
        let block_only = format!(
            "Rule Name: Hir-Login\nEnabled: Yes\nDirection: In\nProfiles: Public\nProgram: {exe}\nAction: Block\n"
        );
        assert!(!netsh_output_grants_path(&block_only, exe), "a Block rule must never count as granted");

        let private_only = format!(
            "Rule Name: Hir-Login\nEnabled: Yes\nDirection: In\nProfiles: Private\nProgram: {exe}\nAction: Allow\n"
        );
        assert!(!netsh_output_grants_path(&private_only, exe), "Private-only misses Tailscale's Public profile");

        let allow_plus_block = format!(
            "Rule Name: Hir-Login\nEnabled: Yes\nDirection: In\nProfiles: Domain,Private,Public\nProgram: {exe}\nAction: Allow\n\n\
             Rule Name: Hir-Login\nEnabled: Yes\nDirection: In\nProfiles: Public\nProgram: {exe}\nAction: Block\n"
        );
        assert!(!netsh_output_grants_path(&allow_plus_block, exe), "explicit Block overrides Allow on Windows");

        let clean_allow = format!(
            "Rule Name: Hir-Login\nEnabled: Yes\nDirection: In\nProfiles: Domain,Private,Public\nProgram: {exe}\nAction: Allow\n"
        );
        assert!(netsh_output_grants_path(&clean_allow, exe));

        let any_allow = format!(
            "Rule Name: Hir-Login\nEnabled: Yes\nDirection: In\nProfiles: Any\nProgram: {exe}\nAction: Allow\n"
        );
        assert!(netsh_output_grants_path(&any_allow, exe));
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
