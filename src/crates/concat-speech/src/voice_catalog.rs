//! Provider voice metadata; unknown attributes are never guessed.
use serde_json::Value;
use std::{io::Read, time::Duration};

/// A provider voice and its published labels.
#[derive(Clone, Debug)]
pub struct Voice {
    /// Synthesis identifier.
    pub id: String,
    /// Display name and labels.
    pub name: String,
    /// 0 unknown, 1 male, 2 female.
    pub gender: i32,
    /// 0 unknown, 1 north, 2 central, 3 south.
    pub region: i32,
    /// Explicit compatible model list; empty means unspecified.
    pub models: Vec<String>,
    /// Provider indicates free-plan/default access.
    pub free: bool,
}
impl Voice {
    /// Match model, gender and free-plan filters.
    pub fn matches(&self, model: &str, gender: i32, free: bool) -> bool {
        (gender == 0 || self.gender == gender)
            && (!free || self.free)
            && (self.models.is_empty() || self.models.iter().any(|m| m == model))
    }
}
/// Prebuilt studio voices shared by the supported Gemini TTS models.
pub fn gemini() -> Vec<Voice> {
    [
        ("Kore", 2),
        ("Aoede", 2),
        ("Leda", 2),
        ("Zephyr", 2),
        ("Puck", 1),
        ("Charon", 1),
        ("Fenrir", 1),
        ("Orus", 1),
        ("Callirrhoe", 0),
        ("Autonoe", 0),
        ("Enceladus", 0),
        ("Iapetus", 0),
        ("Umbriel", 0),
        ("Algieba", 0),
        ("Despina", 0),
        ("Erinome", 0),
        ("Algenib", 0),
        ("Rasalgethi", 0),
        ("Laomedeia", 0),
        ("Achernar", 0),
        ("Alnilam", 0),
        ("Schedar", 0),
        ("Gacrux", 0),
        ("Pulcherrima", 0),
        ("Achird", 0),
        ("Zubenelgenubi", 0),
        ("Vindemiatrix", 0),
        ("Sadachbia", 0),
        ("Sadaltager", 0),
        ("Sulafat", 0),
    ]
    .into_iter()
    .map(|(id, gender)| Voice {
        id: id.into(),
        name: id.into(),
        gender,
        region: 0,
        models: crate::cloud_tts::GEMINI_MODELS
            .iter()
            .map(|m| (*m).into())
            .collect(),
        free: false,
    })
    .collect()
}
/// Parse a catalog without interpreting missing permission flags as free access.
pub fn parse_eleven(value: &Value) -> Vec<Voice> {
    value["voices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| {
            let id = v["voice_id"].as_str()?;
            let labels = &v["labels"];
            let gender = match labels["gender"].as_str().unwrap_or("") {
                "male" => 1,
                "female" => 2,
                _ => 0,
            };
            let category = v["category"].as_str().unwrap_or("");
            let free = if let Some(allowed) = v["sharing"]["free_users_allowed"].as_bool() {
                allowed
            } else {
                if let Some(tiers) = v["available_for_tiers"]
                    .as_array()
                    .filter(|a| !a.is_empty())
                {
                    tiers.iter().any(|t| t.as_str() == Some("free"))
                } else {
                    matches!(category, "premade" | "default")
                }
            };
            let region = match labels["accent"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .as_str()
            {
                "northern vietnamese" | "hanoi" | "hà nội" | "bắc" => 1,
                "central vietnamese" | "hue" | "huế" | "trung" => 2,
                "southern vietnamese" | "saigon" | "sài gòn" | "nam" => 3,
                _ => 0,
            };
            let tags = ["language", "accent", "gender", "use_case", "descriptive"]
                .iter()
                .filter_map(|k| labels[*k].as_str())
                .collect::<Vec<_>>()
                .join(" · ");
            Some(Voice {
                id: id.into(),
                name: format!(
                    "{}{}{}",
                    v["name"].as_str().unwrap_or(id),
                    if tags.is_empty() { "" } else { " · " },
                    tags
                ),
                gender,
                region,
                models: Vec::new(),
                free,
            })
        })
        .collect()
}
/// Fetch account voices on a worker, with bounded pages and response sizes.
pub fn eleven(keys: &str) -> Result<Vec<Voice>, String> {
    crate::api_keys::with_keys(
        crate::api_keys::Provider::ElevenLabs,
        keys,
        &std::sync::atomic::AtomicBool::new(false),
        eleven_one,
    )
}
fn eleven_one(key: &str) -> Result<Vec<Voice>, String> {
    if key.trim().is_empty() {
        return Err("Configure the ElevenLabs API key in Settings first.".into());
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20))
        .build();
    let mut voices = Vec::new();
    let mut next = String::new();
    for _ in 0..20 {
        let mut req = agent
            .get("https://api.elevenlabs.io/v2/voices")
            .set("xi-api-key", key)
            .query("page_size", "100");
        if !next.is_empty() {
            req = req.query("next_page_token", &next);
        }
        let response = req
            .call()
            .map_err(|error| crate::api_keys::http_error("Voice catalog", error))?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(4 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .map_err(|_| "Could not read voice catalog.")?;
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid voice catalog response.")?;
        voices.extend(parse_eleven(&value));
        if value["has_more"].as_bool() != Some(true) {
            return Ok(voices);
        }
        let token = value["next_page_token"].as_str().unwrap_or("");
        if token.is_empty() || token == next {
            return Err("Incomplete voice catalog. Try again.".into());
        }
        next = token.into();
    }
    Err("Voice catalog exceeded the page limit.".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permission_and_unknown_labels_are_conservative() {
        let voices = parse_eleven(&serde_json::json!({"voices":[
            {"voice_id":"a","category":"premade","sharing":{"free_users_allowed":false}},
            {"voice_id":"b","labels":{"gender":"female","accent":"southern vietnamese"},"sharing":{"free_users_allowed":true}},
            {"voice_id":"c"},
            {"voice_id":"d","category":"premade","available_for_tiers":["creator"]}]}));
        assert!(!voices[0].free);
        assert!(voices[1].matches("eleven_v3", 2, true));
        assert_eq!(voices[1].region, 3);
        assert!(!voices[1].matches("eleven_v3", 1, true));
        assert!(!voices[3].free);
        assert_eq!(voices[2].region, 0);
        assert!(!voices[2].matches("eleven_v3", 0, true));
    }
}
