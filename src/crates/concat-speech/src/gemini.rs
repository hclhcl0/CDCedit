// SPDX-License-Identifier: AGPL-3.0-or-later
//! Optional Google Gemini Live transcription. Credentials stay in memory.
use crate::{
    JobStage,
    transcribe::{Segment, TranscribeRequest},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use concat_media::{AudioDecoder, AudioOptions, SampleFormat};
use serde_json::{Value, json};
use std::{
    net::{TcpStream, ToSocketAddrs},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, client::IntoClientRequest, stream::MaybeTlsStream};

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;
const RATE: usize = 16_000;
const WINDOW: usize = RATE * 6;

fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Transcription cancelled".into())
    } else {
        Ok(())
    }
}
fn send(socket: &mut Socket, value: Value) -> Result<(), String> {
    socket
        .send(Message::Text(value.to_string().into()))
        .map_err(|_| "Gemini connection lost while sending audio".into())
}
fn receive(socket: &mut Socket, cancel: &AtomicBool, deadline: Instant) -> Result<Value, String> {
    loop {
        cancelled(cancel)?;
        if Instant::now() >= deadline {
            return Err("Gemini response timed out. No captions were added.".into());
        }
        match socket.read() {
            Ok(Message::Text(text)) => return parse_response(text.as_bytes()),
            Ok(Message::Binary(bytes)) => return parse_response(&bytes),
            Ok(Message::Close(_)) => {
                return Err(
                    "Gemini closed the session. Check your API key, model access and quota.".into(),
                );
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => {
                return Err("Gemini connection failed. Check your network and API access.".into());
            }
        }
    }
}
fn parse_response(bytes: &[u8]) -> Result<Value, String> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "Invalid Gemini response")?;
    if value.get("error").is_some() {
        return Err(
            "Gemini rejected the request. Check your API key, model access and quota.".into(),
        );
    }
    Ok(value)
}
// Transcribe Live's inputTranscription is already final. It is independent of
// conversational turnComplete, which may be absent or arrive before the text.
fn receive_transcript(
    socket: &mut Socket,
    cancel: &AtomicBool,
    deadline: Instant,
) -> Result<String, String> {
    let mut completed = None;
    loop {
        let until = completed.map_or(deadline, |at: Instant| {
            deadline.min(at + Duration::from_secs(5))
        });
        let value = match receive(socket, cancel, until) {
            Ok(value) => value,
            Err(error) if completed.is_some() && error.contains("timed out") => {
                return Ok(String::new());
            }
            Err(error) => return Err(error),
        };
        if let Some(text) = final_text(&value) {
            return Ok(text.trim().to_owned());
        }
        if value
            .pointer("/serverContent/turnComplete")
            .and_then(Value::as_bool)
            == Some(true)
        {
            completed = Some(Instant::now());
        }
    }
}
fn final_text(value: &Value) -> Option<&str> {
    value
        .pointer("/serverContent/inputTranscription/text")
        .and_then(Value::as_str)
}
fn pcm(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| ((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect()
}

/// Clip-relative six-second cues. Live transcription does not supply word timestamps.
/// Only finalized text is committed; any failed window aborts the entire batch.
pub fn transcribe(
    request: &TranscribeRequest,
    key: &str,
    vietnamese: bool,
    cancel: &AtomicBool,
    mut progress: impl FnMut(JobStage),
) -> Result<Vec<Segment>, String> {
    if key.trim().is_empty() {
        return Err("Enter your Gemini API key first".into());
    }
    cancelled(cancel)?;
    progress(JobStage::Reading);
    let mut decoder = AudioDecoder::open(
        &request.path,
        &AudioOptions {
            start: Some(request.source_start),
            duration: Some(request.window),
            filters: Vec::new(),
            rate: RATE as u32,
            channels: 1,
            format: SampleFormat::F32,
            stream: request.audio_stream.map(|index| index as usize),
        },
    )
    .map_err(|e| e.to_string())?;
    let samples = decoder.collect_f32().map_err(|e| e.to_string())?;
    if samples.is_empty() {
        return Err("The clip has no audio".into());
    }
    cancelled(cancel)?;
    progress(JobStage::Loading);
    let mut handshake = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent"
        .into_client_request().map_err(|_| "Could not prepare Gemini connection")?;
    handshake.headers_mut().insert(
        "x-goog-api-key",
        key.trim().parse().map_err(|_| "Invalid API key")?,
    );
    let addresses = ("generativelanguage.googleapis.com", 443)
        .to_socket_addrs()
        .map_err(|_| "Could not resolve Gemini server")?;
    let mut connection = None;
    for address in addresses.take(4) {
        cancelled(cancel)?;
        if let Ok(stream) = TcpStream::connect_timeout(&address, Duration::from_secs(5)) {
            connection = Some(stream);
            break;
        }
    }
    let stream = connection.ok_or("Could not connect to Gemini. Check your network.")?;
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .map_err(|_| "Could not set handshake timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(15)))
        .map_err(|_| "Could not set handshake timeout")?;
    let (mut socket, _) = tungstenite::client_tls_with_config(handshake, stream, None, None)
        .map_err(
            |_| "Could not connect to Gemini. Check your API key, network and model access.",
        )?;
    match socket.get_mut() {
        MaybeTlsStream::NativeTls(stream) => {
            stream
                .get_mut()
                .set_read_timeout(Some(Duration::from_millis(250)))
                .map_err(|_| "Could not set Gemini timeout")?;
            stream
                .get_mut()
                .set_write_timeout(Some(Duration::from_secs(15)))
                .map_err(|_| "Could not set Gemini timeout")?;
        }
        _ => return Err("Gemini requires a secure TLS connection".into()),
    }
    send(
        &mut socket,
        json!({"setup": {
            "model": "models/gemini-3.5-transcribe-live",
            "generationConfig": {"responseModalities": ["TEXT"]},
            "realtimeInputConfig": {"automaticActivityDetection": {"disabled": true}},
            "inputAudioTranscription": {"languageCodes": if vietnamese { vec!["vi-VN"] } else { vec![] }}
        }}),
    )?;
    let deadline = Instant::now() + Duration::from_secs(30);
    while receive(&mut socket, cancel, deadline).map_err(|error| {
        if error.contains("timed out") { "Gemini did not acknowledge session setup within 30 seconds. Check your network and model access.".to_owned() } else {error}
    })?
        .get("setupComplete")
        .is_none()
    {}
    let mut segments = Vec::new();
    let count = samples.len().div_ceil(WINDOW);
    for (index, window) in samples.chunks(WINDOW).enumerate() {
        cancelled(cancel)?;
        progress(JobStage::Working {
            done: index as f32 / count as f32,
            next: (index + 1) as f32 / count as f32,
        });
        send(&mut socket, json!({"realtimeInput": {"activityStart": {}}}))?;
        for chunk in window.chunks(RATE / 10) {
            cancelled(cancel)?;
            send(
                &mut socket,
                json!({"realtimeInput": {"audio": {"mimeType": "audio/pcm;rate=16000", "data": STANDARD.encode(pcm(chunk))}}}),
            )?;
            std::thread::sleep(Duration::from_millis(100));
        }
        send(&mut socket, json!({"realtimeInput": {"activityEnd": {}}}))?;
        let deadline = Instant::now() + Duration::from_secs(60);
        let text = receive_transcript(&mut socket, cancel, deadline)?;
        if !text.trim().is_empty() {
            segments.push(Segment {
                start: (index * WINDOW) as f64 / RATE as f64,
                end: (index * WINDOW + window.len()) as f64 / RATE as f64,
                text: text.trim().to_owned(),
            });
        }
    }
    cancelled(cancel)?;
    let _ = socket.close(None);
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interim_text_is_never_committed() {
        assert_eq!(
            final_text(&json!({"serverContent":{"interimInputTranscription":{"text":"draft"}}})),
            None
        );
        assert_eq!(
            final_text(&json!({"serverContent":{"inputTranscription":{"text":"Xin chào"}}})),
            Some("Xin chào")
        );
    }
    #[test]
    fn pcm_is_clamped_signed_little_endian() {
        assert_eq!(pcm(&[-2.0, 0.0, 2.0]), [1, 128, 0, 0, 255, 127]);
    }
    #[test]
    fn websocket_finalization_ignores_interim_and_handles_close() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            for value in [
                json!({"serverContent":{"interimInputTranscription":{"text":"draft"}}}),
                json!({"serverContent":{"inputTranscription":{"text":"Final"},"turnComplete":true}}),
            ] {
                socket
                    .send(Message::Text(value.to_string().into()))
                    .unwrap();
            }
            let _ = socket.close(None);
        });
        let stream = TcpStream::connect(address).unwrap();
        let (mut socket, _) =
            tungstenite::client(format!("ws://{address}"), MaybeTlsStream::Plain(stream)).unwrap();
        let cancel = AtomicBool::new(false);
        let deadline = Instant::now() + Duration::from_secs(5);
        assert!(final_text(&receive(&mut socket, &cancel, deadline).unwrap()).is_none());
        let final_event = receive(&mut socket, &cancel, deadline).unwrap();
        assert_eq!(final_text(&final_event), Some("Final"));
        assert!(receive(&mut socket, &cancel, deadline).is_err());
        server.join().unwrap();
    }
    #[test]
    fn no_key_or_cancel_never_opens_media_or_network() {
        let request = TranscribeRequest {
            path: "does-not-exist".into(),
            audio_stream: None,
            source_start: 0.0,
            window: 2.0,
            model_id: "gemini-3.5-transcribe-live".into(),
        };
        let cancel = AtomicBool::new(false);
        assert!(
            transcribe(&request, "", true, &cancel, |_| {})
                .unwrap_err()
                .contains("API key")
        );
        cancel.store(true, Ordering::Relaxed);
        assert!(
            transcribe(&request, "test", true, &cancel, |_| {})
                .unwrap_err()
                .contains("cancelled")
        );
    }
    #[test]
    fn binary_setup_and_final_transcript_without_turn_complete_are_accepted() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            for value in [
                json!({"setupComplete":{}}),
                json!({"serverContent":{"interimInputTranscription":{"text":"draft"}}}),
                json!({"serverContent":{"inputTranscription":{"text":"Xin chào Việt Nam"}}}),
            ] {
                ws.send(Message::Binary(value.to_string().into_bytes().into()))
                    .unwrap();
            }
            let _ = ws.read();
        });
        let (stream, _) = tungstenite::client(
            format!("ws://{address}"),
            MaybeTlsStream::Plain(TcpStream::connect(address).unwrap()),
        )
        .unwrap();
        let mut ws = stream;
        let cancel = AtomicBool::new(false);
        let deadline = Instant::now() + Duration::from_secs(2);
        assert!(
            receive(&mut ws, &cancel, deadline)
                .unwrap()
                .get("setupComplete")
                .is_some()
        );
        assert_eq!(
            receive_transcript(&mut ws, &cancel, deadline).unwrap(),
            "Xin chào Việt Nam"
        );
        let _ = ws.close(None);
        server.join().unwrap();
    }
    #[test]
    fn transcript_can_arrive_after_turn_complete() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            ws.send(Message::Text(
                json!({"serverContent":{"turnComplete":true}})
                    .to_string()
                    .into(),
            ))
            .unwrap();
            ws.send(Message::Binary(
                json!({"serverContent":{"inputTranscription":{"text":"Final"}}})
                    .to_string()
                    .into_bytes()
                    .into(),
            ))
            .unwrap();
            let _ = ws.read();
        });
        let (mut ws, _) = tungstenite::client(
            format!("ws://{address}"),
            MaybeTlsStream::Plain(TcpStream::connect(address).unwrap()),
        )
        .unwrap();
        assert_eq!(
            receive_transcript(
                &mut ws,
                &AtomicBool::new(false),
                Instant::now() + Duration::from_secs(2)
            )
            .unwrap(),
            "Final"
        );
        let _ = ws.close(None);
        server.join().unwrap();
    }
}
