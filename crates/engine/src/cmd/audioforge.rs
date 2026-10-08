//! Import an auditioned AudioForge take through the normal undo boundary.
use super::*;
use crate::cmd;
use soundcraft_model::{Generation, MusicGeneration, SourceId};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "audioforge.insert",
            "Insert Voice Take",
            [],
            None,
            "{path: WAV, input: text, voice: id, speed?: 1.0, at?: position, track?: id|name}",
            always,
            insert
        ),
        cmd!(
            "musicforge.insert",
            "Insert Music Take",
            [],
            None,
            "{path: WAV, input: prompt, duration?: 30, bpm?: 100, seed?: 42, model?: acestep-v15-turbo, at?: position, track?: id|name}",
            always,
            insert_music
        ),
    ]
}

fn insert(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "audioforge.insert";
    let path = str_param(p, "path").ok_or_else(|| bad(id, "path required"))?;
    let text = str_param(p, "input").ok_or_else(|| bad(id, "input required"))?;
    let voice = str_param(p, "voice").ok_or_else(|| bad(id, "voice required"))?;
    let speed = p.get("speed").and_then(Value::as_f64).unwrap_or(1.0);
    if text.trim().is_empty()
        || text.chars().count() > 5000
        || voice.is_empty()
        || voice.len() > 80
        || !speed.is_finite()
        || !(0.5..=2.0).contains(&speed)
    {
        return Err(bad(id, "invalid narration settings"));
    }
    let at = position_param(e, id, p, "at")?.unwrap_or(0).max(0);
    let target = track_param(e, id, p, "track")?;
    let file = std::fs::File::open(path).map_err(|err| EngineError::Io(err.to_string()))?;
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(25 * 1024 * 1024 + 1).read_to_end(&mut bytes).map_err(|err| EngineError::Io(err.to_string()))?;
    if bytes.len() > 25 * 1024 * 1024 || !bytes.starts_with(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(bad(id, "take must be a WAV no larger than 25 MB"));
    }
    let absolute = std::fs::canonicalize(path).map_err(|err| EngineError::Io(err.to_string()))?;
    let result = crate::io::import_audio_bytes(e, "Voice take.wav", &bytes, Some(&absolute.to_string_lossy()), target, at)?;
    if let Some(source_id) = result.get("source").and_then(Value::as_u64)
        && let Some(source) = e.session_mut().sources.iter_mut().find(|s| s.id == SourceId(source_id))
    {
        source.name = format!("Voice take {}", source.id.0);
        source.generation = Some(Generation { music: None, provider: "audioforge-kokoro".into(), input: text.into(), voice: voice.into(), speed });
    }
    Ok(result)
}

