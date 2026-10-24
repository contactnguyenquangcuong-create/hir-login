//! Extensions that other software on a computer put into every profile, and broken leftovers.
//!
//! Programs such as NordVPN or McAfee WebAdvisor register their browser extension for "every
//! Chrome" (HKCU/HKLM `Software\Google\Chrome\Extensions`), and the engine honours that: every
//! profile opened on such a computer got them installed, and team sync carried them to the other
//! machines. On a machine where the extension's folder did not arrive, the browser stopped on an
//! "Error Loading Extension … File path cannot be resolved" box in front of the person.
//!
//! Before a profile opens (its browser is not running), this removes from it:
//! - extensions installed from outside the browser by other software ("external" install
//!   locations), unless they come from this app's own Extensions tab;
//! - extensions whose files are gone (a broken entry).
//! It keeps the Extensions tab's own extensions (loaded with `--load-extension`), the ones the
//! person installed in the browser by hand (from the Web Store, or unpacked), and the browser's
//! built-in components. A removed external one is put on the browser's own "uninstalled by the
//! user" list, so the software that registered it does not install it again.

use anyhow::Result;
use serde_json::Value;
use std::path::Path;

/// Chromium's `ManifestLocation` values that mean "put there by something outside the browser":
/// an external preferences file, the registry, a download named by either.
const EXTERNAL: &[i64] = &[2, 3, 6];
/// Built into the browser: never touched.
const COMPONENT: &[i64] = &[5, 10];
/// Loaded on the command line — this app's Extensions tab.
const COMMAND_LINE: i64 = 8;

/// What one removal was.
#[derive(Debug)]
pub struct Removed {
    pub id: String,
    pub name: String,
    pub why: &'static str,
}

/// Cleans the profile in `udd` (its browser must not be running). Returns what was removed.
pub fn clean(udd: &Path) -> Result<Vec<Removed>> {
    clean_with(udd, &registered_by_software())
}

fn clean_with(udd: &Path, registered: &[String]) -> Result<Vec<Removed>> {
    let default = udd.join("Default");
    let secure_path = default.join("Secure Preferences");
    let prefs_path = default.join("Preferences");
    let mut secure = read_json(&secure_path);
    let mut prefs = read_json(&prefs_path);
    if secure.is_none() && prefs.is_none() {
        return Ok(Vec::new());
    }
    // What other software on this computer has registered is refused before the browser starts,
    // so it is never installed: the engine installs them one by one while the profile runs, and
    // removing them at the next launch still let them run (and travel to the team) meanwhile.
    let mut blocked_new = false;
    if let Some(p) = prefs.as_mut() {
        for id in registered {
            blocked_new |= add_to_list(p, &["extensions", "external_uninstalls"], id);
        }
    }

    let mut removed = Vec::new();
    for doc in [secure.as_ref(), prefs.as_ref()].into_iter().flatten() {
        let Some(settings) = doc.pointer("/extensions/settings").and_then(|v| v.as_object()) else { continue };
        for (id, e) in settings {
            if removed.iter().any(|r: &Removed| &r.id == id) {
                continue;
            }
            if let Some(why) = verdict(&default, e) {
                let name = e.pointer("/manifest/name").and_then(|v| v.as_str()).unwrap_or(id).to_string();
                removed.push(Removed { id: id.clone(), name, why });
            }
        }
    }
    if removed.is_empty() {
        if blocked_new {
            if let Some(p) = &prefs {
                std::fs::write(&prefs_path, serde_json::to_string(p)?)?;
            }
        }
        return Ok(removed);
    }

    for r in &removed {
        for doc in [secure.as_mut(), prefs.as_mut()].into_iter().flatten() {
            drop_key(doc, &["extensions", "settings"], &r.id);
            drop_key(doc, &["protection", "macs", "extensions", "settings"], &r.id);
            drop_key(doc, &["protection", "macs", "extensions", "settings_encrypted_hash"], &r.id);
            drop_key(doc, &["updateclientdata", "apps"], &r.id);
            drop_from_list(doc, &["extensions", "pinned_extensions"], &r.id);
        }
        // "Uninstalled by the user": the browser does not install it again from the registry.
        if let Some(p) = prefs.as_mut() {
            add_to_list(p, &["extensions", "external_uninstalls"], &r.id);
        }
        let _ = std::fs::remove_dir_all(default.join("Extensions").join(&r.id));
        let _ = std::fs::remove_dir_all(default.join("Local Extension Settings").join(&r.id));
    }
    if let Some(s) = &secure {
        std::fs::write(&secure_path, serde_json::to_string(s)?)?;
    }
    if let Some(p) = &prefs {
        std::fs::write(&prefs_path, serde_json::to_string(p)?)?;
    }
    Ok(removed)
}

/// Why an installed extension goes, or None when it stays.
fn verdict(default: &Path, e: &Value) -> Option<&'static str> {
    let location = e.get("location").and_then(|v| v.as_i64());
    match location {
        Some(l) if COMPONENT.contains(&l) || l == COMMAND_LINE => return None,
        Some(l) if EXTERNAL.contains(&l) => return Some("installed by other software on a computer"),
        _ => {}
    }
    // The person's own (Web Store, unpacked) — or a leftover with no install record at all.
    let path = e.get("path").and_then(|v| v.as_str()).unwrap_or("");
    if location.is_none() || path.is_empty() {
        return Some("a leftover with no files");
    }
    let p = Path::new(path);
    let full = if p.is_absolute() { p.to_path_buf() } else { default.join("Extensions").join(p) };
    (!full.exists()).then_some("its files are gone")
}

