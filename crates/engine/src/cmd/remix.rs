//! Lyrics, aligned stem import, arrangement sections and portable remix exports.
use super::*;
use crate::cmd;
use serde_json::json;
use soundcraft_model::{LyricVersion, MarkerKind};
use std::io::Read;
use std::path::Path;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("lyrics.save_version", "Save Lyric Version", [], None, "{name,text,lead?,backing?}; up to 32 immutable revisions", always, save_lyrics),
        cmd!(query "lyrics.versions", "List Lyric Versions", [], None, "{}", always, |e, _| Ok(json!(e.session().lyric_versions))),
        cmd!(
            "remix.import_stems",
            "Import Aligned Stems",
            [],
            None,
            "{stems:[{path,name,estimated?:bool}],at?:position}; 1..8 WAV files",
            always,
            import_stems
        ),
        cmd!("remix.section", "Add Arrangement Section", [], None, "{name,start:position,end:position}", always, section),
        cmd!("remix.loop_section", "Loop Arrangement Section", [], None, "{number}", always, loop_section),
        cmd!(noundo "remix.export", "Export Remix Project", [], None, "{dir}; new folder containing project, mix, track stems and manifest; up to 10 minutes", has_tracks, export),
    ]
}
fn limited(p: &Value, field: &str, cap: usize) -> Result<String> {
    let value = str_param(p, field).ok_or_else(|| bad("remix", format!("{field} required")))?;
    if value.trim().is_empty() || value.chars().count() > cap {
        return Err(bad("remix", format!("{field} must have 1..{cap} characters")));
    }
    Ok(value.into())
}
fn save_lyrics(e: &mut Engine, p: &Value) -> Result<Value> {
    let name = limited(p, "name", 80)?;
    let text = limited(p, "text", 3000)?;
    let lead = p.get("lead").cloned().unwrap_or(json!("Female lead"));
    let backing = p.get("backing").cloned().unwrap_or(json!("Male backing shouts"));
    let roles = json!({"lead":lead,"backing":backing});
    let lead = limited(&roles, "lead", 120)?;
    let backing = limited(&roles, "backing", 120)?;
    if e.session().lyric_versions.len() >= 32 {
        return Err(bad("lyrics.save_version", "Project already has 32 lyric versions"));
    }
    if e.session().lyric_versions.iter().any(|v| v.name == name) {
        return Err(bad("lyrics.save_version", "Use a new revision name"));
    }
    e.session_mut().lyric_versions.push(LyricVersion { name, text, lead, backing });
    Ok(json!({"index":e.session().lyric_versions.len().saturating_sub(1)}))
}
fn import_stems(e: &mut Engine, p: &Value) -> Result<Value> {
    let stems = p
        .get("stems")
        .and_then(Value::as_array)
        .filter(|s| !s.is_empty() && s.len() <= 8)
        .ok_or_else(|| bad("remix.import_stems", "Supply 1..8 stems"))?;
    let generation = match p.get("generation").filter(|v| !v.is_null()) {
        Some(value) => {
            let meta: soundcraft_model::Generation =
                serde_json::from_value(value.clone()).map_err(|err| bad("remix.import_stems", err.to_string()))?;
            if meta.input.chars().count() > 2000 || meta.provider.len() > 80 || meta.voice.len() > 80 || !meta.speed.is_finite() {
                return Err(bad("remix.import_stems", "Invalid generation provenance"));
            }
            Some(meta)
        }
        None => None,
    };
    let at = position_param(e, "remix.import_stems", p, "at")?.unwrap_or(0);
    if at < 0 {
        return Err(bad("remix.import_stems", "Negative start"));
    }
    // Build a separate document. A bad final file must not leave a partial import.
    let mut staged = Engine::new(e.session().clone());
    let mut result = Vec::new();
    let mut frames = None;
    for stem in stems {
        let path = limited(stem, "path", 4096)?;
        let name = limited(stem, "name", 80)?;
        let estimated = match stem.get("estimated") {
            None => false,
            Some(v) => v.as_bool().ok_or_else(|| bad("remix.import_stems", "estimated must be boolean"))?,
        };
        let mut bytes = Vec::new();
        std::fs::File::open(&path)
            .map_err(|err| EngineError::Io(err.to_string()))?
            .take(25 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|err| EngineError::Io(err.to_string()))?;
        if bytes.len() > 25 * 1024 * 1024 || bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
            return Err(bad("remix.import_stems", "Each stem must be a WAV up to 25 MB"));
        }
        let label = if estimated { format!("{name} (estimated)") } else { name };
        let imported = crate::io::import_audio_bytes(&mut staged, &format!("{label}.wav"), &bytes, Some(&path), None, at)?;
        let length = imported.get("frames").and_then(Value::as_u64).ok_or_else(|| bad("remix.import_stems", "Missing length"))?;
        if frames.is_some_and(|n| n != length) {
            return Err(bad("remix.import_stems", "Stems must have equal duration after resampling"));
        }
        frames = Some(length);
        if let Some(source_id) = imported.get("source").and_then(Value::as_u64)
            && let Some(source) = staged.session_mut().sources.iter_mut().find(|s| s.id.0 == source_id)
        {
            source.generation = generation.clone();
        }
        result.push(imported);
    }
    *e.session_mut() = staged.session().clone();
    Ok(json!({"stems":result,"at":at}))
}
fn section(e: &mut Engine, p: &Value) -> Result<Value> {
    let name = limited(p, "name", 80)?;
    let start = position_param(e, "remix.section", p, "start")?.ok_or_else(|| bad("remix.section", "start required"))?;
    let end = position_param(e, "remix.section", p, "end")?.ok_or_else(|| bad("remix.section", "end required"))?;
    if start < 0 || end <= start {
        return Err(bad("remix.section", "Use a positive section range"));
    }
    let id = e.session_mut().add_marker(&name, MarkerKind::Selection, start, end);
    Ok(json!({"id":id}))
}
fn loop_section(e: &mut Engine, p: &Value) -> Result<Value> {
    let number = p.get("number").and_then(Value::as_u64).ok_or_else(|| bad("remix.loop_section", "number required"))?;
    let range = e
        .session()
        .markers
        .iter()
        .find(|m| u64::from(m.number) == number && m.kind == MarkerKind::Selection)
        .map(|m| Range::new(m.start, m.end))
        .filter(|r| !r.is_empty())
        .ok_or_else(|| bad("remix.loop_section", "Choose a section marker"))?;
    e.session_mut().edit.selection = range;
    e.session_mut().edit.loop_playback = true;
    Ok(json!({"start":range.start,"end":range.end}))
}
fn export(e: &mut Engine, p: &Value) -> Result<Value> {
    let dir = limited(p, "dir", 4096)?;
    let dir = Path::new(&dir);
    let end = e.session().content_end();
    if end <= 0 || end > e.session().sample_rate.samples(600.0) {
        return Err(bad("remix.export", "Export requires audio lasting at most 10 minutes"));
    }
    // Exclusive directory creation prevents overwriting previous exports.
    std::fs::create_dir(dir).map_err(|err| EngineError::Io(err.to_string()))?;
    let result = export_into(e, dir, Range::new(0, end));
    result.map_err(|err| EngineError::Io(format!("Incomplete export kept at {}: {err}", dir.display())))
}
fn export_into(e: &Engine, dir: &Path, range: Range) -> Result<Value> {
    let mut project = Engine::new(e.session().clone());
    let project_path = dir.join("project.scraft");
    crate::io::save_session(&mut project, &project_path.to_string_lossy())?;
    let opts = soundcraft_audio_io::EncodeOptions::default();
    let (mix, _) = crate::io::bounce_bytes(e, range, &opts, false)?;
    std::fs::write(dir.join("mix.wav"), mix).map_err(|err| EngineError::Io(err.to_string()))?;
    // Unique stable IDs avoid colliding sanitized track names.
    let mut render = Engine::new(e.session().clone());
    for track in &mut render.session_mut().tracks {
        track.name = format!("{:04}-{}", track.id.0, track.name);
    }
    let stems_dir = dir.join("Stems");
    let files = crate::io::bounce_stems(&render, &stems_dir.to_string_lossy(), range, &opts)?;
    let relative: Vec<_> = files.iter().filter_map(|s| Path::new(s).strip_prefix(dir).ok().map(|p| p.to_string_lossy().replace('\\', "/"))).collect();
    let manifest = json!({"schema":1,"name":e.session().name,"project":"project.scraft","mix":"mix.wav","stems":relative,
        "sample_rate":e.session().sample_rate.hz(),"start_sample":0,"end_sample":range.end,
        "lyrics":e.session().lyric_versions,"tempo":e.session().tempo,"key_signatures":e.session().key_signatures,
        "markers":e.session().markers,"groups":e.session().groups,"sources":e.session().sources,
        "tracks":e.session().tracks.iter().map(|t| json!({"id":t.id,"name":t.name,"mixer":t.mixer})).collect::<Vec<_>>(),
        "stem_policy":"Rendered active unmuted tracks, aligned from sample zero; mixed generated songs remain mixed unless separated first"});
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(|err| EngineError::Io(err.to_string()))?;
    std::fs::write(dir.join("manifest.json"), bytes).map_err(|err| EngineError::Io(err.to_string()))?;
    Ok(json!({"dir":dir,"stems":files.len(),"manifest":dir.join("manifest.json")}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lyric_revision_undo_and_old_session() {
        let mut e = Engine::default();
        let p = json!({"name":"First","text":"[Chorus]\nLeave the key"});
        e.execute("lyrics.save_version", &p).unwrap();
        assert!(e.execute("lyrics.save_version", &p).is_err());
        let text = e.session().to_json().unwrap();
        assert_eq!(soundcraft_model::Session::from_json(&text).unwrap().lyric_versions.len(), 1);
        e.execute("edit.undo", &json!({})).unwrap();
        assert!(e.session().lyric_versions.is_empty());
        let mut old = serde_json::to_value(e.session()).unwrap();
        old.as_object_mut().unwrap().remove("lyric_versions");
        assert!(soundcraft_model::Session::from_json(&old.to_string()).unwrap().lyric_versions.is_empty());
    }
    #[test]
    fn section_loop_and_invalid_range() {
        let mut e = Engine::default();
        assert!(e.execute("remix.section", &json!({"name":"bad","start":10,"end":2})).is_err());
        e.execute("remix.section", &json!({"name":"Chorus","start":0,"end":48000})).unwrap();
        e.execute("remix.loop_section", &json!({"number":1})).unwrap();
        assert!(e.session().edit.loop_playback);
        assert_eq!(e.session().edit.selection.end, 48000);
    }
    #[test]
    fn missing_stem_does_not_change_session() {
        let mut e = Engine::default();
        let before = e.session().clone();
        assert!(e.execute("remix.import_stems", &json!({"stems":[{"name":"voice","path":"missing.wav"}]})).is_err());
        assert_eq!(e.session(), &before);
    }
    fn wav(path: &Path, frames: usize) {
        let buffer = soundcraft_audio_io::AudioBuffer { sample_rate: 48000, channels: vec![vec![0.1; frames]] };
        std::fs::write(path, soundcraft_audio_io::encode(&buffer, &soundcraft_audio_io::EncodeOptions::default()).unwrap()).unwrap();
    }
    #[test]
    fn aligned_import_roundtrip_export_and_no_overwrite() {
        let temp = std::env::temp_dir().join(format!(
            "soundcraft-remix-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir(&temp).unwrap();
        let a = temp.as_path().join("a.wav");
        let b = temp.as_path().join("b.wav");
        wav(&a, 4800);
        wav(&b, 4800);
        let mut e = Engine::default();
        e.execute(
            "remix.import_stems",
            &json!({"at":120,"stems":[
            {"path":a,"name":"Guitar / lead","estimated":true},
            {"path":b,"name":"Guitar : lead","estimated":true}]}),
        )
        .unwrap();
        assert_eq!(e.session().tracks.len(), 2);
        for track in &e.session().tracks {
            assert_eq!(track.clips()[0].start, 120);
        }
        let before = e.session().clone();
        wav(&b, 2400);
        assert!(e.execute("remix.import_stems", &json!({"stems":[{"path":a,"name":"a"},{"path":b,"name":"b"}]})).is_err());
        assert_eq!(e.session(), &before);
        let dir = temp.as_path().join("Package");
        let result = e.execute("remix.export", &json!({"dir":dir})).unwrap();
        assert_eq!(result["stems"], 2);
        assert!(e.execute("remix.export", &json!({"dir":dir})).is_err());
        let manifest: Value = serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
        for path in manifest["stems"].as_array().unwrap() {
            let (_, audio) = soundcraft_audio_io::decode(&std::fs::read(dir.join(path.as_str().unwrap())).unwrap(), Some("wav")).unwrap();
            assert_eq!(audio.frames(), 4920);
        }
        let mut reopened = Engine::default();
        crate::io::open_session(&mut reopened, &dir.join("project.scraft").to_string_lossy()).unwrap();
        assert_eq!(reopened.session().tracks.len(), 2);
        assert_eq!(reopened.session().pool.len(), 2);
        assert!(temp.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(temp).unwrap();
    }
}
