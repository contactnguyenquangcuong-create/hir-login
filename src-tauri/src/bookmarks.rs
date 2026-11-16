//! Folder-scoped bookmarks, written into the profile's `Bookmarks` just before
//! launch, inside a folder tagged as ours so the operator's own are untouched.

use crate::store;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Marker on the managed folder node. Anything carrying it is ours to replace.
const MANAGED_KEY: &str = "shardx_managed";
/// What the managed folder is called inside the browser.
const MANAGED_FOLDER: &str = "Hir-Login";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    #[serde(default)]
    pub id: String,
    pub title: String,
    pub url: String,
    /// Launcher folder this belongs to; empty means every profile.
    #[serde(default)]
    pub folder: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct BookmarkStore {
    #[serde(default)]
    bookmarks: Vec<Bookmark>,
    /// When the list last changed (unix ms). A team shares one list through its
    /// server; the newer copy wins.
    #[serde(default)]
    updated_ms: u64,
}

/// None when this machine has never had a list — not the same as an empty one.
/// A team member's machine that never made or received a list must leave the
/// managed folder in a profile alone: stripping it there went up with the
/// profile and took the team's bookmarks away from everyone.
fn load_store_opt() -> Result<Option<BookmarkStore>> {
    let path = store::bookmarks_path()?;
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&fs::read_to_string(path)?).unwrap_or_default()))
}

fn load_store() -> Result<BookmarkStore> {
    Ok(load_store_opt()?.unwrap_or_default())
}

fn write_store(s: &BookmarkStore) -> Result<()> {
    let path = store::bookmarks_path()?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(s)?)?;
    crate::winfs::rename_replace(&tmp, &path)?;
    Ok(())
}

/// A change made here: stamped, written, and offered to the team.
fn save_store(s: &mut BookmarkStore) -> Result<()> {
    s.updated_ms = now_ms().max(s.updated_ms + 1);
    write_store(s)?;
    crate::cloud_sync::kick();
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn list() -> Result<Vec<Bookmark>> {
    Ok(load_store()?.bookmarks)
}

fn clean(mut b: Bookmark) -> Result<Bookmark> {
    b.title = b.title.trim().to_string();
    b.url = normalise_url(&b.url);
    b.folder = b.folder.trim().to_string();
    if b.url.is_empty() {
        anyhow::bail!("a bookmark needs a URL");
    }
    if b.title.is_empty() {
        b.title = b.url.clone();
    }
    Ok(b)
}

/// Insert or update by id; a blank id mints one.
pub fn save(b: Bookmark) -> Result<Bookmark> {
    let mut b = clean(b)?;
    let mut s = load_store()?;
    if b.id.is_empty() {
        b.id = uuid::Uuid::new_v4().to_string();
        s.bookmarks.push(b.clone());
    } else if let Some(slot) = s.bookmarks.iter_mut().find(|x| x.id == b.id) {
        *slot = b.clone();
    } else {
        s.bookmarks.push(b.clone());
    }
    save_store(&mut s)?;
    Ok(b)
}

pub fn delete(id: &str) -> Result<()> {
    let mut s = load_store()?;
    s.bookmarks.retain(|b| b.id != id);
    save_store(&mut s)
}

/// Adds every link of a browser's "Export bookmarks" file (Netscape HTML) to
/// `folder`, skipping ones already there. Returns (added, skipped).
pub fn import_html(html: &str, folder: &str) -> Result<(usize, usize)> {
    let folder = folder.trim().to_string();
    let found = parse_netscape(html);
    if found.is_empty() {
        anyhow::bail!("no bookmarks found in that file");
    }
    let mut s = load_store()?;
    let (mut added, mut skipped) = (0, 0);
    for (url, title) in found {
        let Ok(mut b) = clean(Bookmark { id: String::new(), title, url, folder: folder.clone() }) else {
            skipped += 1;
            continue;
        };
        if s.bookmarks.iter().any(|x| x.folder == b.folder && x.url == b.url) {
            skipped += 1;
            continue;
        }
        b.id = uuid::Uuid::new_v4().to_string();
        s.bookmarks.push(b);
        added += 1;
    }
    if added > 0 {
        save_store(&mut s)?;
    }
    Ok((added, skipped))
}

/// (url, title) of every `<A HREF=…>` in a Netscape bookmark file. Only web links:
/// `javascript:` and `data:` entries are left out.
fn parse_netscape(html: &str) -> Vec<(String, String)> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find("<a ") {
        let start = at + i;
        let Some(tag_end) = lower[start..].find('>').map(|e| start + e) else { break };
        let Some(close) = lower[tag_end..].find("</a>").map(|e| tag_end + e) else { break };
        let tag = &html[start..tag_end];
        let href = unescape(&attr(tag, "href").unwrap_or_default());
        let title = unescape(html[tag_end + 1..close].trim());
        let web = ["http://", "https://", "file://", "chrome://"].iter().any(|p| href.to_ascii_lowercase().starts_with(p));
        if web {
            out.push((href, title));
        }
        at = close + 4;
    }
    out
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let key = format!("{name}=");
    let mut from = 0;
    while let Some(i) = lower[from..].find(&key) {
        let pos = from + i;
        // A whole attribute name, not the tail of a longer one.
        if lower[..pos].ends_with(|c: char| c.is_ascii_whitespace()) {
            let rest = &tag[pos + key.len()..];
            let quote = rest.chars().next()?;
            if quote == '"' || quote == '\'' {
                let body = &rest[1..];
                return body.find(quote).map(|e| body[..e].to_string());
            }
            return Some(rest.split_whitespace().next().unwrap_or("").to_string());
        }
        from = pos + key.len();
    }
    None
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

// ---- the team's copy ----

fn same_list(a: &[Bookmark], b: &[Bookmark]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| x.id == y.id && x.title == y.title && x.url == y.url && x.folder == y.folder)
}

