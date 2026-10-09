//! Local generation panels. Provider work stays outside the realtime engine.
use crate::SoundApp;
use serde_json::{Value, json};
use soundcraft_engine::Engine;
use soundcraft_playback::Player;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// Original prompt templates, editable before generation.
const MUSIC_PRESETS: &[(&str, &str, u32)] = &[
    ("Ambient", "Gentle ambient music, warm piano, evolving synth pads, spacious reverb.", 72),
    ("Lo-fi hip-hop", "Relaxed lo-fi music, mellow electric piano, warm bass, laid-back dusty drum beat.", 88),
    ("Cinematic", "Uplifting cinematic music, expressive strings, piano, warm brass, restrained percussion.", 110),
    ("Electronic", "Melodic electronic music, bright synth arpeggios, deep bass, crisp dance drums.", 124),
    ("Acoustic folk", "Warm acoustic folk music, fingerpicked guitar, gentle piano, light hand percussion.", 90),
    ("Jazz", "Mellow jazz instrumental trio, expressive piano, upright bass, brushed drums, relaxed swing.", 100),
    ("Rock", "Energetic melodic rock music, layered electric guitars, driving bass, live drums.", 120),
];

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
    genre: String,
    vocals: bool,
    lyrics: String,
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
    lyric_name: String,
    lead_role: String,
    backing_role: String,
    remix_dir: String,
    stem_paths: String,
    section_name: String,
    section_start: f64,
    section_end: f64,
    separating: bool,
    compare_a: Option<Take>,
    compare_b: Option<Take>,
    excerpt_start: usize,
    excerpt_end: usize,
    layer_group: String,
    layer_track: String,
    alternate_playlist: u64,
    stem_generation: Value,
    remix_view: bool,
    lyric_analysis: Value,
    lyric_analysis_source: String,
}
fn preset_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
        .map(|p| p.join("Cardona Pipeline Tools").join("SoundCraft Preview").join("voice-presets.json"))
}
impl Default for VoiceState {
    fn default() -> Self {
        Self::new(false)
    }
}
impl VoiceState {
    fn new(music: bool) -> Self {
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
            music,
            genre: "Custom".into(),
            vocals: false,
            lyrics: String::new(),
            text: String::new(),
            voice: "af_heart".into(),
            endpoint: if music {
                std::env::var("SOUNDCRAFT_MUSIC_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8001".into())
            } else {
                std::env::var("SOUNDCRAFT_AUDIOFORGE_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8800".into())
            },
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
            lyric_name: String::new(),
            lead_role: "Female lead".into(),
            backing_role: "Male backing shouts".into(),
            remix_dir: String::new(),
            stem_paths: String::new(),
            section_name: "Chorus".into(),
            section_start: 0.0,
            section_end: 10.0,
            separating: false,
            compare_a: None,
            compare_b: None,
            excerpt_start: 1,
            excerpt_end: 12,
            layer_group: "Guitars".into(),
            layer_track: String::new(),
            alternate_playlist: 1,
            stem_generation: Value::Null,
            remix_view: false,
            lyric_analysis: Value::Null,
            lyric_analysis_source: String::new(),
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
        Self::new(true)
    }
    fn take(&self) -> Option<&Take> {
        self.selected.and_then(|i| self.takes.get(i))
    }
    fn cancel(&mut self) {
        if let Some(mut job) = self.job.take() {
            let _ = job.kill();
            let _ = job.wait();
        }
        self.separating = false;
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
        if self.music && self.vocals && (self.lyrics.trim().is_empty() || self.lyrics.chars().count() > 3000) {
            return Err("Enter between 1 and 3000 lyric characters for sung vocals".into());
        }
        let resolved_prompt = if self.music && self.vocals {
            format!("{} Lead vocal role: {}. Backing vocal role: {}.", self.text, self.lead_role, self.backing_role)
        } else {
            self.text.clone()
        };
        if self.music && resolved_prompt.chars().count() > 2000 {
            return Err("Music prompt plus vocal roles must fit within 2000 characters".into());
        }
        let python = std::env::var("SOUNDCRAFT_AUDIOFORGE_PYTHON").map_err(|_| "Set SOUNDCRAFT_AUDIOFORGE_PYTHON to your Python executable")?;
        let bridge_var = if self.music { "SOUNDCRAFT_MUSIC_BRIDGE" } else { "SOUNDCRAFT_AUDIOFORGE_BRIDGE" };
        let bridge = std::env::var(bridge_var).map_err(|_| format!("Set {bridge_var} to the generation bridge"))?;
        self.audition = None;
        self.separating = false;
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
            json!({"input":resolved_prompt,"duration":self.duration,"bpm":self.bpm,"seed":self.seed,"model":"acestep-v15-turbo","genre":self.genre,"lyrics":if self.vocals { self.lyrics.as_str() } else { "[Instrumental]" }})
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
        self.status = if self.music { "Generating music..." } else { "Generating voice..." }.into();
        Ok(())
    }
    fn audition_path(&mut self, path: &std::path::Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let mut preview = Engine::default();
        soundcraft_engine::io::import_audio_bytes(&mut preview, "Take.wav", &bytes, None, None, 0).map_err(|e| e.to_string())?;
        let player = Player::new(preview.session_arc());
        player.play(0, Some(preview.session().content_end()), None);
        self.audition = Some(player);
        Ok(())
    }
    fn archive(&self, folder: &std::path::Path) -> Result<PathBuf, String> {
        let preferences = preset_path().ok_or("Cannot locate project archive")?;
        let parent = preferences.parent().ok_or("Cannot locate project archive")?.join("Generations");
        std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        let name = folder.file_name().ok_or("Missing take folder")?;
        let destination = parent.join(name);
        std::fs::create_dir(&destination).map_err(|e| e.to_string())?;
        std::fs::copy(folder.join("take.wav"), destination.join("original.wav")).map_err(|e| e.to_string())?;
        let metadata = serde_json::to_vec_pretty(&self.pending).map_err(|e| e.to_string())?;
        std::fs::write(destination.join("generation.json"), metadata).map_err(|e| e.to_string())?;
        Ok(destination)
    }
    fn separate(&mut self) -> Result<(), String> {
        if self.job.is_some() {
            return Err("Wait for the current job".into());
        }
        let take = self.take().ok_or("Select a music take")?.clone();
        let input = take.path;
        self.stem_generation = json!({"provider":"ace-step-local","input":take.settings.get("input"),"voice":"","speed":1.0,
            "music":{"model":take.settings.get("model"),"duration":take.settings.get("duration"),"bpm":take.settings.get("bpm"),
            "seed":take.settings.get("seed"),"genre":take.settings.get("genre"),"lyrics":take.settings.get("lyrics")}});
        let python = std::env::var("SOUNDCRAFT_STEM_PYTHON").map_err(|_| "Set SOUNDCRAFT_STEM_PYTHON to a TorchAudio Python runtime")?;
        let bridge = std::env::var("SOUNDCRAFT_STEM_BRIDGE").map_err(|_| "Set SOUNDCRAFT_STEM_BRIDGE")?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
        let folder = std::env::temp_dir().join(format!("soundcraft-stems-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&folder).map_err(|e| e.to_string())?;
        self.folders.push(folder.clone());
        let error = std::fs::File::create(folder.join("error.txt")).map_err(|e| e.to_string())?;
        self.job = Some(
            Command::new(python)
                .args(["-I", &bridge, "--input"])
                .arg(input)
                .arg("--out")
                .arg(folder.join("stems"))
                .stdout(Stdio::null())
                .stderr(Stdio::from(error))
                .spawn()
                .map_err(|e| e.to_string())?,
        );
        self.folder = Some(folder);
        self.separating = true;
        self.status = "Separating on CPU. First use downloads model weights.".into();
        Ok(())
    }
    fn analyze_lyrics(&mut self) -> Result<Value, String> {
        if self.job.is_some() {
            return Err("Wait for the current job".into());
        }
        if self.lyrics.trim().is_empty() || self.lyrics.chars().count() > 3000 {
            return Err("Enter between 1 and 3000 lyric characters".into());
        }
        let python = std::env::var("SOUNDCRAFT_LYRIC_PYTHON")
            .or_else(|_| std::env::var("SOUNDCRAFT_AUDIOFORGE_PYTHON"))
            .map_err(|_| "Set SOUNDCRAFT_LYRIC_PYTHON to your Python executable")?;
        let bridge = std::env::var("SOUNDCRAFT_LYRIC_ANALYZER").map_err(|_| "Set SOUNDCRAFT_LYRIC_ANALYZER")?;
        let mut command = Command::new(python);
        command.args(["-I", &bridge]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|error| error.to_string())?;
        let payload = json!({"lyrics":self.lyrics}).to_string();
        let write_result = child
            .stdin
            .take()
            .ok_or_else(|| "Cannot open lyric analysis input".to_string())
            .and_then(|mut input| input.write_all(payload.as_bytes()).map_err(|_| "Cannot send lyrics to analyzer".to_string()));
        if let Err(error) = write_result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        let output = child.wait_with_output().map_err(|error| error.to_string())?;
        if !output.status.success() {
            let error = String::from_utf8_lossy(output.stderr.get(..8192).unwrap_or(&output.stderr)).trim().to_string();
            return Err(if error.is_empty() { "Lyric analysis failed".into() } else { error });
        }
        if output.stdout.len() > 256 * 1024 {
            return Err("Lyric analysis output is too large".into());
        }
        let report: Value = serde_json::from_slice(&output.stdout).map_err(|_| "Lyric analyzer returned invalid JSON".to_string())?;
        if report.get("schema").and_then(Value::as_u64) != Some(1)
            || report.get("summary").and_then(Value::as_object).is_none()
            || report.get("lines").and_then(Value::as_array).is_none()
        {
            return Err("Lyric analyzer returned an unsupported report".into());
        }
        self.lyric_analysis = report.clone();
        self.lyric_analysis_source = self.lyrics.clone();
        self.status = "Estimated lyric analysis updated.".into();
        Ok(report)
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(job) = self.job.as_mut() else { return };
        match job.try_wait() {
            Ok(Some(exit)) => {
                self.job = None;
                if let Some(folder) = &self.folder {
                    if exit.success() && self.separating {
                        self.stem_paths = ["drums", "bass", "other", "vocals"]
                            .iter()
                            .map(|name| folder.join("stems").join(format!("{name}.wav")).to_string_lossy().into_owned())
                            .collect::<Vec<_>>()
                            .join("\n");
                        self.status = "Estimated stems ready. Import them below, then save or export your project.".into();
                        self.separating = false;
                    } else if exit.success() {
                        let kind = if self.music { "Music" } else { "Voice" };
                        self.takes.push(Take {
                            name: format!("{kind} take {}", self.next_take),
                            path: folder.join("take.wav"),
                            settings: self.pending.clone(),
                        });
                        self.next_take = self.next_take.saturating_add(1);
                        self.selected = self.takes.len().checked_sub(1);
                        self.status = if self.music {
                            match self.archive(folder) {
                                Ok(path) => format!("Take ready. Original and settings archived in {}", path.display()),
                                Err(error) => format!("Take ready, but archive failed: {error}. Export before closing."),
                            }
                        } else {
                            "Take ready. Audition before inserting.".into()
                        };
                    } else {
                        self.separating = false;
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
        if self.compare_a.as_ref().is_some_and(|t| t.path == take.path) {
            self.compare_a = None;
        }
        if self.compare_b.as_ref().is_some_and(|t| t.path == take.path) {
            self.compare_b = None;
        }
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
            for name in ["drums.wav", "bass.wav", "other.wav", "vocals.wav", "stems.json"] {
                let _ = std::fs::remove_file(folder.join("stems").join(name));
            }
            let _ = std::fs::remove_dir(folder.join("stems"));
            let _ = std::fs::remove_dir(folder);
        }
    }
}

pub fn show(app: &mut SoundApp, ctx: &egui::Context) {
    show_panel(app, ctx, false);
    show_panel(app, ctx, true);
}
fn show_panel(app: &mut SoundApp, ctx: &egui::Context, music: bool) {
    let versions = app.engine.session().lyric_versions.clone();
    let sections = app.engine.session().markers.clone();
    let mut workflow = Vec::new();
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
    egui::Window::new(title).open(&mut open).default_width(560.0).default_height(720.0).vscroll(true).show(ctx, |ui| {
        let busy = state.job.is_some();
        if music {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.remix_view, false, "Generate");
                ui.selectable_value(&mut state.remix_view, true, "Remix");
            });
            ui.separator();
        }
        if !music || !state.remix_view {
        ui.add_enabled_ui(!busy, |ui| {
            ui.label(if music { "Local ACE-Step endpoint" } else { "Local AudioForge endpoint" });
            ui.text_edit_singleline(&mut state.endpoint);
            if music {
                egui::ComboBox::from_id_salt("music-genre").selected_text(&state.genre).show_ui(ui, |ui| {
                    for &(name, prompt, bpm) in MUSIC_PRESETS {
                        if ui.selectable_label(state.genre == name, name).clicked() {
                            state.genre = name.into();
                            state.text = prompt.into();
                            state.bpm = bpm;
                        }
                    }
                });
                ui.checkbox(&mut state.vocals, "Sung vocals (ACE-Step)");
                if state.vocals {
                    ui.collapsing("Lyric versions and vocal roles", |ui| {
                        ui.add(egui::TextEdit::singleline(&mut state.lyric_name).hint_text("New revision name").char_limit(80));
                        ui.horizontal(|ui| {
                            ui.label("Lead");
                            ui.add(egui::TextEdit::singleline(&mut state.lead_role).char_limit(120));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Backing");
                            ui.add(egui::TextEdit::singleline(&mut state.backing_role).char_limit(120));
                        });
                        if ui.button("Save lyric version in project").clicked() {
                            workflow.push((
                                "lyrics.save_version",
                                json!({"name":state.lyric_name,"text":state.lyrics,"lead":state.lead_role,"backing":state.backing_role}),
                            ));
                        }
                        for version in &versions {
                            if ui.button(format!("Load {}", version.name)).clicked() {
                                state.lyrics = version.text.clone();
                                state.lead_role = version.lead.clone();
                                state.backing_role = version.backing.clone();
                            }
                        }
                        ui.label("Use [Verse], [Chorus], and vocal-role tags. Save a new name for each revision.");
                        ui.horizontal(|ui| {
                            ui.label("Excerpt lines");
                            ui.add(egui::DragValue::new(&mut state.excerpt_start).range(1..=3000));
                            ui.add(egui::DragValue::new(&mut state.excerpt_end).range(1..=3000));
                            if ui.button("Use excerpt").clicked() {
                                if state.excerpt_end >= state.excerpt_start {
                                    state.lyrics = state.lyrics.lines().skip(state.excerpt_start.saturating_sub(1))
                                        .take(state.excerpt_end.saturating_sub(state.excerpt_start).saturating_add(1)).collect::<Vec<_>>().join("\n");
                                } else { state.status = "End line must follow start line".into(); }
                            }
                        });
                    });
                    ui.label("English lyrics, up to 3000 characters. Describe the singing voice in the music prompt.");
                    egui::ScrollArea::vertical().id_salt("lyric-editor-scroll").max_height(160.0).show(ui, |ui| {
                        ui.add(egui::TextEdit::multiline(&mut state.lyrics).desired_rows(5).desired_width(f32::INFINITY).char_limit(3000));
                    });
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!busy, egui::Button::new("Analyze lyrics")).clicked() {
                            workflow.push(("musicforge.lyrics_analyze", json!({})));
                        }
                        ui.label("Estimated syllables, rhyme, repetition and section balance");
                    });
                    if let Some(summary) = state.lyric_analysis.get("summary") {
                        let lyric_lines = summary.get("lyric_lines").and_then(Value::as_u64).unwrap_or(0);
                        let words = summary.get("words").and_then(Value::as_u64).unwrap_or(0);
                        let average = summary.get("average_syllables").and_then(Value::as_f64).unwrap_or(0.0);
                        let scheme = summary.get("rhyme_scheme").and_then(Value::as_str).unwrap_or("");
                        ui.label(format!("{lyric_lines} lyric lines, {words} words, {average:.1} syllables/line, rhyme {scheme}"));
                        if state.lyric_analysis_source != state.lyrics {
                            ui.colored_label(egui::Color32::YELLOW, "Analysis is for an earlier draft. Run it again after editing.");
                        }
                        egui::CollapsingHeader::new("Line analysis").show(ui, |ui| {
                            egui::ScrollArea::vertical().id_salt("lyric-analysis-scroll").max_height(180.0).show(ui, |ui| {
                                if let Some(lines) = state.lyric_analysis.get("lines").and_then(Value::as_array) {
                                    for line in lines.iter().take(120) {
                                        let number = line.get("number").and_then(Value::as_u64).unwrap_or(0);
                                        let syllables = line.get("syllables").and_then(Value::as_u64).unwrap_or(0);
                                        let rhyme = line.get("rhyme").and_then(Value::as_str).unwrap_or("-");
                                        let text = line.get("text").and_then(Value::as_str).unwrap_or("");
                                        let flags = line
                                            .get("flags")
                                            .and_then(Value::as_array)
                                            .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))
                                            .unwrap_or_default();
                                        let suffix = if flags.is_empty() { String::new() } else { format!("  [{flags}]") };
                                        ui.monospace(format!("L{number:02}  {syllables:>2} syl  {rhyme:<2}  {text}{suffix}"));
                                    }
                                }
                            });
                        });
                        ui.small("English syllables and spelling-based rhymes are estimates. Confirm phrasing by listening.");
                    }
                }
                ui.label("Use Generate Voice for spoken vocals on a separate track.");
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
            if music {
                ui.horizontal(|ui| {
                    if ui.button("Mark as A").clicked() { state.compare_a = Some(take.clone()); }
                    if ui.button("Mark as B").clicked() { state.compare_b = Some(take.clone()); }
                    for (label, comparison) in [("A", state.compare_a.clone()), ("B", state.compare_b.clone())] {
                        if let Some(comparison) = comparison
                            && ui.add_enabled(!playing && !recording, egui::Button::new(format!("Hear {label}: {}", comparison.name))).clicked()
                            && let Err(error) = state.audition_path(&comparison.path) { state.status = error; }
                    }
                });
            }
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
        }
        if music && state.remix_view {
            egui::CollapsingHeader::new("Remix project").default_open(true).show(ui, |ui| {
                if ui.add_enabled(!busy && state.take().is_some(), egui::Button::new("Separate selected take into 4 stems")).clicked()
                    && let Err(error) = state.separate()
                {
                    state.status = error;
                }
                ui.label("Import aligned WAV stems, one absolute path per line. Separated stems may contain bleed.");
                ui.add(egui::TextEdit::multiline(&mut state.stem_paths).desired_rows(4).desired_width(f32::INFINITY));
                if ui.button("Import stems as estimated tracks").clicked() {
                    let stems: Vec<_> = state
                        .stem_paths
                        .lines()
                        .filter(|p| !p.trim().is_empty())
                        .map(|p| {
                            let path = p.trim();
                            let name = std::path::Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or("Stem");
                            json!({"path":path,"name":name,"estimated":true})
                        })
                        .collect();
                    workflow.push(("remix.import_stems", json!({"stems":stems,"generation":state.stem_generation})));
                }
                ui.separator();
                ui.add(egui::TextEdit::singleline(&mut state.section_name).hint_text("Section name").char_limit(80));
                ui.horizontal(|ui| {
                    ui.label("Start seconds");
                    ui.add(egui::DragValue::new(&mut state.section_start).range(0.0..=600.0));
                    ui.label("End seconds");
                    ui.add(egui::DragValue::new(&mut state.section_end).range(0.0..=600.0));
                });
                if ui.button("Add section").clicked() {
                    workflow.push((
                        "remix.section",
                        json!({"name":state.section_name,"start":{"seconds":state.section_start},"end":{"seconds":state.section_end}}),
                    ));
                }
                for section in &sections {
                    if section.kind == soundcraft_model::MarkerKind::Selection && ui.button(format!("Loop {}", section.name)).clicked() {
                        workflow.push(("remix.loop_section", json!({"number":section.number})));
                    }
                }
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut state.layer_group).hint_text("Layer group name"));
                    if ui.button("Group selected tracks").clicked() {
                        workflow.push(("track.group", json!({"name":state.layer_group,"edit":true,"mix":true})));
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut state.layer_track).hint_text("Track name for arrangement variants"));
                ui.horizontal(|ui| {
                    if ui.button("Duplicate active playlist").clicked() {
                        workflow.push(("track.playlist_duplicate", json!({"track":state.layer_track})));
                    }
                    ui.label("Alternate playlist index");
                    ui.add(egui::DragValue::new(&mut state.alternate_playlist).range(0..=128));
                });
                if ui.button("Replace section from alternate playlist").clicked() {
                    workflow.push(("track.playlist_promote", json!({"track":state.layer_track,"playlist":state.alternate_playlist,
                        "start":{"seconds":state.section_start},"end":{"seconds":state.section_end}})));
                }
                ui.label("Use the mixer for layer mute, solo, volume, pan and groups. Use alternate playlists to replace a section. Export includes active unmuted tracks.");
                ui.add(egui::TextEdit::singleline(&mut state.remix_dir).hint_text("New export folder"));
                if ui.button("Export project, mix and stems").clicked() {
                    workflow.push(("remix.export", json!({"dir":state.remix_dir})));
                }
            });
        }
        ui.label(&state.status);
    });
    state.open = open;
    let insert_take = if insert {
        state.audition = None;
        state.take().cloned()
    } else {
        None
    };
    for (command, params) in workflow {
        let status = match app.run(command, params) {
            Ok(_) if command == "musicforge.lyrics_analyze" => "Estimated lyric analysis updated.".into(),
            Ok(value) => format!("Saved: {value}"),
            Err(error) => error,
        };
        app.music.status = status;
    }
    if let Some(take) = insert_take {
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
            if let Some(v) = p.get("lyrics").and_then(Value::as_str) {
                state.vocals = v != "[Instrumental]";
                state.lyrics = v.into();
            }
            if let Some(v) = p.get("genre").and_then(Value::as_str) {
                if v.is_empty() || v.chars().count() > 60 {
                    return Some(Err("Use a genre label of 1 to 60 characters".into()));
                }
                state.genre = v.into();
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
        "view" if music => {
            let tab = p.get("tab").and_then(Value::as_str).unwrap_or("");
            Some(match tab {
                "generate" => {
                    state.remix_view = false;
                    Ok(json!({"tab":tab}))
                }
                "remix" => {
                    state.remix_view = true;
                    Ok(json!({"tab":tab}))
                }
                _ => Err("Use tab generate or remix".into()),
            })
        }
        "lyrics_load" if music => {
            if state.job.is_some() {
                return Some(Err("Wait for the current job".into()));
            }
            let index = p.get("index").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok());
            Some(index.and_then(|i| app.engine.session().lyric_versions.get(i)).ok_or_else(|| "Choose a saved lyric version index".to_string()).map(
                |version| {
                    state.lyrics = version.text.clone();
                    state.lead_role = version.lead.clone();
                    state.backing_role = version.backing.clone();
                    state.vocals = true;
                    json!({"loaded":version.name})
                },
            ))
        }
        "lyrics_analyze" if music => {
            if state.job.is_some() {
                return Some(Err("Wait for the current job".into()));
            }
            if let Some(lyrics) = p.get("lyrics") {
                let Some(lyrics) = lyrics.as_str() else {
                    return Some(Err("lyrics must be text".into()));
                };
                if lyrics.trim().is_empty() || lyrics.chars().count() > 3000 {
                    return Some(Err("Enter between 1 and 3000 lyric characters".into()));
                }
                state.lyrics = lyrics.into();
                state.vocals = true;
            }
            Some(state.analyze_lyrics())
        }
        "compare_mark" if music => {
            let slot = p.get("slot").and_then(Value::as_str).unwrap_or("");
            let take = state.take().cloned();
            Some(match (slot, take) {
                ("A", Some(take)) => {
                    state.compare_a = Some(take);
                    Ok(json!({"slot":"A"}))
                }
                ("B", Some(take)) => {
                    state.compare_b = Some(take);
                    Ok(json!({"slot":"B"}))
                }
                _ => Err("Select a take and use slot A or B".into()),
            })
        }
        "compare_audition" if music => {
            if app.engine.transport.playing || app.engine.transport.recording {
                return Some(Err("Stop transport before audition".into()));
            }
            let slot = p.get("slot").and_then(Value::as_str).unwrap_or("");
            let take = match slot {
                "A" => state.compare_a.clone(),
                "B" => state.compare_b.clone(),
                _ => None,
            };
            Some(
                take.ok_or_else(|| "Mark comparison A or B first".to_string())
                    .and_then(|t| state.audition_path(&t.path))
                    .map(|()| json!({"playing":slot})),
            )
        }
        "separate" if music => Some(state.separate().map(|()| json!({"started":true}))),
        "cancel" => {
            state.cancel();
            Some(Ok(json!({"cancelled":true})))
        }
        "inspect" => Some(Ok(json!({"busy":state.job.is_some(),"status":state.status,
            "take":state.take().map(|t| json!({"path":t.path,"settings":t.settings,"name":t.name})),
            "takes":state.takes.iter().enumerate().map(|(i,t)| json!({"index":i,"name":t.name,"settings":t.settings})).collect::<Vec<_>>(),
            "presets":state.presets,"lyrics":state.lyrics,"lead_role":state.lead_role,"backing_role":state.backing_role,
            "lyric_analysis":state.lyric_analysis,
            "stem_paths":state.stem_paths,"stem_generation":state.stem_generation,"separating":state.separating,
            "compare_a":state.compare_a.as_ref().map(|t| &t.name),"compare_b":state.compare_b.as_ref().map(|t| &t.name)}))),
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
        "preset_apply" if music => Some(
            p.get("name")
                .and_then(Value::as_str)
                .and_then(|name| MUSIC_PRESETS.iter().find(|(label, _, _)| *label == name))
                .ok_or_else(|| "Choose an existing music genre".to_string())
                .map(|&(name, prompt, bpm)| {
                    state.genre = name.into();
                    state.text = prompt.into();
                    state.bpm = bpm;
                    json!({"genre":name,"input":prompt,"bpm":bpm})
                }),
        ),
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
