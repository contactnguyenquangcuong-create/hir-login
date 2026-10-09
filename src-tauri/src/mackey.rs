//! Which OSCrypt key the browser engine on a Mac really seals its cookies with.
//!
//! Chromium on macOS seals cookies with a key derived from a password. Either that password is
//! fixed (`mock_password`, when the engine runs with a mock keychain) or it is a random one the
//! engine keeps in the login Keychain ("Chromium Safe Storage"). The launcher has to seal the
//! logins it restores with the same key, or the engine cannot read them, drops them, and the
//! profile opens logged out. Which of the two this engine uses was assumed, never observed; here
//! it is observed: after a profile closes, the cookies the engine itself kept are tried with each
//! key, and the one that opens them is remembered for the next restore.
//!
//! The decision itself is plain code that runs (and is tested) on every platform; the Keychain
//! and the remembered answer exist only on a Mac.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use aes::Aes128;
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};

type Aes128CbcDec = cbc::Decryptor<Aes128>;
const IV: [u8; 16] = [0x20; 16];

/// The fixed password of an engine running with a mock keychain.
pub(crate) const MOCK_PASSWORD: &[u8] = b"mock_password";

/// The AES key Chromium derives from an OSCrypt password on macOS.
pub(crate) fn derive(password: &[u8]) -> Vec<u8> {
    let mut key = [0u8; 16];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password, b"saltysalt", 1003, &mut key);
    key.to_vec()
}

