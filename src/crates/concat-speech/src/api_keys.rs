// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded API credential rotation. Credentials and provider error bodies are never logged.
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Service pools never share credentials or rotation counters.
#[derive(Clone, Copy)]
pub enum Provider {
    Gemini,
    ElevenLabs,
}
static GEMINI_NEXT: AtomicUsize = AtomicUsize::new(0);
static ELEVEN_NEXT: AtomicUsize = AtomicUsize::new(0);

/// Accept pasted lines, commas or semicolons and ignore duplicate credentials.
pub fn parse(input: &str) -> Vec<&str> {
    let mut keys = Vec::new();
    for key in input.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        if !key.is_empty() && !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}
fn can_rotate(error: &str) -> bool {
    [
        "HTTP 401",
        "HTTP 403",
        "HTTP 429",
        "API_KEY_INVALID",
        "API_KEY_EXPIRED",
    ]
    .iter()
    .any(|code| error.contains(code))
}
/// Keep status and recognized key failure codes, never provider bodies or echoed credentials.
pub(crate) fn http_error(service: &str, error: ureq::Error) -> String {
    use std::io::Read;
    match error {
        ureq::Error::Status(code, response) => {
            let mut bytes = Vec::new();
            let _ = response.into_reader().take(16_384).read_to_end(&mut bytes);
            let marker = serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|value| {
                    value
                        .pointer("/error/details")
                        .and_then(|v| v.as_array())
                        .cloned()
                })
                .and_then(|details| {
                    details.iter().find_map(|detail| {
                        match detail.get("reason").and_then(|v| v.as_str()) {
                            Some("API_KEY_INVALID") => Some("API_KEY_INVALID"),
                            Some("API_KEY_EXPIRED") => Some("API_KEY_EXPIRED"),
                            _ => None,
                        }
                    })
                })
                .unwrap_or("");
            format!(
                "{service} HTTP {code} {marker}. Check key permissions, model/voice access and quota."
            )
        }
        _ => format!("Could not reach {service}. Check your network or retry."),
    }
}
fn redact(mut message: String, keys: &[&str]) -> String {
    for key in keys {
        message = message.replace(key, "[redacted]");
    }
    message
}
fn attempt<T>(
    keys: &[&str],
    start: usize,
    cancel: &AtomicBool,
    mut send: impl FnMut(&str) -> Result<T, String>,
) -> Result<T, String> {
    if keys.is_empty() {
        return Err("Enter an API key in Settings first".into());
    }
    if keys.len() > 32 {
        return Err("Use at most 32 API keys per service".into());
    }
    for offset in 0..keys.len() {
        if cancel.load(Ordering::Relaxed) {
            return Err("Request cancelled".into());
        }
        match send(keys[(start + offset) % keys.len()]) {
            Ok(value) => {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Request cancelled".into());
                }
                return Ok(value);
            }
            Err(error) => {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Request cancelled".into());
                }
                let rotate = can_rotate(&error);
                let safe = redact(error, keys);
                if !rotate {
                    return Err(safe);
                }
                if offset + 1 == keys.len() {
                    return Err(format!(
                        "All {} configured API keys were unavailable. Last error: {safe}",
                        keys.len()
                    ));
                }
            }
        }
    }
    unreachable!()
}
/// Start successive requests on successive keys; try each unique key at most once.
/// Only explicit authentication/permission/rate-limit responses trigger failover.
/// Ambiguous timeouts/network failures are not replayed (they may already be billed).
pub fn with_keys<T>(
    provider: Provider,
    input: &str,
    cancel: &AtomicBool,
    send: impl FnMut(&str) -> Result<T, String>,
) -> Result<T, String> {
    let keys = parse(input);
    let cursor = match provider {
        Provider::Gemini => &GEMINI_NEXT,
        Provider::ElevenLabs => &ELEVEN_NEXT,
    };
    let start = if keys.is_empty() {
        0
    } else {
        cursor.fetch_add(1, Ordering::Relaxed) % keys.len()
    };
    attempt(&keys, start, cancel, send)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_deduplicates_and_rotates_once_per_key() {
        let keys = parse(" alpha; beta\nalpha, gamma ");
        assert_eq!(keys, vec!["alpha", "beta", "gamma"]);
        let mut seen = Vec::new();
        let value = attempt(&keys, 1, &AtomicBool::new(false), |key| {
            seen.push(key.to_owned());
            if key == "alpha" {
                Ok(7)
            } else {
                Err("HTTP 429".into())
            }
        })
        .unwrap();
        assert_eq!(value, 7);
        assert_eq!(seen, vec!["beta", "gamma", "alpha"]);
    }
    #[test]
    fn bounded_errors_are_redacted_and_network_failures_are_not_replayed() {
        let keys = ["secret-one", "secret-two"];
        let mut count = 0;
        let error = attempt::<()>(&keys, 0, &AtomicBool::new(false), |key| {
            count += 1;
            Err(format!("HTTP 401 {key}"))
        })
        .unwrap_err();
        assert_eq!(count, 2);
        assert!(!error.contains("secret-"));
        for message in ["HTTP 400", "HTTP 404", "timed out", "Request cancelled"] {
            count = 0;
            let _ = attempt::<()>(&keys, 0, &AtomicBool::new(false), |_| {
                count += 1;
                Err(message.into())
            });
            assert_eq!(count, 1);
        }
        count = 0;
        let _ = attempt::<()>(&keys, 0, &AtomicBool::new(true), |_| {
            count += 1;
            Ok(())
        });
        assert_eq!(count, 0);
    }
}
