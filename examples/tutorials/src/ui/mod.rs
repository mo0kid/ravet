//! macOS example window, main-thread UI/device ownership and bounded audio handoff.
mod audio;
use crate::synth::{DEFAULT_VALUES, RANGES, SynthPatch};
use audio::{Audio, Command, MAX_FRAMES, QUEUE, Routing, Shared};
use cpal::traits::DeviceTrait;
use ravet_device::{
    ClockedDuplexOptions, ClockedDuplexStream, OutputStream, PreparedOutput, cpal,
    open_clocked_duplex, open_output,
};
use ravet_foundation::{Midi2Kind, Midi2Message, MidiMessage};
use ravet_webview::{Callbacks, StandaloneOptions, StandaloneWindow};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, sync::Arc};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn device_info(
    id: String,
    name: String,
    ranges: Vec<cpal::SupportedStreamConfigRange>,
    default: cpal::SupportedStreamConfig,
) -> Result<Value> {
    let channels = default.channels();
    let ranges = ranges
        .into_iter()
        .filter(|r| r.sample_format() == cpal::SampleFormat::F32 && r.channels() == channels)
        .collect::<Vec<_>>();
    let mut rates = vec![
        8000,
        11025,
        16000,
        22050,
        32000,
        44100,
        48000,
        88200,
        96000,
        176400,
        192000,
        default.sample_rate(),
    ];
    rates.sort_unstable();
    rates.dedup();
    rates.retain(|rate| {
        (8000..=192000).contains(rate)
            && ranges
                .iter()
                .any(|r| r.min_sample_rate() <= *rate && *rate <= r.max_sample_rate())
    });
    let buffers = [32, 64, 128, 256, 512, 1024, 2048, 4096]
        .into_iter()
        .filter(|frames| {
            ranges.iter().any(|r| match r.buffer_size() {
                cpal::SupportedBufferSize::Range { min, max } => min <= frames && frames <= max,
                cpal::SupportedBufferSize::Unknown => true,
            })
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"id":id,"name":name,"channels":channels.min(64),"rates":rates,"default_rate":default.sample_rate(),"buffers":buffers}),
    )
}
fn selected_output_config(device: &cpal::Device, message: &Value) -> Result<cpal::StreamConfig> {
    let default = device.default_output_config()?;
    let rate = u32::try_from(
        message["sample_rate"]
            .as_u64()
            .ok_or("Choose a sample rate")?,
    )?;
    let buffer = u32::try_from(
        message["buffer_size"]
            .as_u64()
            .ok_or("Choose a buffer size")?,
    )?;
    if !(8000..=192000).contains(&rate) || buffer > 65536 || default.channels() > 64 {
        return Err("Unsupported output configuration".into());
    }
    let supported = device.supported_output_configs()?.any(|r| {
        r.sample_format() == cpal::SampleFormat::F32
            && r.channels() == default.channels()
            && r.min_sample_rate() <= rate
            && rate <= r.max_sample_rate()
            && (buffer == 0
                || match r.buffer_size() {
                    cpal::SupportedBufferSize::Range { min, max } => {
                        *min <= buffer && buffer <= *max
                    }
                    cpal::SupportedBufferSize::Unknown => true,
                })
    });
    if !supported {
        return Err("Selected sample rate / buffer size is unavailable".into());
    }
    Ok(cpal::StreamConfig {
        channels: default.channels(),
        sample_rate: rate,
        buffer_size: if buffer == 0 {
            cpal::BufferSize::Default
        } else {
            cpal::BufferSize::Fixed(buffer)
        },
    })
}
fn channel_mask(pairs: &Value, channels: usize) -> Result<u64> {
    let pairs = pairs
        .as_array()
        .filter(|p| !p.is_empty() && p.len() <= 32)
        .ok_or("Select at least one channel pair")?;
    let mut mask = 0u64;
    for pair in pairs {
        let first = usize::try_from(pair.as_u64().ok_or("Invalid channel pair")?)?;
        if first >= channels || first >= 64 || first % 2 != 0 || mask & (1u64 << first) != 0 {
            return Err("Invalid or duplicate channel pair".into());
        }
        mask |= 1u64 << first;
        if first + 1 < channels {
            mask |= 1u64 << (first + 1);
        }
    }
    Ok(mask)
}