fn insert_music(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "musicforge.insert";
    let prompt = str_param(p, "input").ok_or_else(|| bad(id, "input required"))?;
    for field in ["duration", "bpm", "seed"] {
        if p.get(field).is_some_and(|v| if field == "duration" { v.as_f64().is_none() } else { v.as_u64().is_none() }) {
            return Err(bad(id, "invalid numeric music setting"));
        }
    }
    let duration = p.get("duration").and_then(Value::as_f64).unwrap_or(30.0);
    let bpm = p.get("bpm").and_then(Value::as_u64).unwrap_or(100);
    let seed = p.get("seed").and_then(Value::as_u64).unwrap_or(42);
    let model = str_param(p, "model").unwrap_or("acestep-v15-turbo");
    if prompt.trim().is_empty()
        || prompt.chars().count() > 2000
        || !duration.is_finite()
        || !(10.0..=60.0).contains(&duration)
        || !(30..=300).contains(&bpm)
        || seed > 2147483647
        || model != "acestep-v15-turbo"
    {
        return Err(bad(id, "invalid instrumental settings"));
    }
    let mut voice_params = p.clone();
    voice_params["voice"] = serde_json::json!("instrumental");
    voice_params["speed"] = serde_json::json!(1.0);
    let result = insert(e, &voice_params)?;
    if let Some(source_id) = result.get("source").and_then(Value::as_u64)
        && let Some(source) = e.session_mut().sources.iter_mut().find(|s| s.id == SourceId(source_id))
    {
        source.name = format!("Music take {}", source.id.0);
        source.generation = Some(Generation {
            music: Some(MusicGeneration { model: model.into(), duration, bpm: bpm as u32, seed }),
            provider: "ace-step-local".into(),
            input: prompt.into(),
            voice: String::new(),
            speed: 1.0,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_settings_without_changing_session() {
        let mut engine = Engine::default();
        let before = engine.session().to_json().unwrap();
        assert!(engine.execute("audioforge.insert", &serde_json::json!({"path":"missing.wav","input":"","voice":"af_heart"})).is_err());
        assert_eq!(before, engine.session().to_json().unwrap());
    }
    #[test]
    fn insertion_preserves_settings_through_save_and_undo() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("soundcraft-tts-test-{stamp}.wav"));
        let buffer = soundcraft_audio_io::AudioBuffer { sample_rate: 24000, channels: vec![vec![0.0; 240]] };
        let bytes = soundcraft_audio_io::encode(&buffer, &soundcraft_audio_io::EncodeOptions::default()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let mut engine = Engine::default();
        engine.execute("audioforge.insert", &serde_json::json!({"path":path,"input":"Hello","voice":"af_heart","speed":1.2,"at":48000})).unwrap();
        let stored = soundcraft_model::Session::from_json(&engine.session().to_json().unwrap()).unwrap();
        let settings = stored.sources[0].generation.as_ref().unwrap();
        assert_eq!(settings.input, "Hello");
        assert_eq!(settings.voice, "af_heart");
        assert_eq!(settings.speed, 1.2);
        assert_eq!(stored.tracks[0].playlist().unwrap().clips[0].start, 48000);
        assert!(engine.undo());
        assert!(engine.session().sources.is_empty());
        assert!(engine.session().tracks.is_empty());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn music_metadata_roundtrip_and_undo() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("soundcraft-music-{stamp}.wav"));
        let buffer = soundcraft_audio_io::AudioBuffer { sample_rate: 48000, channels: vec![vec![0.1; 480], vec![0.1; 480]] };
        std::fs::write(&path, soundcraft_audio_io::encode(&buffer, &soundcraft_audio_io::EncodeOptions::default()).unwrap()).unwrap();
        let mut engine = Engine::default();
        for invalid in
            [serde_json::json!({"seed":-1}), serde_json::json!({"bpm":301}), serde_json::json!({"duration":61}), serde_json::json!({"seed":"bad"})]
        {
            let mut params = serde_json::json!({"path":path,"input":"Ambient instrumental"});
            for (key, value) in invalid.as_object().unwrap() {
                params[key] = value.clone();
            }
            assert!(engine.execute("musicforge.insert", &params).is_err());
            assert!(engine.session().tracks.is_empty());
        }
        engine
            .execute(
                "musicforge.insert",
                &serde_json::json!({"path":path,"input":"Ambient instrumental","duration":20,"bpm":90,"seed":123,"at":48000}),
            )
            .unwrap();
        let saved = soundcraft_model::Session::from_json(&engine.session().to_json().unwrap()).unwrap();
        let metadata = saved.sources[0].generation.as_ref().unwrap();
        assert_eq!(metadata.provider, "ace-step-local");
        let settings = metadata.music.as_ref().unwrap();
        assert_eq!(settings.seed, 123);
        assert_eq!(settings.bpm, 90);
        assert_eq!(settings.duration, 20.0);
        assert!(engine.undo());
        assert!(engine.session().sources.is_empty());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn two_takes_save_as_distinct_audio_files() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let folder = std::env::temp_dir().join(format!("soundcraft-two-takes-{stamp}"));
        std::fs::create_dir(&folder).unwrap();
        let path = folder.join("input.wav");
        let mut engine = Engine::default();
        for (text, sample) in [("First", 0.1), ("Second", 0.2)] {
            let buffer = soundcraft_audio_io::AudioBuffer { sample_rate: 24000, channels: vec![vec![sample; 240]] };
            std::fs::write(&path, soundcraft_audio_io::encode(&buffer, &soundcraft_audio_io::EncodeOptions::default()).unwrap()).unwrap();
            engine.execute("audioforge.insert", &serde_json::json!({"path":path,"input":text,"voice":"af_heart"})).unwrap();
        }
        crate::io::save_session(&mut engine, folder.join("session.scraft").to_str().unwrap()).unwrap();
        let first = folder.join(&engine.session().sources[0].path);
        let second = folder.join(&engine.session().sources[1].path);
        assert_ne!(first, second);
        assert_ne!(std::fs::read(&first).unwrap(), std::fs::read(&second).unwrap());
        // Remove only files created by this test.
        for item in [first, second, path, folder.join("session.scraft")] {
            std::fs::remove_file(item).unwrap();
        }
        std::fs::remove_dir(folder.join("Audio Files")).unwrap();
        std::fs::remove_dir(folder).unwrap();
    }
}
