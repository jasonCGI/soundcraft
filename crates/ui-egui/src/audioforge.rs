//! Local generation panels. Provider work stays outside the realtime engine.
use crate::SoundApp;
use serde_json::{Value, json};
use soundcraft_engine::Engine;
use soundcraft_playback::Player;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone)]
struct Take {
    name: String,
    path: PathBuf,
    settings: Value,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Preset {
    name: String,
    voice: String,
    speed: f64,
}

pub struct VoiceState {
    pub open: bool,
    music: bool,
    text: String,
    voice: String,
    endpoint: String,
    speed: f64,
    duration: f64,
    bpm: u32,
    seed: u64,
    status: String,
    job: Option<Child>,
    folder: Option<PathBuf>,
    folders: Vec<PathBuf>,
    takes: Vec<Take>,
    selected: Option<usize>,
    pending: Value,
    audition: Option<Player>,
    presets: Vec<Preset>,
    preset_name: String,
    export_path: String,
    next_take: u64,
}
fn preset_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
        .map(|p| p.join("Cardona Pipeline Tools").join("SoundCraft Preview").join("voice-presets.json"))
}
impl Default for VoiceState {
    fn default() -> Self {
        let mut presets = vec![
            Preset { name: "Warm narration".into(), voice: "af_heart".into(), speed: 1.0 },
            Preset { name: "Measured narration".into(), voice: "af_heart".into(), speed: 0.9 },
            Preset { name: "Bright narration".into(), voice: "af_bella".into(), speed: 1.05 },
        ];
        if let Some(path) = preset_path()
            && let Ok(file) = std::fs::File::open(path)
        {
            use std::io::Read;
            if let Ok(custom) = serde_json::from_reader::<_, Vec<Preset>>(file.take(65536)) {
                presets.extend(custom.into_iter().filter(valid_preset).take(20));
            }
        }
        Self {
            open: false,
            music: false,
            text: String::new(),
            voice: "af_heart".into(),
            endpoint: std::env::var("SOUNDCRAFT_AUDIOFORGE_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8800".into()),
            speed: 1.0,
            duration: 30.0,
            bpm: 100,
            seed: 42,
            status: String::new(),
            job: None,
            folder: None,
            folders: Vec::new(),
            takes: Vec::new(),
            selected: None,
            pending: Value::Null,
            audition: None,
            presets,
            preset_name: String::new(),
            export_path: String::new(),
            next_take: 1,
        }
    }
}
fn valid_preset(p: &Preset) -> bool {
    !p.name.trim().is_empty()
        && p.name.chars().count() <= 60
        && !p.voice.is_empty()
        && p.voice.len() <= 80
        && p.speed.is_finite()
        && (0.5..=2.0).contains(&p.speed)
}
impl VoiceState {
    pub fn music() -> Self {
        Self {
            music: true,
            endpoint: std::env::var("SOUNDCRAFT_MUSIC_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8001".into()),
            ..Self::default()
        }
    }
    fn take(&self) -> Option<&Take> {
        self.selected.and_then(|i| self.takes.get(i))
    }
    fn cancel(&mut self) {
        if let Some(mut job) = self.job.take() {
            let _ = job.kill();
            let _ = job.wait();
        }
        self.status = "Cancelled. The provider may finish its current request.".into();
    }
    fn start(&mut self) -> Result<(), String> {
        if self.job.is_some() {
            return Err("A take is already being generated".into());
        }
        if self.takes.len() >= 8 {
            return Err("Keep up to 8 takes. Export or discard one before generating again.".into());
        }
        let limit = if self.music { 2000 } else { 5000 };
        if self.text.trim().is_empty()
            || self.text.chars().count() > limit
            || !self.speed.is_finite()
            || !(0.5..=2.0).contains(&self.speed)
            || self.voice.is_empty()
            || self.voice.len() > 80
        {
            return Err(format!("Enter between 1 and {limit} characters and valid voice settings"));
        }
        if self.music
            && (!self.duration.is_finite() || !(10.0..=60.0).contains(&self.duration) || !(30..=300).contains(&self.bpm) || self.seed > 2147483647)
        {
            return Err("Use 10 to 60 seconds, 30 to 300 BPM, and a nonnegative 32-bit seed".into());
        }
        let python = std::env::var("SOUNDCRAFT_AUDIOFORGE_PYTHON").map_err(|_| "Set SOUNDCRAFT_AUDIOFORGE_PYTHON to your Python executable")?;
        let bridge_var = if self.music { "SOUNDCRAFT_MUSIC_BRIDGE" } else { "SOUNDCRAFT_AUDIOFORGE_BRIDGE" };
        let bridge = std::env::var(bridge_var).map_err(|_| format!("Set {bridge_var} to the generation bridge"))?;
        self.audition = None;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
        let folder = std::env::temp_dir().join(format!("soundcraft-take-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&folder).map_err(|e| e.to_string())?;
        // Register ownership before starting, so failed launches are cleaned up too.
        self.folders.push(folder.clone());
        let error_file = std::fs::File::create(folder.join("error.txt")).map_err(|e| e.to_string())?;
        let mut command = Command::new(python);
        command
            .args(["-I", &bridge, "--endpoint", &self.endpoint, "--out"])
            .arg(folder.join("take.wav"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::from(error_file));
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let settings = if self.music {
            json!({"input":self.text,"duration":self.duration,"bpm":self.bpm,"seed":self.seed,"model":"acestep-v15-turbo"})
        } else {
            json!({"input":self.text,"voice":self.voice,"speed":self.speed})
        };
        let payload = settings.to_string();
        let write_result = child
            .stdin
            .take()
            .ok_or("Cannot open generation input")
            .and_then(|mut input| input.write_all(payload.as_bytes()).map_err(|_| "Cannot send generation input"));
        if let Err(error) = write_result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        self.pending = settings;
        self.folder = Some(folder);
        self.job = Some(child);
        self.status = if self.music { "Generating instrumental..." } else { "Generating voice..." }.into();
        Ok(())
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(job) = self.job.as_mut() else { return };
        match job.try_wait() {
            Ok(Some(exit)) => {
                self.job = None;
                if let Some(folder) = &self.folder {
                    if exit.success() {
                        let kind = if self.music { "Music" } else { "Voice" };
                        self.takes.push(Take {
                            name: format!("{kind} take {}", self.next_take),
                            path: folder.join("take.wav"),
                            settings: self.pending.clone(),
                        });
                        self.next_take = self.next_take.saturating_add(1);
                        self.selected = self.takes.len().checked_sub(1);
                        self.status = "Take ready. Audition before inserting.".into();
                    } else {
                        self.status = std::fs::read_to_string(folder.join("error.txt")).unwrap_or_else(|_| "Generation failed".into());
                    }
                }
            }
            Ok(None) => ctx.request_repaint_after(Duration::from_millis(100)),
            Err(error) => {
                self.cancel();
                self.status = error.to_string();
            }
        }
    }
    fn save_preset(&mut self, name: &str) -> Result<(), String> {
        let preset = Preset { name: name.trim().into(), voice: self.voice.clone(), speed: self.speed };
        if !valid_preset(&preset) {
            return Err("Use a preset name of 1 to 60 characters and valid voice settings".into());
        }
        if self.presets.len() >= 23 {
            return Err("The preview supports 20 custom presets".into());
        }
        let path = preset_path().ok_or("Cannot locate your preferences folder")?;
        let parent = path.parent().ok_or("Cannot locate your preferences folder")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut presets = self.presets.clone();
        presets.push(preset);
        let custom = presets.get(3..).ok_or("Cannot read presets")?;
        let encoded = serde_json::to_vec_pretty(custom).map_err(|e| e.to_string())?;
        std::fs::write(path, encoded).map_err(|e| e.to_string())?;
        self.presets = presets;
        Ok(())
    }
    fn export(&self, target: &str) -> Result<(), String> {
        let take = self.take().ok_or("Select a take")?;
        if target.trim().is_empty() {
            return Err("Enter a destination WAV path".into());
        }
        let bytes = std::fs::read(&take.path).map_err(|e| e.to_string())?;
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(target).map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())
    }
    fn discard(&mut self) -> Result<(), String> {
        let index = self.selected.filter(|i| *i < self.takes.len()).ok_or("Select a take")?;
        self.audition = None;
        let take = self.takes.remove(index);
        let _ = std::fs::remove_file(take.path);
        self.selected = self.takes.len().checked_sub(1);
        Ok(())
    }
}
impl Drop for VoiceState {
    fn drop(&mut self) {
        self.cancel();
        self.audition = None;
        for folder in &self.folders {
            let _ = std::fs::remove_file(folder.join("take.wav"));
            let _ = std::fs::remove_file(folder.join("error.txt"));
            let _ = std::fs::remove_dir(folder);
        }
    }
}

pub fn show(app: &mut SoundApp, ctx: &egui::Context) {
    show_panel(app, ctx, false);
    show_panel(app, ctx, true);
}
fn show_panel(app: &mut SoundApp, ctx: &egui::Context, music: bool) {
    let state = if music { &mut app.music } else { &mut app.voice };
    state.poll(ctx);
    if !state.open {
        return;
    }
    let mut open = state.open;
    let mut insert = false;
    let playing = app.engine.transport.playing;
    let recording = app.engine.transport.recording;
    let title = if music { "Generate Music" } else { "Generate Voice" };
    egui::Window::new(title).open(&mut open).default_width(500.0).show(ctx, |ui| {
        let busy = state.job.is_some();
        ui.add_enabled_ui(!busy, |ui| {
            ui.label(if music { "Local ACE-Step endpoint" } else { "Local AudioForge endpoint" });
            ui.text_edit_singleline(&mut state.endpoint);
            if music {
                ui.label("ACE-Step turbo, instrumental");
                ui.add(egui::Slider::new(&mut state.duration, 10.0..=60.0).text("Seconds"));
                ui.add(egui::Slider::new(&mut state.bpm, 30..=300).text("BPM"));
                ui.horizontal(|ui| {
                    ui.label("Seed");
                    ui.add(egui::DragValue::new(&mut state.seed).range(0..=2147483647_u64));
                });
            } else {
                egui::ComboBox::from_id_salt("voice-preset").selected_text("Choose voice preset").show_ui(ui, |ui| {
                    for preset in &state.presets {
                        if ui.button(&preset.name).clicked() {
                            state.voice = preset.voice.clone();
                            state.speed = preset.speed;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Voice ID");
                    ui.text_edit_singleline(&mut state.voice);
                });
                ui.add(egui::Slider::new(&mut state.speed, 0.5..=2.0).text("Speed"));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut state.preset_name).hint_text("Custom preset name").char_limit(60));
                    if ui.button("Save preset").clicked() {
                        let name = state.preset_name.clone();
                        state.status = match state.save_preset(&name) {
                            Ok(()) => "Preset saved.".into(),
                            Err(e) => e,
                        };
                    }
                });
            }
            ui.label(if music { "Music prompt" } else { "Script" });
            let limit = if music { 2000 } else { 5000 };
            ui.add(egui::TextEdit::multiline(&mut state.text).desired_rows(5).desired_width(f32::INFINITY).char_limit(limit));
            ui.label(format!("{} / {limit} characters", state.text.chars().count()));
            if ui.button("Generate take").clicked()
                && let Err(error) = state.start()
            {
                state.status = error;
            }
        });
        if busy && ui.button("Cancel generation").clicked() {
            state.cancel();
        }
        ui.separator();
        ui.label(format!("Takes: {} / 8, kept until this app closes", state.takes.len()));
        for (index, take) in state.takes.iter().enumerate() {
            if ui.selectable_label(state.selected == Some(index), &take.name).clicked() {
                state.selected = Some(index);
                state.audition = None;
            }
        }
        if let Some(take) = state.take().cloned() {
            if let Some(index) = state.selected
                && let Some(selected) = state.takes.get_mut(index)
            {
                ui.add(egui::TextEdit::singleline(&mut selected.name).hint_text("Take name").char_limit(80));
            }
            ui.label(take.settings.to_string());
            ui.horizontal(|ui| {
                if ui.add_enabled(!busy && !playing && !recording, egui::Button::new("Audition")).clicked() {
                    let result = std::fs::read(&take.path).map_err(|e| e.to_string()).and_then(|bytes| {
                        let mut preview = Engine::new(soundcraft_model::Session::default());
                        soundcraft_engine::io::import_audio_bytes(&mut preview, "Take.wav", &bytes, None, None, 0).map_err(|e| e.to_string())?;
                        let player = Player::new(preview.session_arc());
                        player.play(0, Some(preview.session().content_end()), None);
                        state.audition = Some(player);
                        Ok(())
                    });
                    if let Err(error) = result {
                        state.status = error;
                    }
                }
                if ui.button("Stop audition").clicked() {
                    state.audition = None;
                }
                insert = ui.add_enabled(!busy && !recording, egui::Button::new("Insert at playhead")).clicked();
                if ui.button("Discard").clicked() {
                    let _ = state.discard();
                }
            });
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut state.export_path).hint_text("Destination WAV path"));
                if ui.button("Export WAV").clicked() {
                    state.status = match state.export(&state.export_path) {
                        Ok(()) => "WAV exported.".into(),
                        Err(e) => e,
                    };
                }
            });
        }
        ui.label(&state.status);
    });
    state.open = open;
    if insert && let Some(take) = state.take().cloned() {
        state.audition = None;
        let mut params = take.settings;
        params["path"] = json!(take.path);
        params["at"] = json!(app.position());
        let command = if music { "musicforge.insert" } else { "audioforge.insert" };
        let result = match app.run(command, params) {
            Ok(_) => "Inserted on a new track. Undo restores the previous session.".into(),
            Err(e) => e,
        };
        let state = if music { &mut app.music } else { &mut app.voice };
        state.status = result;
    }
}

