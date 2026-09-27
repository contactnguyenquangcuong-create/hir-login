use anyhow::Result;
use std::path::PathBuf;

fn app_exe_path() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

#[cfg(target_os = "macos")]
fn launch_agent_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join("Library/LaunchAgents/com.hirlogin.app.plist"))
}

#[cfg(target_os = "macos")]
pub fn is_enabled() -> bool {
    launch_agent_path().map(|p| p.exists()).unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
pub fn is_enabled() -> bool {
    // Windows: check Startup shortcut or Registry
    #[cfg(target_os = "windows")]
    {
        if let Some(p) = startup_shortcut_path() {
            if p.exists() { return true; }
        }
        // also check registry Run key
        use std::os::windows::process::CommandExt;
        if let Ok(out) = std::process::Command::new("reg")
            .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "Hir-Login"])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW — no console flash
            .output()
        {
            if out.status.success() { return true; }
        }
        return false;
    }
    #[allow(unreachable_code)]
    false
}

#[cfg(target_os = "macos")]
pub fn set_enabled(enabled: bool) -> Result<()> {
    let plist = launch_agent_path().ok_or_else(|| anyhow::anyhow!("no home dir"))?;
    if !enabled {
        if plist.exists() {
            std::fs::remove_file(&plist)?;
            let _ = std::process::Command::new("launchctl").args(["unload", &plist.to_string_lossy().to_string()]).output();
        }
        return Ok(());
    }
    let exe = app_exe_path().ok_or_else(|| anyhow::anyhow!("cannot find exe"))?;
    // For a bundled .app the exe is .../Hir-Login.app/Contents/MacOS/Hir-Login
    // LaunchAgent should open the .app bundle, not the raw binary.
    let bundle = exe.ancestors().find(|p| p.extension().map(|e| e == "app").unwrap_or(false));
    let program_arg = if let Some(b) = bundle {
        format!("open -a \"{}\" --args --minimized", b.display())
    } else {
        format!("\"{}\" --minimized", exe.display())
    };
    // Use open -a for .app so macOS brings up the bundle properly; LaunchAgent runs `sh -c "open -a ..."`
    let label = "com.hirlogin.app";
    let content = if bundle.is_some() {
        format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>{label}</string>
  <key>ProgramArguments</key><array><string>/bin/sh</string><string>-c</string><string>{program_arg}</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><false/>
</dict></plist>
"#)
    } else {
        format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>{label}</string>
  <key>ProgramArguments</key><array><string>{}</string><string>--minimized</string></array>
  <key>RunAtLoad</key><true/>
</dict></plist>
"#, exe.display())
    };
    if let Some(parent) = plist.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(&plist, content)?;
    let _ = std::process::Command::new("launchctl").args(["load", &plist.to_string_lossy().to_string()]).output();
    Ok(())
}

#[cfg(target_os = "windows")]
fn startup_shortcut_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(r"AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\Hir-Login.lnk"))
}

#[cfg(target_os = "windows")]
pub fn set_enabled(enabled: bool) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x08000000; // CREATE_NO_WINDOW — no console flash
    // Use registry Run key — simplest and most reliable on Windows
    if enabled {
        let exe = app_exe_path().ok_or_else(|| anyhow::anyhow!("no exe"))?;
        let val = format!("\"{}\" --minimized", exe.display());
        let out = std::process::Command::new("reg")
            .args(["add", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "Hir-Login", "/t", "REG_SZ", "/d", &val, "/f"])
            .creation_flags(NO_WINDOW)
            .output()?;
        if !out.status.success() {
            anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr));
        }
    } else {
        let _ = std::process::Command::new("reg")
            .args(["delete", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "Hir-Login", "/f"])
            .creation_flags(NO_WINDOW)
            .output();
        if let Some(p) = startup_shortcut_path() { let _ = std::fs::remove_file(p); }
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn set_enabled(_enabled: bool) -> Result<()> {
    anyhow::bail!("autostart not supported on this OS")
}