/// What a remote copy means for this machine.
#[derive(Debug, PartialEq, Eq)]
pub enum RemoteOutcome {
    /// The remote was newer (or this machine may not keep its own) and is now the list here.
    Taken,
    /// This machine's list is newer: it should go up.
    Push,
    /// Both are the same.
    Same,
}

/// This machine's list as the team stores it; None when it never had one.
pub fn team_copy() -> Result<Option<Vec<u8>>> {
    Ok(match load_store_opt()? {
        Some(s) => Some(serde_json::to_vec(&s)?),
        None => None,
    })
}

/// Compares the team's copy with this machine's and takes it when it is newer —
/// or whenever they differ, with `authoritative` (this machine may not change the team's list).
pub fn take_remote(bytes: &[u8], authoritative: bool) -> Result<RemoteOutcome> {
    let remote: BookmarkStore = serde_json::from_slice(bytes)?;
    let local = load_store_opt()?;
    let local_ms = local.as_ref().map(|s| s.updated_ms).unwrap_or(0);
    if let Some(l) = &local {
        if remote.updated_ms == local_ms && same_list(&l.bookmarks, &remote.bookmarks) {
            return Ok(RemoteOutcome::Same);
        }
    }
    // Same time, different lists (lists from before the time was kept are all 0): the team's.
    if local.is_some() && remote.updated_ms == local_ms {
        write_store(&remote)?;
        return Ok(RemoteOutcome::Taken);
    }
    if local.is_some() && local_ms > remote.updated_ms && !authoritative {
        return Ok(RemoteOutcome::Push);
    }
    write_store(&remote)?;
    Ok(RemoteOutcome::Taken)
}

fn normalise_url(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() || t.contains("://") {
        return t.to_string();
    }
    format!("https://{t}")
}

/// Chromium timestamps are microseconds since 1601-01-01.
fn chrome_now() -> String {
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    ((unix + 11_644_473_600) * 1_000_000).to_string()
}

fn empty_tree() -> Value {
    let ts = chrome_now();
    let root = |id: &str, name: &str| {
        json!({
            "children": [], "date_added": ts, "date_modified": ts,
            "guid": uuid::Uuid::new_v4().to_string(),
            "id": id, "name": name, "type": "folder"
        })
    };
    json!({
        "checksum": "",
        "roots": {
            "bookmark_bar": root("1", "Bookmarks bar"),
            "other": root("2", "Other bookmarks"),
            "synced": root("3", "Mobile bookmarks"),
        },
        "version": 1
    })
}

fn max_id(node: &Value, cur: &mut u64) {
    if let Some(id) = node.get("id").and_then(|v| v.as_str()).and_then(|s| s.parse::<u64>().ok()) {
        *cur = (*cur).max(id);
    }
    if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
        for c in children {
            max_id(c, cur);
        }
    }
}