fn read_json(p: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

fn object_at<'a>(doc: &'a mut Value, path: &[&str]) -> Option<&'a mut serde_json::Map<String, Value>> {
    let mut cur = doc;
    for k in path {
        cur = cur.get_mut(*k)?;
    }
    cur.as_object_mut()
}

fn drop_key(doc: &mut Value, path: &[&str], key: &str) {
    if let Some(o) = object_at(doc, path) {
        o.remove(key);
    }
}

fn drop_from_list(doc: &mut Value, path: &[&str], item: &str) {
    let (last, parent) = path.split_last().unwrap();
    if let Some(o) = object_at(doc, parent) {
        if let Some(Value::Array(a)) = o.get_mut(*last) {
            a.retain(|v| v.as_str() != Some(item));
        }
    }
}

/// The extension ids that other software has registered for "every Chrome" on this computer
/// (`Software\Google\Chrome\Extensions` under HKCU and HKLM, both registry views).
#[cfg(windows)]
fn registered_by_software() -> Vec<String> {
    use std::os::windows::process::CommandExt;
    let mut ids = Vec::new();
    for key in [
        r"HKCU\Software\Google\Chrome\Extensions",
        r"HKLM\Software\Google\Chrome\Extensions",
        r"HKLM\Software\WOW6432Node\Google\Chrome\Extensions",
    ] {
        let Ok(out) = std::process::Command::new("reg")
            .args(["query", key])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .output()
        else {
            continue;
        };
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let id = line.trim().rsplit('\\').next().unwrap_or("");
            if id.len() == 32 && id.chars().all(|c| ('a'..='p').contains(&c)) && !ids.iter().any(|x: &String| x == id) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

#[cfg(not(windows))]
fn registered_by_software() -> Vec<String> {
    Vec::new()
}

/// Adds `item` to the list at `path`; true when it was not there yet.
fn add_to_list(doc: &mut Value, path: &[&str], item: &str) -> bool {
    let mut cur = &mut *doc;
    let (last, parents) = path.split_last().unwrap();
    for k in parents {
        if !cur.get(*k).is_some_and(|v| v.is_object()) {
            if let Some(o) = cur.as_object_mut() {
                o.insert((*k).to_string(), Value::Object(Default::default()));
            }
        }
        cur = cur.get_mut(*k).unwrap();
    }
    let Some(o) = cur.as_object_mut() else { return false };
    let list = o.entry((*last).to_string()).or_insert_with(|| Value::Array(Vec::new()));
    if let Value::Array(a) = list {
        if !a.iter().any(|v| v.as_str() == Some(item)) {
            a.push(Value::String(item.to_string()));
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn removes_external_and_broken_keeps_the_rest() {
        let udd = std::env::temp_dir().join(format!("ext-clean-{}", uuid::Uuid::new_v4()));
        let def = udd.join("Default");
        std::fs::create_dir_all(def.join("Extensions/userown/1.0_0")).unwrap();
        std::fs::create_dir_all(def.join("Extensions/nordvpn/6.3.0_0")).unwrap();
        let secure = json!({
            "extensions": { "settings": {
                "userown": { "location": 1, "path": "userown\\1.0_0", "manifest": { "name": "Mine" } },
                "nordvpn": { "location": 6, "path": "nordvpn\\6.3.0_0", "manifest": { "name": "NordVPN" } },
                "mcafee":  { "active_bit": false, "disable_reasons": [4] },
                "gone":    { "location": 1, "path": "gone\\2.0_0" },
                "tab":     { "location": 8, "path": "C:\\nowhere\\tab-ext" },
                "pdf":     { "location": 5, "path": "C:\\nowhere\\pdf" }
            }},
            "protection": { "macs": { "extensions": {
                "settings": { "userown": "A", "nordvpn": "B", "mcafee": "C", "gone": "D", "tab": "E", "pdf": "F" },
                "settings_encrypted_hash": { "mcafee": "x" }
            }}}
        });
        std::fs::write(def.join("Secure Preferences"), secure.to_string()).unwrap();
        std::fs::write(def.join("Preferences"), json!({ "updateclientdata": { "apps": { "mcafee": {} } } }).to_string()).unwrap();

        let removed = clean_with(&udd, &["fjoaledfpmneenckfbpdfhkmimnjocfa".to_string()]).unwrap();
        let mut ids: Vec<&str> = removed.iter().map(|r| r.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["gone", "mcafee", "nordvpn"]);

        let s: Value = serde_json::from_str(&std::fs::read_to_string(def.join("Secure Preferences")).unwrap()).unwrap();
        let kept: Vec<&String> = s["extensions"]["settings"].as_object().unwrap().keys().collect();
        assert_eq!(kept.len(), 3, "the person's own, the tab's and the built-in stay: {kept:?}");
        assert!(s["protection"]["macs"]["extensions"]["settings"].get("mcafee").is_none());
        assert!(s["protection"]["macs"]["extensions"]["settings"].get("userown").is_some());
        assert!(s["protection"]["macs"]["extensions"]["settings_encrypted_hash"].get("mcafee").is_none());
        let p: Value = serde_json::from_str(&std::fs::read_to_string(def.join("Preferences")).unwrap()).unwrap();
        assert!(p["updateclientdata"]["apps"].get("mcafee").is_none());
        let blocked: Vec<&str> = p["extensions"]["external_uninstalls"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        assert!(blocked.contains(&"nordvpn") && blocked.contains(&"mcafee"));
        assert!(blocked.contains(&"fjoaledfpmneenckfbpdfhkmimnjocfa"), "registered by software, refused before it is installed");
        assert!(!def.join("Extensions/nordvpn").exists() && def.join("Extensions/userown").exists());

        assert!(clean_with(&udd, &[]).unwrap().is_empty(), "a second pass finds nothing");
        let _ = std::fs::remove_dir_all(&udd);
    }
}
