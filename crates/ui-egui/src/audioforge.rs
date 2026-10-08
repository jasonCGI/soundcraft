//! Native Generate Voice panel. AudioForge runs outside the realtime audio engine.
use crate::SoundApp;
use serde_json::{Value, json};
use soundcraft_engine::Engine;
use soundcraft_playback::Player;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct VoiceState {
    pub open: bool,
    text: String,
    voice: String,
    endpoint: String,
    speed: f64,
    status: String,
    job: Option<Child>,
    folder: Option<PathBuf>,
    folders: Vec<PathBuf>,
    take: Option<(PathBuf, Value)>,
    pending: Value,
    audition: Option<Player>,
}
impl Default for VoiceState {
    fn default() -> Self {
        Self {
            open: false,
            text: String::new(),
            voice: "af_heart".into(),
            endpoint: std::env::var("SOUNDCRAFT_AUDIOFORGE_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8800".into()),
            speed: 1.0,
            status: String::new(),
            job: None,
            folder: None,
            folders: Vec::new(),
            take: None,
            pending: Value::Null,
            audition: None,
        }
    }
}
impl VoiceState {
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
        if self.text.trim().is_empty()
            || self.text.chars().count() > 5000
            || !self.speed.is_finite()
            || !(0.5..=2.0).contains(&self.speed)
            || self.voice.is_empty()
            || self.voice.len() > 80
        {
            return Err("Enter between 1 and 5000 characters".into());
        }
        let python = std::env::var("SOUNDCRAFT_AUDIOFORGE_PYTHON").map_err(|_| "Set SOUNDCRAFT_AUDIOFORGE_PYTHON to your Python executable")?;
        let bridge = std::env::var("SOUNDCRAFT_AUDIOFORGE_BRIDGE").map_err(|_| "Set SOUNDCRAFT_AUDIOFORGE_BRIDGE to bridge.py")?;
        self.audition = None;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
        let folder = std::env::temp_dir().join(format!("soundcraft-voice-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&folder).map_err(|e| e.to_string())?;
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
        let settings = json!({"input":self.text,"voice":self.voice,"speed":self.speed});
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
        self.folders.push(folder.clone());
        self.folder = Some(folder);
        self.job = Some(child);
        self.status = "Generating voice...".into();
        Ok(())
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(job) = self.job.as_mut() else { return };
        match job.try_wait() {
            Ok(Some(exit)) => {
                self.job = None;
                if let Some(folder) = &self.folder {
                    if exit.success() {
                        self.take = Some((folder.join("take.wav"), self.pending.clone()));
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
    app.voice.poll(ctx);
    if !app.voice.open {
        return;
    }
    let mut open = app.voice.open;
    let mut insert = false;
    egui::Window::new("Generate Voice").open(&mut open).default_width(470.0).show(ctx, |ui| {
        let state = &mut app.voice;
        let busy = state.job.is_some();
        ui.add_enabled_ui(!busy, |ui| {
            ui.label("Local AudioForge endpoint");
            ui.text_edit_singleline(&mut state.endpoint);
            ui.horizontal(|ui| {
                ui.label("Voice ID");
                ui.text_edit_singleline(&mut state.voice);
            });
            ui.add(egui::Slider::new(&mut state.speed, 0.5..=2.0).text("Speed"));
            ui.label("Script");
            ui.add(egui::TextEdit::multiline(&mut state.text).desired_rows(8).char_limit(5000));
            ui.label(format!("{} / 5000 characters", state.text.chars().count()));
            if ui.button("Generate take").clicked()
                && let Err(error) = state.start()
            {
                state.status = error;
            }
        });
        if busy && ui.button("Cancel generation").clicked() {
            state.cancel();
        }
        if let Some((path, _)) = state.take.clone() {
            ui.horizontal(|ui| {
                if ui.add_enabled(!busy && !app.engine.transport.playing && !app.engine.transport.recording, egui::Button::new("Audition")).clicked()
                {
                    let result = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|bytes| {
                        let mut preview = Engine::new(soundcraft_model::Session::default());
                        soundcraft_engine::io::import_audio_bytes(&mut preview, "Voice.wav", &bytes, None, None, 0).map_err(|e| e.to_string())?;
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
                insert = ui.add_enabled(!busy && !app.engine.transport.recording, egui::Button::new("Insert at playhead")).clicked();
            });
        }
        ui.label(&state.status);
    });
    app.voice.open = open;
    if insert && let Some((path, settings)) = app.voice.take.clone() {
        app.voice.audition = None;
        let mut params = settings;
        params["path"] = json!(path);
        params["at"] = json!(app.position());
        app.voice.status = match app.run("audioforge.insert", params) {
            Ok(_) => "Inserted on a new track. Undo restores the previous session.".into(),
            Err(error) => error,
        };
    }
}

/// The UI and control channel use the same generation commands.
pub fn run(app: &mut SoundApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    match id {
        "window.generate_voice" => {
            app.voice.open = p.get("value").and_then(Value::as_bool).unwrap_or(!app.voice.open);
            Some(Ok(json!({"open":app.voice.open})))
        }
        "audioforge.generate" => {
            if app.voice.job.is_some() {
                return Some(Err("A take is already being generated".into()));
            }
            if let Some(text) = p.get("input").and_then(Value::as_str) {
                app.voice.text = text.into();
            }
            if let Some(voice) = p.get("voice").and_then(Value::as_str) {
                app.voice.voice = voice.into();
            }
            if let Some(endpoint) = p.get("endpoint").and_then(Value::as_str) {
                app.voice.endpoint = endpoint.into();
            }
            if let Some(speed) = p.get("speed").and_then(Value::as_f64) {
                app.voice.speed = speed;
            }
            Some(app.voice.start().map(|()| json!({"started":true})))
        }
        "audioforge.cancel" => {
            app.voice.cancel();
            Some(Ok(json!({"cancelled":true})))
        }
        "audioforge.inspect" => Some(Ok(
            json!({"busy":app.voice.job.is_some(),"status":app.voice.status,"take":app.voice.take.as_ref().map(|(path,settings)| json!({"path":path,"settings":settings}))}),
        )),
        _ => None,
    }
}