/// Rewrite `<udd>/Default/Bookmarks` so it carries exactly the bookmarks this
/// launcher folder defines, leaving everything the operator added untouched.
pub fn apply_to_profile(udd: &Path, folder: &str) -> Result<usize> {
    // No list on this machine: whatever the profile carries is the team's, not ours to strip.
    let Some(store) = load_store_opt()? else { return Ok(0) };
    let wanted: Vec<Bookmark> = store
        .bookmarks
        .into_iter()
        .filter(|b| b.folder.is_empty() || b.folder == folder)
        .collect();

    let default_dir = udd.join("Default");
    let path = default_dir.join("Bookmarks");
    let existing = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok());

    // Nothing to add and nothing of ours to clean out — leave the file alone
    // rather than rewriting a profile that never had managed bookmarks.
    if wanted.is_empty() && existing.is_none() {
        return Ok(0);
    }
    let mut tree = existing.unwrap_or_else(empty_tree);

    // Highest id in use, before anything is borrowed mutably.
    let mut next_id = 0u64;
    max_id(&tree, &mut next_id);

    let Some(bar) = tree
        .get_mut("roots")
        .and_then(|r| r.get_mut("bookmark_bar"))
        .and_then(|b| b.as_object_mut())
    else {
        return Ok(0);
    };
    let Some(list) = bar
        .entry("children")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
    else {
        return Ok(0);
    };

    let had_ours = list.iter().any(is_managed);
    list.retain(|n| !is_managed(n));
    if wanted.is_empty() && !had_ours {
        return Ok(0);
    }
    if !wanted.is_empty() {
        let ts = chrome_now();
        let mut kids = Vec::with_capacity(wanted.len());
        for b in &wanted {
            next_id += 1;
            kids.push(json!({
                "date_added": ts,
                "guid": uuid::Uuid::new_v4().to_string(),
                "id": next_id.to_string(),
                "name": b.title,
                "type": "url",
                "url": b.url,
            }));
        }
        next_id += 1;
        list.insert(
            0,
            json!({
                "children": kids,
                "date_added": ts, "date_modified": ts,
                "guid": uuid::Uuid::new_v4().to_string(),
                "id": next_id.to_string(),
                "name": MANAGED_FOLDER,
                "type": "folder",
                "meta_info": { MANAGED_KEY: "1" },
            }),
        );
    }

    // The checksum no longer matches. Chromium notices, keeps the bookmarks and
    // rewrites the file itself — blanking it is how we say "recompute".
    tree["checksum"] = json!("");

    fs::create_dir_all(&default_dir)?;
    fs::write(&path, serde_json::to_string(&tree)?)?;
    Ok(wanted.len())
}