/// The UI and control channel share these generation and take-management commands.
pub fn run(app: &mut SoundApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    let music = id == "window.generate_music" || id.starts_with("musicforge.");
    let operation = if id == "window.generate_voice" || id == "window.generate_music" {
        "window"
    } else if id.starts_with("audioforge.") || id.starts_with("musicforge.") {
        id.split('.').nth(1)?
    } else {
        return None;
    };
    let state = if music { &mut app.music } else { &mut app.voice };
    match operation {
        "window" => {
            state.open = p.get("value").and_then(Value::as_bool).unwrap_or(!state.open);
            Some(Ok(json!({"open":state.open})))
        }
        "generate" => {
            if state.job.is_some() {
                return Some(Err("A take is already being generated".into()));
            }
            for field in ["speed", "duration", "bpm", "seed"] {
                if p.get(field).is_some_and(|v| if field == "speed" || field == "duration" { v.as_f64().is_none() } else { v.as_u64().is_none() }) {
                    return Some(Err(format!("Invalid {field}")));
                }
            }
            if music && p.get("model").is_some_and(|v| v.as_str() != Some("acestep-v15-turbo")) {
                return Some(Err("This preview supports the ACE-Step turbo model".into()));
            }
            if let Some(v) = p.get("input").and_then(Value::as_str) {
                state.text = v.into();
            }
            if let Some(v) = p.get("voice").and_then(Value::as_str) {
                state.voice = v.into();
            }
            if let Some(v) = p.get("endpoint").and_then(Value::as_str) {
                state.endpoint = v.into();
            }
            if let Some(v) = p.get("speed").and_then(Value::as_f64) {
                state.speed = v;
            }
            if let Some(v) = p.get("duration").and_then(Value::as_f64) {
                state.duration = v;
            }
            if let Some(v) = p.get("bpm").and_then(Value::as_u64) {
                state.bpm = u32::try_from(v).unwrap_or(u32::MAX);
            }
            if let Some(v) = p.get("seed").and_then(Value::as_u64) {
                state.seed = v;
            }
            Some(state.start().map(|()| json!({"started":true})))
        }
        "cancel" => {
            state.cancel();
            Some(Ok(json!({"cancelled":true})))
        }
        "inspect" => Some(Ok(json!({"busy":state.job.is_some(),"status":state.status,
            "take":state.take().map(|t| json!({"path":t.path,"settings":t.settings,"name":t.name})),
            "takes":state.takes.iter().enumerate().map(|(i,t)| json!({"index":i,"name":t.name,"settings":t.settings})).collect::<Vec<_>>(),
            "presets":state.presets}))),
        "take_select" => Some(
            p.get("index")
                .and_then(Value::as_u64)
                .and_then(|i| usize::try_from(i).ok())
                .filter(|i| *i < state.takes.len())
                .ok_or_else(|| "Select an existing take index".to_string())
                .map(|i| {
                    state.selected = Some(i);
                    state.audition = None;
                    json!({"selected":i})
                }),
        ),
        "take_rename" => {
            let name = p.get("name").and_then(Value::as_str).unwrap_or("").trim();
            if name.is_empty() || name.chars().count() > 80 {
                return Some(Err("Use a take name of 1 to 80 characters".into()));
            }
            Some(state.selected.and_then(|i| state.takes.get_mut(i)).ok_or_else(|| "Select a take".to_string()).map(|take| {
                take.name = name.into();
                json!({"renamed":true})
            }))
        }
        "take_export" => Some(state.export(p.get("path").and_then(Value::as_str).unwrap_or("")).map(|()| json!({"exported":true}))),
        "take_discard" => Some(state.discard().map(|()| json!({"discarded":true}))),
        "preset_save" if !music => Some(state.save_preset(p.get("name").and_then(Value::as_str).unwrap_or("")).map(|()| json!({"saved":true}))),
        "preset_apply" if !music => Some(
            p.get("name")
                .and_then(Value::as_str)
                .and_then(|name| state.presets.iter().find(|p| p.name == name))
                .cloned()
                .ok_or_else(|| "Choose an existing preset".to_string())
                .map(|preset| {
                    state.voice = preset.voice;
                    state.speed = preset.speed;
                    json!({"voice":state.voice,"speed":state.speed})
                }),
        ),
        _ => None,
    }
}
