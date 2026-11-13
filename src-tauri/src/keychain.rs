//! The API key, kept in the operating system's keychain: the macOS Keychain,
//! the Windows Credential Manager, or the Secret Service on Linux (GNOME
//! Keyring, KWallet).
//!
//! The key is never written to a file by this app. When no keychain can be
//! used, the key is kept in memory until the app quits, and the UI says so.
//! The UI never gets the key back: only whether one is set and its last
//! four characters.

use std::sync::{Arc, Mutex};

use serde::Serialize;

pub const SERVICE: &str = "sh.photos.desktop";
pub const ACCOUNT: &str = "api-key";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    /// There is no keychain, or it cannot be reached or unlocked.
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Other(String),
}

/// Somewhere secrets are kept.
pub trait SecretStore: Send + Sync {
    fn get(&self) -> Result<Option<String>, SecretError>;
    fn set(&self, secret: &str) -> Result<(), SecretError>;
    fn delete(&self) -> Result<(), SecretError>;
}

/// The operating system's keychain.
pub struct OsKeychain {
    service: String,
    account: String,
}

impl Default for OsKeychain {
    fn default() -> Self {
        Self::new(SERVICE, ACCOUNT)
    }
}

impl OsKeychain {
    pub fn new(service: &str, account: &str) -> Self {
        Self {
            service: service.to_string(),
            account: account.to_string(),
        }
    }

    fn entry(&self) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, &self.account).map_err(map_error)
    }
}

fn map_error(error: keyring::Error) -> SecretError {
    match error {
        keyring::Error::NoStorageAccess(e) | keyring::Error::PlatformFailure(e) => {
            SecretError::Unavailable(e.to_string())
        }
        keyring::Error::NoDefaultStore => {
            SecretError::Unavailable("no keychain service was found".to_string())
        }
        other => SecretError::Other(other.to_string()),
    }
}

impl SecretStore for OsKeychain {
    fn get(&self) -> Result<Option<String>, SecretError> {
        match self.entry()?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_error(e)),
        }
    }

    fn set(&self, secret: &str) -> Result<(), SecretError> {
        self.entry()?.set_password(secret).map_err(map_error)
    }

    fn delete(&self) -> Result<(), SecretError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_error(e)),
        }
    }
}

/// Where the key in use is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Storage {
    /// No key is set.
    None,
    /// In the system keychain.
    Keychain,
    /// In memory only: it is gone when the app quits.
    Session,
}

/// What the UI may know about the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyStatus {
    pub set: bool,
    /// The key's last four characters, to recognise it by.
    pub last4: Option<String>,
    pub storage: Storage,
    /// False when the system keychain cannot be used.
    pub keychain_available: bool,
    /// Why the keychain cannot be used, as the system put it.
    pub keychain_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    #[error("Paste an API key first.")]
    Empty,
    #[error("An API key has no spaces or line breaks. Copy it again from the console.")]
    Malformed,
    #[error("The keychain refused: {0}")]
    Keychain(String),
}

#[derive(Default)]
struct Cached {
    /// Whether the keychain was read yet.
    loaded: bool,
    key: Option<String>,
    storage: Option<Storage>,
    keychain_error: Option<String>,
}

/// The API key: the keychain, with what was read from it kept in memory.
pub struct KeyVault {
    store: Box<dyn SecretStore>,
    cached: Mutex<Cached>,
}

