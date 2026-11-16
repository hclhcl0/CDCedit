// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared service credentials protected per user account.
//!
//! On Windows, secrets are encrypted with DPAPI (`CryptProtectData` /
//! `CryptUnprotectData`) and encoded with a `"dpapi:"` prefix; on other
//! platforms, keys are kept under `"local:"` in the user's private
//! `settings.json` file. [`ApiKey`] never exposes raw secrets in `Debug`
//! output or serialized JSON.

use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::dirs::AppDirs;

const SETTINGS_FILE: &str = "settings.json";

/// A stored service API key (single key or `;`/`,`-separated key rotation list).
#[derive(Clone, Default)]
pub struct ApiKey(String);

impl ApiKey {
    /// Wraps `value`, trimming surrounding whitespace.
    pub fn new(value: String) -> Self {
        Self(value.trim().to_owned())
    }

    /// Returns the plaintext key string for authorized API requests.
    /// Callers must never log or serialize the returned slice.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey([redacted])")
    }
}

impl Serialize for ApiKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0.is_empty() {
            return serializer.serialize_str("");
        }
        let bytes = protect(self.0.as_bytes(), false).map_err(serde::ser::Error::custom)?;
        let encoded: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        serializer.serialize_str(&format!(
            "{}:{encoded}",
            if cfg!(windows) { "dpapi" } else { "local" }
        ))
    }
}

impl<'de> Deserialize<'de> for ApiKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let encoded = String::deserialize(deserializer)?;
        let prefix = if cfg!(windows) { "dpapi:" } else { "local:" };
        let value = encoded
            .strip_prefix(prefix)
            .and_then(|hex| {
                if hex.len() % 2 != 0 || !hex.is_ascii() {
                    return None;
                }
                let bytes = (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
                    .collect::<Result<Vec<_>, _>>()
                    .ok()?;
                String::from_utf8(protect(&bytes, true).ok()?).ok()
            })
            .unwrap_or_default();
        Ok(Self(value))
    }
}

/// Encrypted service credentials loaded from `settings.json` for backend callers
/// without deserializing UI preferences.
#[derive(Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ServiceKeys {
    /// Gemini API key(s), protected per user.
    pub gemini_api_key: ApiKey,
    /// ElevenLabs API key(s), protected per user.
    pub eleven_api_key: ApiKey,
}

impl std::fmt::Debug for ServiceKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceKeys")
            .field("gemini_api_key", &self.gemini_api_key)
            .field("eleven_api_key", &self.eleven_api_key)
            .finish()
    }
}

impl ServiceKeys {
    /// Loads saved credentials from `<dirs.config>/settings.json`, returning
    /// empty defaults if missing or unreadable.
    pub fn load(dirs: &AppDirs) -> Self {
        Self::load_from(&dirs.config.join(SETTINGS_FILE))
    }

    /// Loads saved credentials from an explicit `settings.json` file path.
    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    /// Scrubs any configured secret tokens from error or diagnostic strings.
    pub fn redact(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for raw in [self.gemini_api_key.expose(), self.eleven_api_key.expose()] {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                out = out.replace(trimmed, "[redacted]");
            }
            for token in raw.split([';', ',', '\n', '\r']) {
                let token = token.trim();
                if token.len() >= 4 {
                    out = out.replace(token, "[redacted]");
                }
            }
        }
        out
    }
}

#[cfg(not(windows))]
fn protect(bytes: &[u8], _: bool) -> Result<Vec<u8>, &'static str> {
    Ok(bytes.to_vec())
}

#[cfg(windows)]
fn protect(bytes: &[u8], decrypt: bool) -> Result<Vec<u8>, &'static str> {
    use std::{ffi::c_void, ptr};
    #[repr(C)]
    struct Blob {
        len: u32,
        data: *mut u8,
    }
    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            input: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }
    let input = Blob {
        len: bytes.len().try_into().map_err(|_| "API key is too long")?,
        data: bytes.as_ptr() as *mut u8,
    };
    let mut output = Blob {
        len: 0,
        data: ptr::null_mut(),
    };
    unsafe {
        let ok = if decrypt {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                1,
                &mut output,
            )
        } else {
            CryptProtectData(
                &input,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                1,
                &mut output,
            )
        };
        if ok == 0 {
            return Err("Could not protect the API key for this Windows account");
        }
        let result = if output.len == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(output.data, output.len as usize).to_vec()
        };
        LocalFree(output.data.cast());
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_round_trip_without_exposing_debug_or_json() {
        let key = ApiKey::new("  test-secret-for-roundtrip  ".into());
        let json = serde_json::to_string(&key).unwrap();
        assert!(!json.contains("test-secret-for-roundtrip"));
        assert!(!format!("{key:?}").contains("test-secret-for-roundtrip"));
        assert_eq!(
            serde_json::from_str::<ApiKey>(&json).unwrap().expose(),
            key.expose()
        );
        assert!(
            serde_json::from_str::<ApiKey>("\"dpapi:invalid\"")
                .unwrap()
                .expose()
                .is_empty()
        );
    }

    #[test]
    fn service_keys_load_from_settings_json_and_redact_errors() {
        let scratch = tempfile::tempdir().expect("scratch");
        let dirs = AppDirs {
            config: scratch.path().join("config"),
            data: scratch.path().join("data"),
        };
        std::fs::create_dir_all(&dirs.config).expect("config dir");
        let gem = ApiKey::new("gem-secret-alpha;gem-secret-beta".to_owned());
        let elev = ApiKey::new("eleven-secret-gamma".to_owned());
        let doc = serde_json::json!({
            "geminiApiKey": serde_json::to_value(&gem).expect("ser"),
            "elevenApiKey": serde_json::to_value(&elev).expect("ser"),
            "unrelatedField": 123,
        });
        std::fs::write(dirs.config.join(SETTINGS_FILE), doc.to_string()).expect("write");

        let loaded = ServiceKeys::load(&dirs);
        assert_eq!(
            loaded.gemini_api_key.expose(),
            "gem-secret-alpha;gem-secret-beta"
        );
        assert_eq!(loaded.eleven_api_key.expose(), "eleven-secret-gamma");
        let debug = format!("{loaded:?}");
        assert!(!debug.contains("gem-secret"));
        assert!(!debug.contains("eleven-secret"));

        let cleaned =
            loaded.redact("HTTP 403 failed with key gem-secret-beta or eleven-secret-gamma");
        assert!(!cleaned.contains("gem-secret-beta"));
        assert!(!cleaned.contains("eleven-secret-gamma"));
        assert!(cleaned.contains("[redacted]"));
    }
}
