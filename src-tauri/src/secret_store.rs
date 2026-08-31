use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

const KEYCHAIN_SERVICE: &str = "Harbor Transfer";
const KEYCHAIN_VAULT_ACCOUNT: &str = "credential-vault-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretLookup {
    Missing,
    Removed,
    Value(String),
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SecretVault {
    version: u8,
    entries: HashMap<String, String>,
    removed: HashSet<String>,
}

#[derive(Default)]
struct SecretVaultCache {
    loaded: bool,
    vault: SecretVault,
}

static SECRET_VAULT_CACHE: OnceLock<Mutex<SecretVaultCache>> = OnceLock::new();

fn cache() -> &'static Mutex<SecretVaultCache> {
    SECRET_VAULT_CACHE.get_or_init(|| Mutex::new(SecretVaultCache::default()))
}

fn entry() -> Result<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_VAULT_ACCOUNT).map_err(Into::into)
}

fn ensure_loaded(cache: &mut SecretVaultCache) -> Result<()> {
    if cache.loaded {
        return Ok(());
    }
    cache.vault = match entry()?.get_password() {
        Ok(value) => {
            let vault: SecretVault =
                serde_json::from_str(&value).context("The Harbor Transfer Keychain vault is invalid")?;
            if vault.version > 1 {
                return Err(anyhow!(
                    "The Harbor Transfer Keychain vault was created by a newer app version."
                ));
            }
            vault
        }
        Err(keyring::Error::NoEntry) => SecretVault { version: 1, ..SecretVault::default() },
        Err(error) => return Err(error.into()),
    };
    cache.loaded = true;
    Ok(())
}

fn persist(vault: &SecretVault) -> Result<()> {
    let serialized = serde_json::to_string(vault).context("Could not encode the Keychain vault")?;
    entry()?.set_password(&serialized)?;
    Ok(())
}

pub fn lookup(key: &str) -> Result<SecretLookup> {
    let mut cache = cache().lock().map_err(|_| anyhow!("The Keychain vault cache is unavailable."))?;
    ensure_loaded(&mut cache)?;
    if let Some(value) = cache.vault.entries.get(key) {
        return Ok(SecretLookup::Value(value.clone()));
    }
    if cache.vault.removed.contains(key) {
        return Ok(SecretLookup::Removed);
    }
    Ok(SecretLookup::Missing)
}

pub fn store(key: &str, value: &str) -> Result<()> {
    let mut cache = cache().lock().map_err(|_| anyhow!("The Keychain vault cache is unavailable."))?;
    ensure_loaded(&mut cache)?;
    if cache.vault.entries.get(key).map(String::as_str) == Some(value) && !cache.vault.removed.contains(key) {
        return Ok(());
    }
    let mut updated = SecretVault {
        version: 1,
        entries: cache.vault.entries.clone(),
        removed: cache.vault.removed.clone(),
    };
    updated.entries.insert(key.to_string(), value.to_string());
    updated.removed.remove(key);
    persist(&updated)?;
    cache.vault = updated;
    Ok(())
}

pub fn remove(key: &str) -> Result<()> {
    let mut cache = cache().lock().map_err(|_| anyhow!("The Keychain vault cache is unavailable."))?;
    ensure_loaded(&mut cache)?;
    if !cache.vault.entries.contains_key(key) && cache.vault.removed.contains(key) {
        return Ok(());
    }
    let mut updated = SecretVault {
        version: 1,
        entries: cache.vault.entries.clone(),
        removed: cache.vault.removed.clone(),
    };
    updated.entries.remove(key);
    updated.removed.insert(key.to_string());
    persist(&updated)?;
    cache.vault = updated;
    Ok(())
}

/// Removes short-lived state that has no legacy Keychain entry to suppress.
/// Unlike credential removal, this does not retain a permanent tombstone.
pub fn remove_ephemeral(key: &str) -> Result<()> {
    let mut cache = cache().lock().map_err(|_| anyhow!("The Keychain vault cache is unavailable."))?;
    ensure_loaded(&mut cache)?;
    if !cache.vault.entries.contains_key(key) && !cache.vault.removed.contains(key) {
        return Ok(());
    }
    let mut updated = SecretVault {
        version: 1,
        entries: cache.vault.entries.clone(),
        removed: cache.vault.removed.clone(),
    };
    updated.entries.remove(key);
    updated.removed.remove(key);
    persist(&updated)?;
    cache.vault = updated;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SecretLookup, SecretVault};

    fn lookup(vault: &SecretVault, key: &str) -> SecretLookup {
        if let Some(value) = vault.entries.get(key) {
            SecretLookup::Value(value.clone())
        } else if vault.removed.contains(key) {
            SecretLookup::Removed
        } else {
            SecretLookup::Missing
        }
    }

    #[test]
    fn tombstones_prevent_deleted_legacy_secrets_from_returning() {
        let mut vault = SecretVault { version: 1, ..SecretVault::default() };
        assert_eq!(lookup(&vault, "bookmark:one"), SecretLookup::Missing);
        vault.removed.insert("bookmark:one".into());
        assert_eq!(lookup(&vault, "bookmark:one"), SecretLookup::Removed);
    }

    #[test]
    fn vault_round_trip_preserves_namespaced_secrets() {
        let mut vault = SecretVault { version: 1, ..SecretVault::default() };
        vault.entries.insert("bookmark:one".into(), "secret".into());
        vault.removed.insert("bookmark:old".into());
        let encoded = serde_json::to_string(&vault).unwrap();
        let decoded: SecretVault = serde_json::from_str(&encoded).unwrap();
        assert_eq!(lookup(&decoded, "bookmark:one"), SecretLookup::Value("secret".into()));
        assert_eq!(lookup(&decoded, "bookmark:old"), SecretLookup::Removed);
    }
}
