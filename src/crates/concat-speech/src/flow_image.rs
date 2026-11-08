// SPDX-License-Identifier: AGPL-3.0-or-later
//! Gemini reference images, kept separate from Flow render requests.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{io::Read, sync::atomic::AtomicBool, time::Duration};

/// One validated image response, ready to save in the application's asset folder.
pub struct GeneratedImage {
    /// PNG/JPEG/WebP bytes.
    pub bytes: Vec<u8>,
    /// File extension matched to the response MIME and signature.
    pub extension: &'static str,
}
/// Generate a single reference using the configured Gemini key pool and explicit model.
pub fn generate(keys: &str, model: &str, prompt: &str) -> Result<GeneratedImage, String> {
    if model.is_empty()
        || model.len() > 120
        || !model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err("Model tạo ảnh không hợp lệ.".into());
    }
    let body = json!({"contents":[{"role":"user","parts":[{"text":prompt}]}],"generationConfig":{"responseModalities":["TEXT","IMAGE"]}});
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(180))
        .timeout_write(Duration::from_secs(30))
        .build();
    let payload = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
    crate::api_keys::with_keys(
        crate::api_keys::Provider::Gemini,
        keys,
        &AtomicBool::new(false),
        |key| {
            let response=agent.post(&format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"))
            .set("x-goog-api-key",key.trim()).set("Content-Type","application/json").send_bytes(&payload)
            .map_err(|e|crate::api_keys::http_error("Gemini Image",e))?;
            let mut bytes = Vec::new();
            response
                .into_reader()
                .take(48_000_001)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > 48_000_000 {
                return Err("Phản hồi ảnh quá lớn.".into());
            }
            parse(
                &serde_json::from_slice(&bytes)
                    .map_err(|_| "Phản hồi ảnh không hợp lệ.".to_owned())?,
            )
        },
    )
}
fn parse(value: &Value) -> Result<GeneratedImage, String> {
    for part in value
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if part["thought"] == true {
            continue;
        }
        let Some(data) = part.get("inlineData").or_else(|| part.get("inline_data")) else {
            continue;
        };
        let encoded = data["data"].as_str().ok_or("Ảnh không có dữ liệu.")?;
        if encoded.len() > 44_000_000 {
            return Err("Ảnh quá lớn.".into());
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| "Dữ liệu ảnh base64 không hợp lệ.")?;
        let mime = data
            .get("mimeType")
            .or_else(|| data.get("mime_type"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let extension = match mime {
            "image/png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => "png",
            "image/jpeg" if bytes.starts_with(&[0xff, 0xd8, 0xff]) => "jpg",
            "image/webp" if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") => {
                "webp"
            }
            _ => return Err("Định dạng ảnh không được hỗ trợ.".into()),
        };
        return Ok(GeneratedImage { bytes, extension });
    }
    Err("Gemini chưa trả ảnh. Kiểm tra model, hạn mức và quyền tạo ảnh của key.".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flow_image_rejects_text_and_mismatched_signatures() {
        assert!(
            parse(&json!({"candidates":[{"content":{"parts":[{"text":"no image"}]}}]})).is_err()
        );
        assert!(parse(&json!({"candidates":[{"content":{"parts":[{"inlineData":{"mimeType":"image/png","data":STANDARD.encode(b"not an image")}}]}}]})).is_err());
        let value = json!({"candidates":[{"content":{"parts":[{"thought":true,"inlineData":{"mimeType":"image/png","data":"invalid"}},{"inlineData":{"mimeType":"image/png","data":STANDARD.encode(b"\x89PNG\r\n\x1a\nfixture")}}]}}]});
        assert_eq!(parse(&value).unwrap().extension, "png");
    }
}