fn is_managed(node: &Value) -> bool {
    // The marker only: a folder the operator made and happened to call "Hir-Login" is theirs.
    node.get("meta_info")
        .and_then(|m| m.get(MANAGED_KEY))
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("hir-bm-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(d.join("udd").join("Default")).unwrap();
        store::set_data_root(Some(d.clone()));
        d
    }

    fn bar_names(udd: &Path) -> Vec<String> {
        let v: Value = serde_json::from_str(&fs::read_to_string(udd.join("Default/Bookmarks")).unwrap()).unwrap();
        v["roots"]["bookmark_bar"]["children"].as_array().unwrap().iter()
            .map(|n| n["name"].as_str().unwrap_or("").to_string()).collect()
    }

    /// A machine that never had a list (a team member's) launched a profile carrying the team's
    /// bookmarks and stripped them — and the stripped file went up to the team.
    #[test]
    fn no_list_here_leaves_the_profiles_bookmarks_alone() {
        let _g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = scratch();
        let udd = d.join("udd");
        let tree = json!({"checksum": "x", "version": 1, "roots": {
            "bookmark_bar": {"type": "folder", "id": "1", "name": "Bookmarks bar", "children": [
                {"type": "folder", "id": "3", "name": "Hir-Login", "meta_info": {MANAGED_KEY: "1"},
                 "children": [{"type": "url", "id": "4", "name": "outlook", "url": "https://outlook.office.com/"}]}
            ]},
            "other": {"type": "folder", "id": "2", "name": "Other", "children": []},
            "synced": {"type": "folder", "id": "5", "name": "Mobile", "children": []}}});
        let before = serde_json::to_string(&tree).unwrap();
        fs::write(udd.join("Default/Bookmarks"), &before).unwrap();

        assert_eq!(apply_to_profile(&udd, "").unwrap(), 0);
        assert_eq!(fs::read_to_string(udd.join("Default/Bookmarks")).unwrap(), before, "untouched");

        // Once this machine has a list — even an empty one — it is the list.
        save(Bookmark { id: String::new(), title: "".into(), url: "tiktok.com".into(), folder: "".into() }).unwrap();
        apply_to_profile(&udd, "").unwrap();
        assert_eq!(bar_names(&udd), vec!["Hir-Login"]);
        let v: Value = serde_json::from_str(&fs::read_to_string(udd.join("Default/Bookmarks")).unwrap()).unwrap();
        assert_eq!(v["roots"]["bookmark_bar"]["children"][0]["children"][0]["url"], "https://tiktok.com");
        let _ = fs::remove_dir_all(&d);
    }

    /// The operator's own folder that happens to share the name is not ours.
    #[test]
    fn a_folder_only_named_like_ours_stays() {
        let _g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = scratch();
        let udd = d.join("udd");
        let mut tree = empty_tree();
        tree["roots"]["bookmark_bar"]["children"] = json!([{"type": "folder", "id": "9", "name": "Hir-Login", "children": []}]);
        fs::write(udd.join("Default/Bookmarks"), serde_json::to_string(&tree).unwrap()).unwrap();
        save(Bookmark { id: String::new(), title: "a".into(), url: "a.com".into(), folder: "".into() }).unwrap();
        apply_to_profile(&udd, "").unwrap();
        assert_eq!(bar_names(&udd), vec!["Hir-Login", "Hir-Login"], "ours added, theirs kept");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn browser_export_file_is_imported_once() {
        let _g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = scratch();
        let html = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<DL><p>
    <DT><H3 ADD_DATE="1">Hir-Login</H3>
    <DL><p>
        <DT><A HREF="https://outlook.office.com/mail/login" ADD_DATE="1" ICON="data:image/png;base64,AAA">outlook</A>
        <DT><A HREF="https://affiliate.shopee.sg/" ADD_DATE="1">affiliate</A>
        <DT><A HREF="https://www.whatismyip.com/?a=1&amp;b=2" ADD_DATE="1">https://www.whatismyip.com/</A>
        <DT><A HREF="javascript:alert(1)">bad</A>
    </DL><p>
</DL><p>"#;
        assert_eq!(import_html(html, "shop").unwrap(), (3, 0));
        assert_eq!(import_html(html, "shop").unwrap(), (0, 3), "a second import adds nothing");
        let l = list().unwrap();
        assert_eq!(l[0].title, "outlook");
        assert_eq!(l[0].url, "https://outlook.office.com/mail/login");
        assert_eq!(l[2].url, "https://www.whatismyip.com/?a=1&b=2");
        assert!(l.iter().all(|b| b.folder == "shop"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn the_newer_list_wins_between_machines() {
        let _g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = scratch();
        let remote = serde_json::to_vec(&json!({"updated_ms": 1000, "bookmarks": [
            {"id": "r", "title": "r", "url": "https://r.com", "folder": ""}]})).unwrap();
        // Nothing here yet: take it.
        assert_eq!(take_remote(&remote, false).unwrap(), RemoteOutcome::Taken);
        assert_eq!(list().unwrap().len(), 1);
        assert_eq!(take_remote(&remote, false).unwrap(), RemoteOutcome::Same);
        // Changed here after: this one goes up — unless this machine may not change the team's.
        save(Bookmark { id: String::new(), title: "l".into(), url: "l.com".into(), folder: "".into() }).unwrap();
        assert_eq!(take_remote(&remote, false).unwrap(), RemoteOutcome::Push);
        assert_eq!(take_remote(&remote, true).unwrap(), RemoteOutcome::Taken);
        assert_eq!(list().unwrap().len(), 1);
        // A list from before the time was kept (0 on both sides) but different: the team's wins.
        write_store(&BookmarkStore { bookmarks: vec![], updated_ms: 0 }).unwrap();
        let old_team = serde_json::to_vec(&json!({"bookmarks": [
            {"id": "t", "title": "t", "url": "https://t.com", "folder": ""}]})).unwrap();
        assert_eq!(take_remote(&old_team, false).unwrap(), RemoteOutcome::Taken);
        assert_eq!(list().unwrap()[0].id, "t");
        let _ = fs::remove_dir_all(&d);
    }
}