impl KeyVault {
    pub fn new(store: Box<dyn SecretStore>) -> Self {
        Self {
            store,
            cached: Mutex::default(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Cached> {
        let mut cached = self
            .cached
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !cached.loaded {
            cached.loaded = true;
            match self.store.get() {
                Ok(key) => {
                    cached.storage = Some(match key {
                        Some(_) => Storage::Keychain,
                        None => Storage::None,
                    });
                    cached.key = key;
                }
                Err(SecretError::Unavailable(e) | SecretError::Other(e)) => {
                    cached.storage = Some(Storage::None);
                    cached.keychain_error = Some(e);
                }
            }
        }
        cached
    }

    /// The key, for the API client only.
    pub fn key(&self) -> Option<String> {
        self.lock().key.clone()
    }

    pub fn status(&self) -> KeyStatus {
        status_of(&self.lock())
    }

    /// Stores a key. When the keychain cannot be used, the key is kept for
    /// this session only, and the status says so.
    pub fn set(&self, key: &str) -> Result<KeyStatus, KeyError> {
        let key = key.trim();
        if key.is_empty() {
            return Err(KeyError::Empty);
        }
        if key.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(KeyError::Malformed);
        }
        let mut cached = self.lock();
        match self.store.set(key) {
            Ok(()) => {
                cached.storage = Some(Storage::Keychain);
                cached.keychain_error = None;
            }
            Err(SecretError::Unavailable(e)) => {
                cached.storage = Some(Storage::Session);
                cached.keychain_error = Some(e);
            }
            Err(SecretError::Other(e)) => return Err(KeyError::Keychain(e)),
        }
        cached.key = Some(key.to_string());
        Ok(status_of(&cached))
    }

    /// Forgets the key, here and in the keychain.
    pub fn remove(&self) -> Result<KeyStatus, KeyError> {
        let mut cached = self.lock();
        match self.store.delete() {
            Ok(()) => cached.keychain_error = None,
            Err(SecretError::Unavailable(e)) => cached.keychain_error = Some(e),
            Err(SecretError::Other(e)) => return Err(KeyError::Keychain(e)),
        }
        cached.key = None;
        cached.storage = Some(Storage::None);
        Ok(status_of(&cached))
    }
}

fn status_of(cached: &Cached) -> KeyStatus {
    KeyStatus {
        set: cached.key.is_some(),
        last4: cached.key.as_deref().map(last4),
        storage: cached.storage.unwrap_or(Storage::None),
        keychain_available: cached.keychain_error.is_none(),
        keychain_error: cached.keychain_error.clone(),
    }
}

/// The last four characters; fewer for a key too short to hide behind them.
fn last4(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    let shown = if chars.len() >= 12 {
        4
    } else {
        chars.len().min(2)
    };
    chars[chars.len() - shown..].iter().collect()
}

/// A keychain in memory, for tests: nothing leaves the process. It can be
/// made to fail like a system without a keychain.
#[derive(Default, Clone)]
pub struct MemoryKeychain {
    pub secret: Arc<Mutex<Option<String>>>,
    pub unavailable: Arc<Mutex<bool>>,
}

impl MemoryKeychain {
    pub fn with_key(key: &str) -> Self {
        let store = Self::default();
        *lock_plain(&store.secret) = Some(key.to_string());
        store
    }

    fn check(&self) -> Result<(), SecretError> {
        if *lock_plain(&self.unavailable) {
            Err(SecretError::Unavailable(
                "no secret service on this bus".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

fn lock_plain<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl SecretStore for MemoryKeychain {
    fn get(&self) -> Result<Option<String>, SecretError> {
        self.check()?;
        Ok(lock_plain(&self.secret).clone())
    }
    fn set(&self, secret: &str) -> Result<(), SecretError> {
        self.check()?;
        *lock_plain(&self.secret) = Some(secret.to_string());
        Ok(())
    }
    fn delete(&self) -> Result<(), SecretError> {
        self.check()?;
        *lock_plain(&self.secret) = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryKeychain as MemoryStore;
    use super::*;

    #[test]
    fn a_key_goes_to_the_keychain_and_only_its_end_is_shown() {
        let store = MemoryStore::default();
        let vault = KeyVault::new(Box::new(store.clone()));
        let empty = vault.status();
        assert!(!empty.set && empty.keychain_available);
        assert_eq!((empty.storage, empty.last4), (Storage::None, None));
        assert_eq!(vault.key(), None);

        let status = vault.set("  ps_live_0123456789abcdef\n").unwrap();
        assert!(status.set);
        assert_eq!(status.storage, Storage::Keychain);
        assert_eq!(status.last4.as_deref(), Some("cdef"));
        assert_eq!(
            store.secret.lock().unwrap().as_deref(),
            Some("ps_live_0123456789abcdef")
        );
        assert_eq!(vault.key().as_deref(), Some("ps_live_0123456789abcdef"));
        // The status the UI gets never holds the key.
        let json = serde_json::to_string(&status).unwrap();
        assert!(!json.contains("0123456789"));
    }

    #[test]
    fn a_stored_key_is_found_again() {
        let store = MemoryStore::default();
        *store.secret.lock().unwrap() = Some("ps_live_0123456789abcdef".to_string());
        let vault = KeyVault::new(Box::new(store));
        let status = vault.status();
        assert!(status.set);
        assert_eq!(status.storage, Storage::Keychain);
        assert_eq!(status.last4.as_deref(), Some("cdef"));
    }

    #[test]
    fn removing_forgets_the_key_everywhere() {
        let store = MemoryStore::default();
        let vault = KeyVault::new(Box::new(store.clone()));
        vault.set("ps_live_0123456789abcdef").unwrap();
        let status = vault.remove().unwrap();
        assert!(!status.set);
        assert_eq!(status.storage, Storage::None);
        assert_eq!(*store.secret.lock().unwrap(), None);
        assert_eq!(vault.key(), None);
        // Removing when nothing is set is fine.
        assert!(!vault.remove().unwrap().set);
    }

    #[test]
    fn without_a_keychain_the_key_lasts_the_session_and_the_status_says_so() {
        let store = MemoryStore::default();
        *store.unavailable.lock().unwrap() = true;
        let vault = KeyVault::new(Box::new(store.clone()));
        let before = vault.status();
        assert!(!before.keychain_available);
        assert!(before.keychain_error.is_some());

        let status = vault.set("ps_live_0123456789abcdef").unwrap();
        assert!(status.set);
        assert_eq!(status.storage, Storage::Session);
        assert!(!status.keychain_available);
        assert_eq!(
            status.keychain_error.as_deref(),
            Some("no secret service on this bus")
        );
        // Usable now, but stored nowhere.
        assert_eq!(vault.key().as_deref(), Some("ps_live_0123456789abcdef"));
        assert_eq!(*store.secret.lock().unwrap(), None);
        // A new start of the app has no key.
        let next_start = KeyVault::new(Box::new(store));
        assert!(!next_start.status().set);
        assert!(!vault.remove().unwrap().set);
    }

    #[test]
    fn bad_keys_are_refused() {
        let vault = KeyVault::new(Box::new(MemoryStore::default()));
        assert_eq!(vault.set("   "), Err(KeyError::Empty));
        assert_eq!(vault.set("two words"), Err(KeyError::Malformed));
        assert_eq!(vault.set("line\nbreak"), Err(KeyError::Malformed));
        assert!(!vault.status().set);
    }

    #[test]
    fn short_keys_show_less() {
        assert_eq!(last4("0123456789abcdef"), "cdef");
        assert_eq!(last4("abcdefgh"), "gh");
        assert_eq!(last4("a"), "a");
    }
}