enum Running {
    Output(OutputStream),
    Duplex(ClockedDuplexStream),
    Test(Box<PreparedOutput<Audio>>),
}
impl Running {
    fn stop(&self) {
        match self {
            Self::Output(s) => {
                let _ = s.pause();
            }
            Self::Duplex(s) => {
                let _ = s.pause();
            }
            Self::Test(_) => {}
        }
    }
    fn errors(&self) -> bool {
        let s = match self {
            Self::Output(s) => s.statistics(),
            Self::Duplex(s) => s.output_statistics(),
            Self::Test(s) => s.statistics(),
        };
        s.processor_faults != 0 || s.panics != 0 || s.backend_errors != 0
    }
}
struct State {
    mode: String,
    values: [f32; 9],
    delay: [f32; 5],
    demo: bool,
    rate: f64,
    status: String,
    outputs: Vec<Value>,
    inputs: Vec<Value>,
    midis: Vec<Value>,
    producer: Option<rtrb::Producer<Command>>,
    running: Option<Running>,
    midi: Vec<ravet_midi_device::MidiInputStream>,
    shared: Arc<Shared>,
    words: [u32; 80],
    test: bool,
    packets: Vec<String>,
}
impl State {
    fn new(mode: &str, test: bool) -> Self {
        let mut state = Self {
            mode: mode.to_owned(),
            values: DEFAULT_VALUES,
            delay: crate::delay::DEFAULT_VALUES,
            demo: true,
            rate: 48_000.,
            status: "Ready · choose an output and press Start".into(),
            outputs: Vec::new(),
            inputs: Vec::new(),
            midis: Vec::new(),
            producer: None,
            running: None,
            midi: Vec::new(),
            shared: Shared::new(),
            words: [0; 80],
            test,
            packets: Vec::new(),
        };
        if let Err(e) = state.refresh() {
            state.status = e.to_string();
        }
        state
    }
    fn refresh(&mut self) -> Result<()> {
        if self.test {
            self.outputs = vec![
                json!({"id":"test-output","name":"Offline UI fixture","channels":6,"rates":[44100,48000,96000],"default_rate":48000,"buffers":[64,128,256,512,1024]}),
            ];
            self.inputs = vec![
                json!({"id":"test-input","name":"Offline capture fixture","channels":4,"rates":[48000],"default_rate":48000,"buffers":[128,256]}),
            ];
            self.midis = vec![
                json!({"id":"midi-a","name":"Fixture keyboard"}),
                json!({"id":"midi-b","name":"Fixture MPE controller"}),
            ];
            return Ok(());
        }
        if self.mode == "midi2" {
            return Ok(());
        }
        self.outputs = ravet_device::output_devices()?
            .into_iter()
            .map(|d| {
                let device =
                    ravet_device::output_device(Some(&d.id))?.ok_or("Output disappeared")?;
                device_info(
                    d.id,
                    d.name,
                    d.configurations,
                    device.default_output_config()?,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        self.inputs = ravet_device::input_devices()?
            .into_iter()
            .map(|d| {
                let device = ravet_device::input_device(Some(&d.id))?.ok_or("Input disappeared")?;
                device_info(
                    d.id,
                    d.name,
                    d.configurations,
                    device.default_input_config()?,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        self.midis = ravet_midi_device::input_ports()?
            .into_iter()
            .map(|p| json!({"id":p.id,"name":p.name}))
            .collect();
        Ok(())
    }
    fn stop(&mut self) {
        if let Some(running) = self.running.take() {
            running.stop();
            drop(running);
        }
        for midi in self.midi.drain(..) {
            drop(midi.close());
        }
        self.producer = None;
        self.words = [0; 80];
        self.shared = Shared::new();
        self.status = "Stopped".into();
    }
    fn push(&mut self, command: Command) -> Result<()> {
        let Some(producer) = &mut self.producer else {
            return Err("Press Start before playing notes".into());
        };
        if producer.push(command).is_err() {
            self.shared
                .panic
                .store(true, std::sync::atomic::Ordering::Release);
            return Err("Control queue full · notes cleared; retry the control".into());
        }
        Ok(())
    }
    fn start(&mut self, message: &Value) -> Result<()> {
        self.stop();
        let synth = self.mode != "effect";
        let output_id = message["output"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Choose an audio output")?;
        let source = message["input"].as_str().unwrap_or("");
        let demo = !synth && self.demo;
        let (producer, consumer) = rtrb::RingBuffer::new(QUEUE);
        self.producer = Some(producer);
        let result = (|| -> Result<Running> {
            if self.test {
                self.rate = message["sample_rate"].as_u64().unwrap_or(48000) as f64;
                if ![44100., 48000., 96000.].contains(&self.rate) {
                    return Err("Unsupported fixture sample rate".into());
                }
                let routing = Routing {
                    input: 3,
                    output: channel_mask(&message["output_pairs"], 6)?,
                };
                let audio = Audio::new(
                    synth,
                    demo,
                    consumer,
                    Vec::new(),
                    self.shared.clone(),
                    routing,
                );
                return Ok(Running::Test(Box::new(PreparedOutput::new(
                    audio,
                    self.rate as u32,
                    6,
                    MAX_FRAMES,
                )?)));
            }
            let output = ravet_device::output_device(Some(output_id))?
                .ok_or("Selected output is unavailable")?;
            let config = selected_output_config(&output, message)?;
            self.rate = f64::from(config.sample_rate);
            let mut receivers = Vec::new();
            if synth {
                let ids = message["midis"]
                    .as_array()
                    .ok_or("MIDI inputs must be a list")?;
                if ids.len() > 16 {
                    return Err("Select at most 16 MIDI inputs".into());
                }
                let mut unique = std::collections::BTreeSet::new();
                for value in ids {
                    let id = value.as_str().ok_or("Invalid MIDI input ID")?;
                    if !unique.insert(id) {
                        return Err("Duplicate MIDI input".into());
                    }
                    let (stream, receiver) = ravet_midi_device::open_input(id, 256)?;
                    self.midi.push(stream);
                    receivers.push(receiver);
                }
            }
            let output_mask = channel_mask(&message["output_pairs"], usize::from(config.channels))?;
            let input = if !source.is_empty() {
                Some(
                    ravet_device::input_device(Some(source))?
                        .ok_or("Selected input is unavailable")?,
                )
            } else {
                None
            };
            let input_config = input
                .as_ref()
                .map(|device| device.default_input_config())
                .transpose()?;
            if input_config
                .as_ref()
                .is_some_and(|c| c.sample_format() != cpal::SampleFormat::F32)
            {
                return Err("Selected input's default format must be f32".into());
            }
            let input_mask = if let Some(c) = &input_config {
                channel_mask(&message["input_pairs"], usize::from(c.channels()))?
            } else {
                3
            };
            let audio = Audio::new(
                synth,
                demo,
                consumer,
                receivers,
                self.shared.clone(),
                Routing {
                    input: input_mask,
                    output: output_mask,
                },
            );
            if let Some(input) = input {
                let input_config = input_config.expect("selected input config");
                let options = ClockedDuplexOptions {
                    maximum_frames: MAX_FRAMES,
                    ..ClockedDuplexOptions::default()
                };
                Ok(Running::Duplex(open_clocked_duplex(
                    &input,
                    &output,
                    input_config.config(),
                    config,
                    options.clock.capacity_frames,
                    audio,
                    options,
                )?))
            } else {
                Ok(Running::Output(open_output(
                    &output, config, MAX_FRAMES, audio,
                )?))
            }
        })();
        match result {
            Ok(r) => self.running = Some(r),
            Err(e) => {
                self.stop();
                return Err(e);
            }
        }
        if synth {
            self.push(Command::Patch(SynthPatch::new(self.rate, self.values)?))?;
            if self.mode == "mpe" {
                for bytes in [
                    [0xb0, 101, 0],
                    [0xb0, 100, 6],
                    [0xb0, 6, 2],
                    [0xb0, 101, 127],
                    [0xb0, 100, 127],
                ] {
                    self.push(Command::Midi(MidiMessage::from_bytes(&bytes).expect("RPN")))?;
                }
            }
        } else {
            self.push(Command::Delay(self.delay))?;
        }
        let play = match self.running.as_ref().expect("opened stream") {
            Running::Output(s) => s.play().map_err(|e| e.into()),
            Running::Duplex(s) => s.play().map_err(|e| e.into()),
            Running::Test(_) => Ok(()),
        };
        if let Err(e) = play {
            self.stop();
            return Err(e);
        }

        self.status = format!("Running · {} Hz · selected channels", self.rate);
        Ok(())
    }
    fn handle(&mut self, message: &Value) -> Result<()> {
        match message["type"].as_str().ok_or("Missing message type")? {
            "begin" | "end" => {}
            "edit" | "value" => {
                let id = message["id"].as_u64().ok_or("Invalid control ID")?;
                let value = message["value"]
                    .as_f64()
                    .filter(|v| v.is_finite())
                    .ok_or("Invalid control value")? as f32;
                if self.mode == "effect" {
                    if id == 6 {
                        if value != 0. && value != 1. {
                            return Err("Invalid demo selection".into());
                        }
                        if self.running.is_some() {
                            self.push(Command::Demo(value == 1.))?;
                        }
                        self.demo = value == 1.;
                        return Ok(());
                    }
                    let index = usize::try_from(id)
                        .ok()
                        .and_then(|id| id.checked_sub(1))
                        .filter(|i| *i < 5)
                        .ok_or("Invalid effect control")?;
                    let mut values = self.delay;
                    values[index] = value;
                    let mut validation = crate::delay::DelayProcessor::default();
                    validation.set_controls(values)?;
                    if self.running.is_some() {
                        self.push(Command::Delay(values))?;
                    }
                    self.delay = values;
                } else {
                    let index = usize::try_from(id)
                        .ok()
                        .and_then(|id| id.checked_sub(1))
                        .filter(|i| *i < 9)
                        .ok_or("Unknown synth control")?;
                    let mut values = self.values;
                    values[index] = value;
                    let patch = SynthPatch::new(self.rate, values)?;
                    if self.running.is_some() {
                        self.push(Command::Patch(patch))?;
                    }
                    self.values = values;
                }
            }
            "midi" => {
                let bytes = message["bytes"]
                    .as_array()
                    .filter(|b| b.len() <= 3)
                    .ok_or("Invalid MIDI bytes")?;
                let mut data = [0; 3];
                for (i, value) in bytes.iter().enumerate() {
                    data[i] = u8::try_from(value.as_u64().ok_or("Invalid MIDI byte")?)?;
                }
                let midi = MidiMessage::from_bytes(&data[..bytes.len()])
                    .map_err(|_| "Invalid MIDI message")?;
                self.push(Command::Midi(midi))?;
            }
            "packet" => {
                let field = |name: &str, max: u64| -> Result<u64> {
                    message[name]
                        .as_u64()
                        .filter(|n| *n <= max)
                        .ok_or_else(|| format!("Invalid MIDI 2.0 {name}").into())
                };
                let group = field("group", 15)? as u8;
                let channel = field("channel", 15)? as u8;
                let key = field("key", 127)? as u8;
                let kind = match message["kind"].as_str() {
                    Some("on") => Midi2Kind::NoteOn {
                        key,
                        velocity: field("velocity", 65535)? as u16,
                        attribute_type: 0,
                        attribute: 0,
                    },
                    Some("off") => Midi2Kind::NoteOff {
                        key,
                        velocity: field("velocity", 65535)? as u16,
                        attribute_type: 0,
                        attribute: 0,
                    },
                    Some("bend") => Midi2Kind::PerNotePitchBend {
                        key,
                        value: field("expression", u32::MAX as u64)? as u32,
                    },
                    Some("pressure") => Midi2Kind::PolyPressure {
                        key,
                        value: field("expression", u32::MAX as u64)? as u32,
                    },
                    _ => return Err("Unknown MIDI 2.0 kind".into()),
                };
                let typed = Midi2Message::new(group, channel, kind)
                    .map_err(|_| "Invalid MIDI 2.0 fields")?;
                let packet = typed.packet();
                let decoded = Midi2Message::from_packet(packet).map_err(|_| "Invalid packet")?;
                let words = packet
                    .as_words()
                    .iter()
                    .map(|w| format!("{w:08x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                if self.packets.len() == 16 {
                    self.packets.remove(0);
                }
                self.packets.push(format!(
                    "{words}\nGroup {group} · Channel {channel} · {:?}",
                    decoded.kind()
                ));
                self.status = "MIDI 2.0 fields preserved · packet inspector".into();
            }
            "action" => match message["action"].as_str() {
                Some("start") => self.start(message)?,
                Some("stop") => self.stop(),
                Some("panic") => {
                    self.shared
                        .panic
                        .store(true, std::sync::atomic::Ordering::Release);
                    self.status = "All notes off".into();
                }
                Some("refresh") => self.refresh()?,
                Some("mode") => {
                    let mode = message["mode"]
                        .as_str()
                        .filter(|m| ["synth", "effect", "mpe", "midi2"].contains(m))
                        .ok_or("Unknown example")?;
                    self.stop();
                    self.mode = mode.to_owned();
                    self.refresh()?;
                }
                _ => return Err("Unknown action".into()),
            },
            _ => return Err("Unknown message".into()),
        }
        Ok(())
    }
    fn render_test(&mut self) {
        if let Some(Running::Test(renderer)) = &mut self.running {
            let mut samples = [0.; 768];
            renderer.render(&mut samples);
        }
    }
    fn snapshot(&mut self) -> Value {
        if let Some(snapshot) = self.shared.telemetry.try_read() {
            self.words = snapshot.words;
        }
        if self.running.as_ref().is_some_and(|r| r.errors()) {
            self.status = "Audio backend/processor fault · stop and choose the device again".into();
        }
        let word = |index: usize| f32::from_bits(self.words[index]);
        let meters = (0..4)
            .map(|i| json!({"peak":word(2+i*2),"rms":word(3+i*2),"nonfinite":false}))
            .collect::<Vec<_>>();
        let voices=(0..8).filter_map(|i|{let s=12+i*8;let id=self.words[s];let phase=(id>>16)&255;
            (phase!=0).then(||json!({"channel":id&255,"key":(id>>8)&255,"phase":(["Idle","Held","Sustained","Released"][phase as usize]),
                "velocity":word(s+1),"bend":word(s+2)+word(s+3),"pressure":word(s+4),"timbre":word(s+5),"envelope":word(s+6)}))}).collect::<Vec<_>>();
        json!({"mode":self.mode,"values":self.values,"delay":self.delay,"demo":self.demo,"running":self.running.is_some(),"status":self.status,
            "io":{"outputs":self.outputs,"inputs":self.inputs,"midis":self.midis,"audio":self.mode!="midi2","midi":self.mode=="synth"||self.mode=="mpe","input":self.mode!="midi2","note":if self.mode=="effect" {"Stop audio to change settings. Selected input pairs are mixed to stereo."} else {"Stop audio to change settings. Audio input is optional; notes come from MIDI or the keyboard."}},"meters":meters,"voices":voices,"packets":self.packets,
            "frames":u64::from(self.words[0])|(u64::from(self.words[1])<<32),"midiDrops":self.words[10],"ranges":RANGES})
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.stop();
    }
}
struct Bridge(Rc<RefCell<State>>);
impl Callbacks for Bridge {
    fn message(&mut self, text: &str) {
        let mut state = self.0.borrow_mut();
        let result = (|| -> Result<()> {
            if text.len() > 4096 {
                return Err("Message too large".into());
            }
            let message: Value = serde_json::from_str(text)?;
            state.handle(&message)
        })();
        if let Err(e) = result {
            state.status = e.to_string();
        }
    }
    fn snapshot(&mut self) -> Option<String> {
        Some(self.0.borrow_mut().snapshot().to_string())
    }
}

/// Open the native example window. Devices start only from the page's Start action.
/// `--ui-test directory` runs a hidden, hardware-free WebKit/audio fixture instead.
pub fn run(mode: &str) -> Result<()> {
    if !["synth", "effect", "mpe", "midi2"].contains(&mode) {
        return Err("Unknown example".into());
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--ui-test") {
        if args.len() != 2 {
            return Err("usage: --ui-test <screenshot-directory>".into());
        }
        return probe(std::path::Path::new(&args[1]));
    }
    if !args.is_empty() {
        return Err(
            "usage: no arguments for live UI; --render [file.wav] or --offline for fixtures".into(),
        );
    }
    let state = Rc::new(RefCell::new(State::new(mode, false)));
    let window = StandaloneWindow::new(StandaloneOptions {
        title: "RAVET · Live examples".into(),
        lock_aspect_ratio: true,
        ..StandaloneOptions::default()
    })?;
    let html = ravet_surface::embed_web_controls(include_str!("../../editor/index.html"))
        .ok_or("Missing example control placeholders")?;
    let editor = window.embed(&html, Bridge(state.clone()))?;
    editor.set_visible(true);
    while window.pump(0.01) {}
    state.borrow_mut().stop();
    drop(editor);
    drop(window);
    Ok(())
}

fn probe(directory: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(directory)?;
    {
        let state = Rc::new(RefCell::new(State::new("synth", true)));
        let window = StandaloneWindow::new(StandaloneOptions {
            hidden: true,
            io_settings: false,
            ..StandaloneOptions::default()
        })?;
        let editor = window.embed(
            "<body><script>window.ready=true</script></body>",
            Bridge(state),
        )?;
        for _ in 0..20 {
            window.pump(0.02);
        }
        assert!(!window.open_settings());
        assert!(window.eval("window.ready===true&&!document.getElementById('io-settings')")?);
        drop(editor);
        drop(window);
    }

    for mode in ["synth", "effect", "mpe", "midi2"] {
        let state = Rc::new(RefCell::new(State::new(mode, true)));
        let window = StandaloneWindow::new(StandaloneOptions {
            hidden: true,
            title: format!("RAVET · UI fixture · {mode}"),
            lock_aspect_ratio: true,
            ..StandaloneOptions::default()
        })?;
        let html = ravet_surface::embed_web_controls(include_str!("../../editor/index.html"))
            .ok_or("Missing example control placeholders")?;
        let editor = window.embed(&html, Bridge(state.clone()))?;
        editor.set_visible(true);
        let wait = |script: &str| -> Result<()> {
            for _ in 0..80 {
                if window.eval(script)? {
                    return Ok(());
                }
                window.pump(0.05);
            }
            Err(format!("UI assertion timed out: {script}").into())
        };
        wait("window.ravetExampleReady===true")?;
        assert!(window.open_settings());
        wait("document.getElementById('io-settings').open")?;
        if mode != "midi2" {
            assert!(window.eval("document.getElementById('audio-output').value='test-output';document.getElementById('audio-output').dispatchEvent(new Event('change'));true")?);
            wait(
                "document.getElementById('sample-rate').value==='48000'&&document.querySelectorAll('#output-channels input').length===3",
            )?;
            assert!(window.eval("document.getElementById('buffer-size').value='128';document.getElementById('buffer-size').selectedOptions[0].textContent.includes('2.7 ms')")?);
            if mode != "effect" {
                assert!(window.eval("document.querySelectorAll('#midi-inputs input').length===2&&document.querySelectorAll('#midi-inputs input:checked').length===0")?);
            }
            assert!(window.eval("document.getElementById('audio-input').value='test-input';document.getElementById('audio-input').dispatchEvent(new Event('change'));document.querySelectorAll('#input-channels input').length===2")?);
            assert!(window.snapshot(&directory.join(format!("{mode}-settings.png")))?);
            assert!(window.eval("document.getElementById('audio-input').value='';document.getElementById('audio-input').dispatchEvent(new Event('change'));true")?);
        }
        assert!(window.eval("document.getElementById('close-settings').click();true")?);
        if mode == "synth" {
            assert!(window.eval("Array.from(document.querySelectorAll('#keyboard .ravet-piano-key')).every(key=>key.disabled&&getComputedStyle(key).opacity==='1')")?, "stopped piano keys must remain opaque");
            assert!(window.snapshot(&directory.join("synth-stopped.png"))?);
            knob_fixture(&window)?;
        }

        if mode == "midi2" {
            assert!(window.eval("document.getElementById('encode').click();true")?);
            wait("document.getElementById('packets').textContent.includes('41923c00 abcd0000')")?;
        } else {
            assert!(window.eval(
                "window.ravetOpenSettings();document.getElementById('start').click();true"
            )?);
            for _ in 0..20 {
                state.borrow_mut().render_test();
                window.pump(0.02);
            }
            assert!(
                state.borrow().running.is_some(),
                "{}",
                state.borrow().status
            );
            wait("!document.getElementById('io-settings').open")?;
            if mode == "synth" {
                piano_fixture(&window)?;
            }

            if mode == "effect" {
                assert!(window.eval("window.ravetPost({type:'edit',id:1,value:250});window.ravetPost({type:'edit',id:2,value:0.5});window.ravetPost({type:'edit',id:3,value:0.6});window.ravetPost({type:'edit',id:5,value:1});true")?);
            } else {
                assert!(window.eval("window.ravetPost({type:'edit',id:1,value:2});document.querySelector('[data-note=\"60\"]').dispatchEvent(new PointerEvent('pointerdown',{pointerId:1,bubbles:true}));true")?);
            }
            for _ in 0..if mode == "effect" { 100 } else { 30 } {
                state.borrow_mut().render_test();
                window.pump(0.02);
            }
            assert!(state.borrow().words[0] > 0);
            assert!(f32::from_bits(state.borrow().words[6]) > 0.0001);
            assert!(window.snapshot(&directory.join(format!("{mode}-playing.png")))?);
            if mode == "effect" {
                assert_eq!(state.borrow().delay, [250., 0.5, 0.6, 0., 1.]);
                wait(
                    "document.getElementById('title').textContent==='Stereo delay'&&document.querySelectorAll('#delay-controls [role=slider]').length===3&&document.getElementById('delay-number-1').value==='250'&&document.getElementById('ping-pong').checked",
                )?;
                assert!(window.eval("document.getElementById('bypass').click();true")?);
                wait("document.getElementById('bypass').checked")?;
                assert_eq!(state.borrow().delay[3], 1.);
                assert!(window.eval("document.getElementById('main-stop').click();true")?);
                wait(
                    "document.getElementById('main-stop').textContent==='Start audio'&&!document.getElementById('main-stop').disabled",
                )?;
                assert!(state.borrow().running.is_none());
                assert!(window.eval("document.getElementById('main-stop').click();true")?);
                wait("document.getElementById('main-stop').textContent==='Stop audio'")?;
                assert!(state.borrow().running.is_some());
                assert!(window.eval("document.getElementById('demo-loop').click();true")?);
                wait("!document.getElementById('demo-loop').checked")?;
                assert!(!state.borrow().demo);
                assert!(window.eval("Array.from(document.getElementById('audio-input').options).every(option=>!option.textContent.includes('Demo')&&option.value!=='demo')")?);
            }
            if mode != "effect" {
                assert_eq!(state.borrow().values[0], 2.);
                assert!(window.eval("document.querySelector('[data-note=\"60\"]').dispatchEvent(new PointerEvent('pointerup',{pointerId:1,bubbles:true}));true")?);
                for _ in 0..150 {
                    state.borrow_mut().render_test();
                }
                assert!(
                    state.borrow_mut().snapshot()["voices"]
                        .as_array()
                        .expect("voices")
                        .is_empty()
                );
            }
        }
        window.pump(0.1);
        layout_fixture(&window, mode, directory)?;
        assert!(window.snapshot(&directory.join(format!("{mode}.png")))?);
        state.borrow_mut().stop();
        drop(editor);
        drop(window);
        println!("PASS {mode} native UI/control/audio fixture (no device opened)");
    }
    settings_fixture()?;
    Ok(())
}

fn layout_fixture(
    window: &StandaloneWindow,
    mode: &str,
    directory: &std::path::Path,
) -> Result<()> {
    assert!(window.eval("window.ravetLayoutBaseline=Array.from(document.querySelectorAll('header,h1,.tabs,.dial,.panel,input[type=number],footer')).filter(node=>{const rect=node.getBoundingClientRect();return rect.width>0&&rect.height>0}).map(node=>({node,rect:node.getBoundingClientRect()}));window.ravetExampleScale===1")?);
    for (width, height, actual_width, actual_height, scale, label) in [
        (720., 624., 720, 624, 0.8, "minimum"),
        (1080., 936., 1080, 936, 1.2, "large"),
        (1200., 624., 720, 624, 0.8, "wide-short"),
        (720., 1000., 720, 624, 0.8, "narrow-tall"),
        (900., 780., 900, 780, 1., "default"),
    ] {
        assert!(window.set_size(width, height));
        let check = format!(
            "window.innerWidth==={actual_width}&&window.innerHeight==={actual_height}&&Math.abs(window.ravetExampleScale-{scale})<0.001"
        );
        let mut ready = false;
        for _ in 0..80 {
            if window.eval(&check)? {
                ready = true;
                break;
            }
            window.pump(0.05);
        }
        assert!(ready, "{mode} {label} native aspect/viewport mismatch");
        assert!(window.eval(&format!("window.ravetLayoutBaseline.every(item=>{{const rect=item.node.getBoundingClientRect();return Math.abs(rect.width-item.rect.width*{scale})<1&&Math.abs(rect.height-item.rect.height*{scale})<1&&Math.abs(rect.top-item.rect.top*{scale})<1&&Math.abs(rect.left-item.rect.left*{scale})<1}})"))?,"{mode} {label} must scale all UI dimensions and positions uniformly");
        assert!(window.eval("window.ravetLayoutBaseline.every(item=>{const rect=item.node.getBoundingClientRect();return rect.left>=-1&&rect.top>=-1&&rect.right<=window.innerWidth+1&&rect.bottom<=window.innerHeight+1})&&document.documentElement.scrollHeight<=window.innerHeight+1&&document.documentElement.scrollWidth<=window.innerWidth+1")?,"{mode} {label} content clipped/overflows");
        if mode != "midi2" {
            assert!(window.eval("Array.from(document.querySelectorAll('#meters-panel .reading')).every(reading=>reading.getBoundingClientRect().bottom<=document.getElementById('meters-panel').getBoundingClientRect().bottom-1)")?,"{mode} {label} meter readings overflow");
        }
        if label == "minimum" || label == "large" {
            assert!(window.snapshot(&directory.join(format!("{mode}-{label}.png")))?);
        }
    }
    Ok(())
}

fn settings_fixture() -> Result<()> {
    for pass in 0..3 {
        let state = Rc::new(RefCell::new(State::new("synth", true)));
        let window = StandaloneWindow::new(StandaloneOptions {
            hidden: true,
            title: "RAVET · settings restore fixture".into(),
            ..Default::default()
        })?;
        let html =
            ravet_surface::embed_web_controls(include_str!("../../editor/index.html")).unwrap();
        let editor = window.embed(&html, Bridge(state.clone()))?;
        editor.set_visible(true);
        for _ in 0..80 {
            if window.eval("window.ravetExampleReady===true")? {
                break;
            }
            window.pump(0.05);
        }
        assert!(window.eval("window.ravetExampleReady===true")?);
        if pass == 0 {
            assert!(window.eval(r#"(()=>{
                const get=id=>document.getElementById(id),change=node=>node.dispatchEvent(new Event('change',{bubbles:true}));
                get('audio-output').value='test-output';change(get('audio-output'));
                get('audio-input').value='test-input';change(get('audio-input'));
                get('sample-rate').value='96000';change(get('sample-rate'));
                get('buffer-size').value='256';change(get('buffer-size'));
                for(const [id,value] of [['output-channels','4'],['input-channels','2'],['midi-inputs','midi-b']]){
                    for(const check of get(id).querySelectorAll('input'))check.checked=check.value===value;
                    change(get(id));
                }return true;
            })()"#)?);
            window.pump(0.1);
        } else if pass == 1 {
            assert!(window.eval(r#"document.getElementById('audio-output').value==='test-output'&&document.getElementById('audio-input').value==='test-input'&&document.getElementById('sample-rate').value==='96000'&&document.getElementById('buffer-size').value==='256'&&['output-channels','input-channels','midi-inputs'].every(id=>document.querySelectorAll('#'+id+' input:checked').length===1)&&document.querySelector('#output-channels input:checked').value==='4'&&document.querySelector('#input-channels input:checked').value==='2'&&document.querySelector('#midi-inputs input:checked').value==='midi-b'"#)?);
            assert!(state.borrow().running.is_none());
            assert!(window.eval("window.webkit.messageHandlers.ravetSettings.postMessage({version:1,output:'missing-output',input:'missing-input',sample_rate:96000,buffer_size:256,output_pairs:[0],input_pairs:[0],midis:['missing-midi']});true")?);
            window.pump(0.1);
        } else {
            assert!(window.eval("document.getElementById('audio-output').value===''&&document.getElementById('audio-input').value===''&&document.querySelectorAll('#midi-inputs input:checked').length===0")?);
        }
        drop(editor);
        drop(window);
    }
    println!(
        "PASS Audio/MIDI selection restore, unavailable devices and no automatic start (in-memory native preferences fixture)"
    );
    Ok(())
}

fn knob_fixture(window: &StandaloneWindow) -> Result<()> {
    assert!(window.eval(r#"(()=>{
        if(document.querySelectorAll('input[type=range]').length!==0)return false;
        const dial=document.createElement('div'),number=document.createElement('input');number.type='number';
        const messages=[],knob=new RavetControls.Knob(dial,number,{id:90,minimum:20,maximum:20000,step:1,value:200,logarithmic:true},m=>{messages.push(m);return true});
        dial.dispatchEvent(new PointerEvent('pointerdown',{button:0,pointerId:99,clientY:100}));
        dial.dispatchEvent(new PointerEvent('pointermove',{pointerId:99,clientY:50}));
        if(knob.update(400)||Math.abs(knob.current-2000)>1)return false;
        dial.dispatchEvent(new PointerEvent('pointercancel',{pointerId:99}));
        if(messages.map(m=>m.type).join(',')!=='begin,value,end')return false;
        knob.update(200);dial.dispatchEvent(new KeyboardEvent('keydown',{key:'End'}));dial.dispatchEvent(new KeyboardEvent('keyup',{key:'End'}));
        if(knob.current!==20000)return false;
        number.value='-1';number.dispatchEvent(new Event('change'));if(knob.current!==20)return false;
        if(!knob.dispose())return false;
        let rejected=true;const retry=new RavetControls.Knob(dial,null,{id:91,minimum:0,maximum:1,value:.6},m=>m.type!=='end'||!rejected);
        retry.begin();if(retry.dispose()||!retry.active)return false;rejected=false;if(!retry.dispose())return false;
        const denied=new RavetControls.Knob(dial,null,{id:92,minimum:0,maximum:1},()=>false);if(denied.begin()||denied.active||!denied.dispose())return false;
        return true;
    })()"#)?);
    Ok(())
}

fn piano_fixture(window: &StandaloneWindow) -> Result<()> {
    assert!(window.eval(r#"(()=>{
        const root=document.getElementById('keyboard'),white=root.querySelector('[data-note="60"]'),black=root.querySelector('[data-note="61"]');
        const w=white.getBoundingClientRect(),b=black.getBoundingClientRect();
        if(b.height>=w.height*.7||b.width>=w.width||b.left>=w.right||b.left<=w.left||root.querySelectorAll('.white').length!==15||root.querySelectorAll('.black').length!==10)return false;
        const outer=document.createElement('div');outer.style.cssText='position:fixed;left:10px;top:10px;width:300px;height:100px;z-index:1000';document.body.append(outer);
        const messages=[],active=new Map();
        const piano=new RavetControls.PianoKeyboard(outer,{firstNote:60,lastNote:72,onNoteOn:(note,source)=>{active.set(source,note);messages.push(['on',note]);return true},onNoteOff:source=>{messages.push(['off',active.get(source)]);active.delete(source)}});
        const key=note=>outer.querySelector(`[data-note="${note}"]`),pointer=(node,type,id,x,y)=>node.dispatchEvent(new PointerEvent(type,{button:0,pointerId:id,clientX:x,clientY:y,bubbles:true}));
        const c=key(60).getBoundingClientRect(),e=key(64).getBoundingClientRect(),d=key(62).getBoundingClientRect();
        pointer(key(60),'pointerdown',31,c.left+5,c.bottom-5);pointer(key(64),'pointerdown',32,e.left+5,e.bottom-5);
        if(active.size!==2)return false;
        pointer(outer,'pointermove',31,d.left+5,d.bottom-5);if(messages.slice(-2).map(m=>m.join(':')).join(',')!=='off:60,on:62')return false;
        pointer(outer,'pointercancel',31,0,0);if(active.size!==1)return false;
        piano.releaseAll();if(active.size!==0)return false;
        piano.setActiveNotes([61]);if(!key(61).classList.contains('held')||messages.length!==6)return false;
        key(60).dispatchEvent(new KeyboardEvent('keydown',{key:' ',bubbles:true}));key(60).dispatchEvent(new KeyboardEvent('keyup',{key:' ',bubbles:true}));
        if(active.size!==0||messages.slice(-2).map(m=>m.join(':')).join(',')!=='on:60,off:60')return false;
        piano.dispose();outer.remove();return true;
    })()"#)?);
    Ok(())
}
