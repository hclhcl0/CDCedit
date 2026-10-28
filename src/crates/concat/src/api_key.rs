// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared service credentials. Windows persists DPAPI ciphertext for the current user.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Default)]
pub struct ApiKey(String);
impl ApiKey {
    pub fn new(value: String) -> Self {
        Self(value.trim().to_owned())
    }
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
}
// Other platforms use the account-private settings file (same policy as the server token).
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
    // DPAPI borrows input and allocates output; UI_FORBIDDEN keeps saving non-interactive.
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
