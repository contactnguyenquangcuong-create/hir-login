//! Who is allowed to do what on the team server. Pure data and rules, no I/O
//! beyond one JSON file, so the whole permission model can be tested without a
//! network.
//!
//! Roles: the *admin* is whoever started the server (the original shared token);
//! a *manager* can add, edit and delete anywhere; a *member* only reaches the
//! folders they were granted, at one level per folder — use, edit or delete.
//! A folder a member was not granted does not exist for them.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    /// Cannot see it at all.
    None = 0,
    /// Open and close the profile; change nothing about it.
    Use = 1,
    /// Use + change its configuration.
    Edit = 2,
    /// Edit + delete it.
    Delete = 3,
    /// Admin / manager: also add, move between folders.
    Full = 4,
}

impl Level {
    pub fn parse(s: &str) -> Level {
        match s {
            "use" => Level::Use,
            "edit" => Level::Edit,
            "delete" => Level::Delete,
            _ => Level::None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Level::None => "none",
            Level::Use => "use",
            Level::Edit => "edit",
            Level::Delete => "delete",
            Level::Full => "full",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Admin,
    Manager,
    Member,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Manager => "manager",
            Role::Member => "member",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub role: Role,
}

impl Identity {
    pub fn admin() -> Self {
        Identity { id: "admin".into(), name: "Admin".into(), role: Role::Admin }
    }
    /// May add things (profiles, folders, fingerprints) and see everything.
    pub fn is_privileged(&self) -> bool {
        matches!(self.role, Role::Admin | Role::Manager)
    }
}

#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub struct Member {
    pub id: String,
    pub name: String,
    /// "manager" or "member".
    pub role: String,
    #[serde(rename = "tokenHash")]
    pub token_hash: String,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default, rename = "createdAt")]
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct AclStore {
    #[serde(default)]
    pub members: Vec<Member>,
    /// folder name -> member id -> "use" | "edit" | "delete"
    #[serde(default)]
    pub folders: BTreeMap<String, BTreeMap<String, String>>,
}

pub fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