/// Whether `key` opens a sealed value ("v10" + AES-128-CBC).
pub(crate) fn opens(key: &[u8], blob: &[u8]) -> bool {
    if blob.len() < 4 || &blob[..3] != b"v10" {
        return false;
    }
    Aes128CbcDec::new_from_slices(key, &IV)
        .ok()
        .and_then(|d| d.decrypt_padded_vec_mut::<Pkcs7>(&blob[3..]).ok())
        .is_some()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Engine {
    /// The fixed `mock_password`.
    Mock,
    /// The random password in the login Keychain.
    Keychain,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Verdict {
    pub engine: Engine,
    /// Sealed values looked at.
    pub sealed: usize,
    pub mock_opens: usize,
    /// `None` when the mock key opened them all and the Keychain was not asked.
    pub keychain_opens: Option<usize>,
}

/// Which key the engine used, from values the engine itself wrote. `None` when there are too few
/// sealed values to tell. The Keychain (which may ask the person for permission) is asked only
/// when the mock key does not open everything.
pub(crate) fn decide(blobs: &[Vec<u8>], keychain_password: impl FnOnce() -> Option<Vec<u8>>) -> Option<Verdict> {
    let sealed: Vec<&Vec<u8>> = blobs.iter().filter(|b| b.len() > 3 && &b[..3] == b"v10").collect();
    if sealed.len() < 3 {
        return None;
    }
    let mock = derive(MOCK_PASSWORD);
    let mock_opens = sealed.iter().filter(|b| opens(&mock, b)).count();
    if mock_opens == sealed.len() {
        return Some(Verdict { engine: Engine::Mock, sealed: sealed.len(), mock_opens, keychain_opens: None });
    }
    let keychain_opens = keychain_password().map(|p| {
        let key = derive(&p);
        sealed.iter().filter(|b| opens(&key, b)).count()
    });
    let engine = if keychain_opens.is_some_and(|k| k > mock_opens) { Engine::Keychain } else { Engine::Mock };
    Some(Verdict { engine, sealed: sealed.len(), mock_opens, keychain_opens })
}

#[cfg(target_os = "macos")]
mod mac {
    use super::Engine;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    /// The password of "Chromium Safe Storage" in the login Keychain, asked of the system once
    /// per run (the first time it may show the person a permission dialog). `None` when there is
    /// none, the person said no, or it took longer than two minutes.
    pub(crate) fn keychain_password() -> Option<Vec<u8>> {
        static CACHE: OnceLock<Option<Vec<u8>>> = OnceLock::new();
        CACHE
            .get_or_init(|| {
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let out = std::process::Command::new("security")
                        .args(["find-generic-password", "-w", "-s", "Chromium Safe Storage", "-a", "Chromium"])
                        .output();
                    let _ = tx.send(out);
                });
                let out = rx.recv_timeout(std::time::Duration::from_secs(120)).ok()?.ok()?;
                if !out.status.success() {
                    return None;
                }
                let mut pw = out.stdout;
                while pw.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
                    pw.pop();
                }
                if pw.is_empty() { None } else { Some(pw) }
            })
            .clone()
    }

    fn state_path() -> Option<PathBuf> {
        Some(crate::store::data_root().ok()?.join("mac-key.json"))
    }

    /// (the key the engine seals with, whether the Keychain key has ever opened its values).
    pub(crate) fn stored() -> (Engine, bool) {
        let read = || -> Option<(Engine, bool)> {
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(state_path()?).ok()?).ok()?;
            let engine = if v.get("engine").and_then(|e| e.as_str()) == Some("keychain") { Engine::Keychain } else { Engine::Mock };
            Some((engine, v.get("seen_keychain").and_then(|b| b.as_bool()).unwrap_or(false)))
        };
        read().unwrap_or((Engine::Mock, false))
    }

    pub(crate) fn save(engine: Engine, seen_keychain: bool) {
        if let Some(p) = state_path() {
            let v = serde_json::json!({
                "engine": if engine == Engine::Keychain { "keychain" } else { "mock" },
                "seen_keychain": seen_keychain,
            });
            let _ = std::fs::write(p, v.to_string());
        }
    }

    /// The password this machine's launcher seals the logins it restores with.
    pub(crate) fn engine_password() -> Vec<u8> {
        match stored().0 {
            Engine::Keychain => keychain_password().unwrap_or_else(|| super::MOCK_PASSWORD.to_vec()),
            Engine::Mock => super::MOCK_PASSWORD.to_vec(),
        }
    }

    /// Keys that only open values — for rows sealed with the other password, written before the
    /// engine's key was known or by an older build.
    pub(crate) fn alternates() -> Vec<Vec<u8>> {
        let (engine, seen_keychain) = stored();
        match engine {
            Engine::Keychain => vec![super::derive(super::MOCK_PASSWORD)],
            Engine::Mock if seen_keychain => keychain_password().map(|p| vec![super::derive(&p)]).unwrap_or_default(),
            Engine::Mock => Vec::new(),
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) use mac::{alternates, engine_password, keychain_password, save, stored};

#[cfg(test)]
mod tests {
    use super::*;
    use cbc::cipher::BlockEncryptMut;

    fn seal(password: &[u8], plain: &[u8]) -> Vec<u8> {
        let enc = cbc::Encryptor::<Aes128>::new_from_slices(&derive(password), &IV).unwrap();
        let mut out = b"v10".to_vec();
        out.extend_from_slice(&enc.encrypt_padded_vec_mut::<Pkcs7>(plain));
        out
    }

    #[test]
    fn the_key_that_opens_what_the_engine_wrote_is_the_engines_key() {
        let mock: Vec<Vec<u8>> = (0..5).map(|i| seal(MOCK_PASSWORD, format!("value-{i}").as_bytes())).collect();
        // Everything opens with the fixed key: the Keychain is never asked.
        let v = decide(&mock, || panic!("the Keychain must not be asked when the mock key opens everything")).unwrap();
        assert_eq!((v.engine, v.sealed, v.mock_opens, v.keychain_opens), (Engine::Mock, 5, 5, None));

        // Nothing opens with it, everything with the Keychain's.
        let kc: Vec<Vec<u8>> = (0..5).map(|i| seal(b"random-keychain-password", format!("value-{i}").as_bytes())).collect();
        let v = decide(&kc, || Some(b"random-keychain-password".to_vec())).unwrap();
        assert_eq!((v.engine, v.mock_opens, v.keychain_opens), (Engine::Keychain, 0, Some(5)));

        // The Keychain says nothing (no entry, or the person refused): the mock key stays.
        let v = decide(&kc, || None).unwrap();
        assert_eq!((v.engine, v.keychain_opens), (Engine::Mock, None));

        // A mix: the key that opens more wins; a tie keeps the mock key.
        let mut mixed = mock[..2].to_vec();
        mixed.extend(kc[..3].iter().cloned());
        assert_eq!(decide(&mixed, || Some(b"random-keychain-password".to_vec())).unwrap().engine, Engine::Keychain);
        let mut tie = mock[..2].to_vec();
        tie.extend(kc[..2].iter().cloned());
        assert_eq!(decide(&tie, || Some(b"random-keychain-password".to_vec())).unwrap().engine, Engine::Mock);

        // Too few values to tell, or none sealed at all.
        assert!(decide(&mock[..2], || None).is_none());
        assert!(decide(&vec![b"plain".to_vec(); 5], || None).is_none());
    }
}