pub fn new_token() -> String {
    format!("hm_{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

impl AclStore {
    pub fn load(dir: &Path) -> AclStore {
        std::fs::read_to_string(dir.join("members.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let path = dir.join("members.json");
        let tmp = dir.join("members.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap_or_default())?;
        std::fs::rename(tmp, path)
    }

    /// The member whose token this is, unless they were switched off.
    pub fn authenticate(&self, token: &str) -> Option<Identity> {
        let h = hash_token(token);
        self.members.iter().find(|m| m.token_hash == h && !m.disabled).map(|m| Identity {
            id: m.id.clone(),
            name: m.name.clone(),
            role: if m.role == "manager" { Role::Manager } else { Role::Member },
        })
    }

    /// What `who` may do with anything in `folder`. A profile that is in no
    /// folder belongs to admins and managers only.
    pub fn level(&self, who: &Identity, folder: &str) -> Level {
        if who.is_privileged() {
            return Level::Full;
        }
        if folder.is_empty() {
            return Level::None;
        }
        self.folders
            .get(folder)
            .and_then(|m| m.get(&who.id))
            .map(|s| Level::parse(s))
            .unwrap_or(Level::None)
    }

    pub fn set_access(&mut self, folder: &str, member_id: &str, level: Level) {
        let entry = self.folders.entry(folder.to_string()).or_default();
        if level == Level::None {
            entry.remove(member_id);
        } else {
            entry.insert(member_id.to_string(), level.as_str().to_string());
        }
        if entry.is_empty() {
            self.folders.remove(folder);
        }
    }

    pub fn remove_member(&mut self, member_id: &str) {
        self.members.retain(|m| m.id != member_id);
        for f in self.folders.values_mut() {
            f.remove(member_id);
        }
        self.folders.retain(|_, v| !v.is_empty());
    }
}

/// Key order must not matter: two machines serialising the same profile can
/// list its fields differently.
fn canonical(v: &Value, out: &mut String) {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for k in keys {
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push(':');
                canonical(&m[k], out);
                out.push(',');
            }
            out.push('}');
        }
        Value::Array(a) => {
            out.push('[');
            for x in a {
                canonical(x, out);
                out.push(',');
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// The part of a profile a user with only "use" access must not be able to
/// change: its configuration, folder, extensions, colour and bound proxy.
/// Runtime bookkeeping (last launch, run time, revision, pin) is left out
/// because opening a profile legitimately changes those.
pub fn protected_signature(profile_json: &Value, proxy_json: &Value) -> String {
    let mut cfg = profile_json.clone();
    let meta = cfg.as_object_mut().and_then(|o| o.remove("_meta")).unwrap_or(Value::Null);
    let pick = |k: &str| meta.get(k).cloned().unwrap_or(Value::Null);
    let mut proxy = proxy_json.clone();
    if let Some(o) = proxy.as_object_mut() {
        o.remove("country"); // each machine measures its own
    }
    let sig = serde_json::json!({
        "config": cfg,
        "folder": pick("folder"),
        "extensions": pick("extensions"),
        "color": pick("color"),
        "proxy": proxy,
    });
    let mut s = String::new();
    canonical(&sig, &mut s);
    Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

pub fn folder_of(profile_json: &Value) -> String {
    profile_json
        .get("_meta")
        .and_then(|m| m.get("folder"))
        .and_then(|f| f.as_str())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> (AclStore, Identity, Identity) {
        let mut s = AclStore::default();
        let (t1, t2) = (new_token(), new_token());
        s.members.push(Member { id: "m1".into(), name: "An".into(), role: "member".into(), token_hash: hash_token(&t1), ..Default::default() });
        s.members.push(Member { id: "m2".into(), name: "Binh".into(), role: "manager".into(), token_hash: hash_token(&t2), ..Default::default() });
        let an = s.authenticate(&t1).unwrap();
        let binh = s.authenticate(&t2).unwrap();
        (s, an, binh)
    }

    #[test]
    fn a_member_sees_only_granted_folders_at_the_granted_level() {
        let (mut s, an, binh) = store();
        assert_eq!(s.level(&an, "Ads"), Level::None, "no grant, no access");
        s.set_access("Ads", "m1", Level::Use);
        s.set_access("Shop", "m1", Level::Delete);
        assert_eq!(s.level(&an, "Ads"), Level::Use);
        assert_eq!(s.level(&an, "Shop"), Level::Delete);
        assert_eq!(s.level(&an, "Other"), Level::None, "folders are independent");
        assert_eq!(s.level(&an, ""), Level::None, "unfiled belongs to admins");
        assert_eq!(s.level(&binh, "Anything"), Level::Full);
        assert_eq!(s.level(&binh, ""), Level::Full);
        assert_eq!(s.level(&Identity::admin(), "x"), Level::Full);
        s.set_access("Ads", "m1", Level::None);
        assert_eq!(s.level(&an, "Ads"), Level::None);
        assert!(Level::Use < Level::Edit && Level::Edit < Level::Delete && Level::Delete < Level::Full);
    }

    #[test]
    fn tokens_are_stored_hashed_and_disabling_locks_a_member_out() {
        let (mut s, _, _) = store();
        let t = new_token();
        s.members.push(Member { id: "m3".into(), name: "C".into(), role: "member".into(), token_hash: hash_token(&t), ..Default::default() });
        assert!(s.authenticate(&t).is_some());
        assert!(s.members.iter().all(|m| !m.token_hash.contains("hm_")), "raw token must not be stored");
        s.members.iter_mut().find(|m| m.id == "m3").unwrap().disabled = true;
        assert!(s.authenticate(&t).is_none());
        assert!(s.authenticate("wrong").is_none());
    }

    #[test]
    fn removing_a_member_removes_their_grants() {
        let (mut s, _, _) = store();
        s.set_access("Ads", "m1", Level::Edit);
        s.set_access("Ads", "m2", Level::Use);
        s.remove_member("m1");
        assert_eq!(s.folders["Ads"].len(), 1);
        s.remove_member("m2");
        assert!(s.folders.is_empty());
    }

    #[test]
    fn protected_signature_ignores_bookkeeping_but_catches_real_edits() {
        let base = json!({"name":"P","navigator":{"a":1,"b":2},"_meta":{"folder":"Ads","extensions":["x"],"color":null,"pinned":false,"rev":1,"last_launched_at":null,"total_runtime_ms":0}});
        let proxy = json!({"id":"p","host":"h","port":1,"country":"VN"});
        let sig = protected_signature(&base, &proxy);
        // Opening the profile: bookkeeping moves, key order differs, other machine's country tag.
        let opened = json!({"navigator":{"b":2,"a":1},"name":"P","_meta":{"folder":"Ads","extensions":["x"],"color":null,"pinned":true,"rev":9,"last_launched_at":"now","total_runtime_ms":5000}});
        let proxy2 = json!({"port":1,"host":"h","id":"p","country":"PH"});
        assert_eq!(sig, protected_signature(&opened, &proxy2));
        // Real edits are caught.
        let mut renamed = base.clone(); renamed["name"] = json!("Q");
        let mut moved = base.clone(); moved["_meta"]["folder"] = json!("Other");
        let mut ext = base.clone(); ext["_meta"]["extensions"] = json!(["x","y"]);
        let mut fp = base.clone(); fp["navigator"]["a"] = json!(99);
        for changed in [renamed, moved, ext, fp] {
            assert_ne!(sig, protected_signature(&changed, &proxy));
        }
        assert_ne!(sig, protected_signature(&base, &json!({"id":"p","host":"OTHER","port":1})));
        assert_ne!(sig, protected_signature(&base, &Value::Null), "unbinding the proxy is an edit");
    }
}
