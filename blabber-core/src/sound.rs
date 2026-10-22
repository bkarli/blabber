//! Shared audio engine: owns cpal device selection and stream lifecycle
//! centrally, and mixes all audio sources (live mesh call audio, one-shot
//! MP3 sound effects) into a single output stream.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, StreamConfig};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

const WIRE_SAMPLE_RATE: u32 = 48000;
const MIX_CHUNK_MS: u64 = 10;
/// The mixer always produces chunks of this size. Large chunks would pull a
/// peer's jitter buffer empty in one go (silence) and let it overflow in
/// between (trimmed audio).
const MIX_CHUNK_SAMPLES: usize = WIRE_SAMPLE_RATE as usize * MIX_CHUNK_MS as usize / 1000;
/// Lower bound for how much mixed audio waits between mixer and sound card.
const OUTPUT_QUEUE_MIN_TARGET: usize = WIRE_SAMPLE_RATE as usize * 30 / 1000;
const LEVELER_TARGET_RMS: f32 = 0.15;
/// Block RMS above which the mic counts as speaking.
const LEVELER_NOISE_GATE_RMS: f32 = 0.004;
const LEVELER_MAX_GAIN: f32 = 8.0;
/// AGC smoothing: gain comes down fast on loud input, rises slowly.
const LEVELER_ATTACK_MS: f32 = 15.0;
const LEVELER_RELEASE_MS: f32 = 300.0;
/// Gate envelope: opens fast so word onsets aren't cut, stays open through
/// short pauses
const GATE_OPEN_MS: f32 = 5.0;
const GATE_HOLD_MS: f32 = 200.0;
const GATE_CLOSE_MS: f32 = 50.0;
/// Output level up to which the limiter is fully linear.
const LIMITER_KNEE: f32 = 0.8;

fn downmix_to_mono(data: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    let channels = channels as usize;
    data.chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

fn upmix_from_mono(data: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    let channels = channels as usize;
    let mut out = Vec::with_capacity(data.len() * channels);
    for &sample in data {
        for _ in 0..channels {
            out.push(sample);
        }
    }
    out
}

/// Per-sample smoothing coefficient for a one-pole filter with time constant `ms`.
fn one_pole_coeff(ms: f32, sample_rate: u32) -> f32 {
    1.0 - (-1.0 / (ms / 1000.0 * sample_rate as f32)).exp()
}

/// Linear below LIMITER_KNEE, then rounds off towards 1.0.
fn soft_limit(x: f32) -> f32 {
    let magnitude = x.abs();
    if magnitude <= LIMITER_KNEE {
        return x;
    }
    let headroom = 1.0 - LIMITER_KNEE;
    let limited = LIMITER_KNEE + headroom * ((magnitude - LIMITER_KNEE) / headroom).tanh();
    limited.copysign(x)
}

/// Mic level control: an automatic gain towards LEVELER_TARGET_RMS plus a
/// noise gate. Both are separate envelopes: the gate decides whether the mic
/// is heard, the AGC decides*how loud and
/// only adapts while someone is speaking, so it never pumps up background
/// noise during pauses. Gains are interpolated per sample to avoid zipper noise.
struct RmsLeveler {
    agc_gain: f32,
    gate: f32,
    hold_left: usize,
}

impl RmsLeveler {
    fn new() -> Self {
        Self { agc_gain: 1.0, gate: 0.0, hold_left: 0 }
    }

    fn process(&mut self, input: &[f32], sample_rate: u32) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }

        let sum_sq: f32 = input.iter().map(|&s| s * s).sum();
        let rms = (sum_sq / input.len() as f32).sqrt();
        let speaking = rms >= LEVELER_NOISE_GATE_RMS;

        let start_gain = self.agc_gain;
        if speaking {
            self.hold_left = (GATE_HOLD_MS / 1000.0 * sample_rate as f32) as usize;
            let desired = (LEVELER_TARGET_RMS / rms).min(LEVELER_MAX_GAIN);
            let time_constant_ms = if desired < self.agc_gain { LEVELER_ATTACK_MS } else { LEVELER_RELEASE_MS };
            let block_s = input.len() as f32 / sample_rate as f32;
            let coeff = 1.0 - (-block_s / (time_constant_ms / 1000.0)).exp();
            self.agc_gain += (desired - self.agc_gain) * coeff;
        } else {
            self.hold_left = self.hold_left.saturating_sub(input.len());
        }
        let gate_target = if speaking || self.hold_left > 0 { 1.0 } else { 0.0 };
        let gate_coeff = if gate_target > self.gate {
            one_pole_coeff(GATE_OPEN_MS, sample_rate)
        } else {
            one_pole_coeff(GATE_CLOSE_MS, sample_rate)
        };

        let len = input.len() as f32;
        input.iter()
            .enumerate()
            .map(|(i, &sample)| {
                let gain = start_gain + (self.agc_gain - start_gain) * (i + 1) as f32 / len;
                self.gate += (gate_target - self.gate) * gate_coeff;
                soft_limit(sample * gain * self.gate)
            })
            .collect()
    }
}

/// Linear-interpolating resampler for a continuous stream fed in chunks.
struct LinearResampler {
    step: f64,
    passthrough: bool,
    /// read position of the next output, where 0 is `prev` and 1 is the
    /// current chunk's first sample
    pos: f64,
    prev: Option<f32>,
}

impl LinearResampler {
    fn new(from_rate: u32, to_rate: u32) -> Self {
        Self {
            step: from_rate as f64 / to_rate.max(1) as f64,
            passthrough: from_rate == to_rate,
            // start on the first real sample, `prev` is only a placeholder then
            pos: 1.0,
            prev: None,
        }
    }

    fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if self.passthrough || input.is_empty() {
            return input.to_vec();
        }
        let prev = *self.prev.get_or_insert(input[0]);
        let at = |i: usize| if i == 0 { prev } else { input[i - 1] };

        let len = input.len();
        let mut output = Vec::with_capacity((len as f64 / self.step) as usize + 1);
        while self.pos < len as f64 {
            let idx = self.pos.floor() as usize;
            let frac = (self.pos - idx as f64) as f32;
            let s0 = at(idx);
            output.push(s0 + (at(idx + 1) - s0) * frac);
            self.pos += self.step;
        }
        self.pos -= len as f64;
        self.prev = Some(input[len - 1]);
        output
    }
}

/// Opens input stream regardless of the device native sample format.
fn open_input_stream(
    device: &cpal::Device,
    config: cpal::SupportedStreamConfig,
    mut process: impl FnMut(&[f32]) + Send + 'static,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream> {
    let format = config.sample_format();
    let stream_config: StreamConfig = config.into();

    macro_rules! build {
        ($sample_ty:ty) => {
            device.build_input_stream(
                &stream_config,
                move |data: &[$sample_ty], _: &cpal::InputCallbackInfo| {
                    let converted: Vec<f32> = data.iter().map(|&s| s.to_sample::<f32>()).collect();
                    process(&converted);
                },
                err_fn,
                None,
            )
        };
    }

    let stream = match format {
        SampleFormat::F32 => build!(f32),
        SampleFormat::F64 => build!(f64),
        SampleFormat::I8 => build!(i8),
        SampleFormat::I16 => build!(i16),
        SampleFormat::I32 => build!(i32),
        SampleFormat::U8 => build!(u8),
        SampleFormat::U16 => build!(u16),
        SampleFormat::U32 => build!(u32),
        other => return Err(anyhow!("unsupported input sample format: {other}")),
    }
    .context("failed to build input stream")?;

    Ok(stream)
}

/// Opens an output stream regardless of the device's native sample format.
/// Mirrors `open_input_stream`: the caller fills an f32 scratch buffer and
/// this converts it into whatever the device actually wants.
fn open_output_stream(
    device: &cpal::Device,
    config: cpal::SupportedStreamConfig,
    mut supply: impl FnMut(&mut [f32]) + Send + 'static,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream> {
    let format = config.sample_format();
    let stream_config: StreamConfig = config.into();

    macro_rules! build {
        ($sample_ty:ty) => {{
            let mut scratch: Vec<f32> = Vec::new();
            device.build_output_stream(
                &stream_config,
                move |data: &mut [$sample_ty], _: &cpal::OutputCallbackInfo| {
                    scratch.resize(data.len(), 0.0);
                    supply(&mut scratch);
                    for (out, &s) in data.iter_mut().zip(scratch.iter()) {
                        *out = s.to_sample::<$sample_ty>();
                    }
                },
                err_fn,
                None,
            )
        }};
    }

    let stream = match format {
        SampleFormat::F32 => build!(f32),
        SampleFormat::F64 => build!(f64),
        SampleFormat::I8 => build!(i8),
        SampleFormat::I16 => build!(i16),
        SampleFormat::I32 => build!(i32),
        SampleFormat::U8 => build!(u8),
        SampleFormat::U16 => build!(u16),
        SampleFormat::U32 => build!(u32),
        other => return Err(anyhow!("unsupported output sample format: {other}")),
    }
    .context("failed to build output stream")?;

    Ok(stream)
}

/// One entry in the device list returned to the UI.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
}

fn devices_with_default(devices: impl Iterator<Item = cpal::Device>, default: Option<String>) -> Vec<AudioDeviceInfo> {
    devices
        .filter_map(|d| d.name().ok())
        .map(|name| {
            let is_default = default.as_deref() == Some(name.as_str());
            AudioDeviceInfo { name, is_default }
        })
        .collect()
}

fn find_input_device(host: &cpal::Host, name: &Option<String>) -> Result<cpal::Device> {
    match name {
        Some(wanted) => host
            .input_devices()?
            .find(|d| d.name().ok().as_deref() == Some(wanted.as_str()))
            .ok_or_else(|| anyhow!("input device '{wanted}' not found")),
        None => host
            .default_input_device()
            .ok_or_else(|| anyhow!("no default input device available")),
    }
}

fn find_output_device(host: &cpal::Host, name: &Option<String>) -> Result<cpal::Device> {
    match name {
        Some(wanted) => host
            .output_devices()?
            .find(|d| d.name().ok().as_deref() == Some(wanted.as_str()))
            .ok_or_else(|| anyhow!("output device '{wanted}' not found")),
        None => host
            .default_output_device()
            .ok_or_else(|| anyhow!("no default output device available")),
    }
}

/// A single contributor to the output mix. Implementations must ADD their contribution onto `out`
/// never overwrite it, since other voices may already have written into it
/// this tick.
pub trait MixSource: Send {
    fn mix_into(&mut self, out: &mut [f32]);
    /// One-shot voices report true once exhausted so the mixer drops them.
    fn is_finished(&self) -> bool {
        false
    }
}

type VoiceEntry = (u64, Box<dyn MixSource>);
type VoiceList = Arc<StdMutex<Vec<VoiceEntry>>>;

/// Keeps a registered voice in the mix. dropping removes the voice.
pub struct VoiceHandle {
    id: u64,
    voices: VoiceList,
}

impl Drop for VoiceHandle {
    fn drop(&mut self) {
        self.voices.lock().unwrap().retain(|(id, _)| *id != self.id);
    }
}

/// A decoded sound effect, mono and already resampled to the engine
/// rate so playback needs no more processing.
#[derive(Clone)]
pub struct DecodedSound {
    pub samples: Arc<Vec<f32>>,
}

/// Decodes an MP3 file fully as mono f32 samples at the engine's
/// sample rate.
pub fn decode_mp3_to_wire_rate(bytes: &[u8]) -> Result<DecodedSound> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let cursor = std::io::Cursor::new(bytes.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .context("failed to probe mp3 data")?;
    let mut format = probed.format;

    let track = format.default_track().context("no default track in mp3 data")?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("failed to create mp3 decoder")?;

    let mut interleaved: Vec<f32> = Vec::new();
    let mut source_rate = WIRE_SAMPLE_RATE;
    let mut source_channels: u16 = 1;
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(anyhow!("failed reading mp3 packet: {e}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(anyhow!("failed decoding mp3 packet: {e}")),
        };
        if sample_buf.is_none() {
            let spec = *decoded.spec();
            source_rate = spec.rate;
            source_channels = spec.channels.count() as u16;
            sample_buf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
        }
        if let Some(buf) = sample_buf.as_mut() {
            buf.copy_interleaved_ref(decoded);
            interleaved.extend_from_slice(buf.samples());
        }
    }

    if interleaved.is_empty() {
        return Err(anyhow!("mp3 data decoded to zero samples"));
    }

    let mono = downmix_to_mono(&interleaved, source_channels);
    let resampled = LinearResampler::new(source_rate, WIRE_SAMPLE_RATE).process(&mono);
    Ok(DecodedSound { samples: Arc::new(resampled) })
}

/// Length of the linear fade applied to the start/end of a sound effect, to
/// avoid the audible click a hard start/stop would
/// produce mid-waveform.
const SFX_FADE_MS: usize = 5;
const SFX_FADE_SAMPLES: usize = (WIRE_SAMPLE_RATE as usize) / 1000 * SFX_FADE_MS;

/// A one-shot sound-effect voice: plays its samples once, then reports finished so the mixer removes it.
struct SfxVoice {
    samples: Arc<Vec<f32>>,
    cursor: usize,
}

impl MixSource for SfxVoice {
    fn mix_into(&mut self, out: &mut [f32]) {
        let total = self.samples.len();
        let fade = SFX_FADE_SAMPLES.min(total / 2).max(1);
        let remaining = total.saturating_sub(self.cursor);
        let n = remaining.min(out.len());
        for i in 0..n {
            let idx = self.cursor + i;
            let fade_in = ((idx + 1) as f32 / fade as f32).min(1.0);
            let fade_out = ((total - idx) as f32 / fade as f32).min(1.0);
            out[i] += self.samples[idx] * fade_in.min(fade_out);
        }
        self.cursor += n;
    }

    fn is_finished(&self) -> bool {
        self.cursor >= self.samples.len()
    }
}

pub(crate) type CaptureListener = Box<dyn FnMut(&[f32]) + Send + 'static>;

enum OutputCommand {
    SetDevice(Option<String>),
    Shutdown,
}

enum InputCommand {
    SetDevice(Option<String>),
    SetListener(Option<CaptureListener>),
    Shutdown,
}

struct OutputStreamState {
    // kept alive so the audio callback keeps firing. never read directly
    #[allow(dead_code)]
    stream: cpal::Stream,
    mixed_tx: std::sync::mpsc::Sender<Vec<f32>>,
    /// wire samples sent into mixed_tx but not yet taken
    queued: Arc<AtomicUsize>,
    /// largest callback request seen, in wire samples
    max_request: Arc<AtomicUsize>,
}

/// How many wire samples the queue to the sound card is short of its target,
/// nothing once it's full. Pacing by what the card consumed keeps the queue
/// bounded however far the two clocks drift, and stops production entirely if
/// the card stops pulling. The target covers the largest callback request plus
/// two mix chunks of slack for a late mixer tick.
fn samples_to_mix(queued: usize, max_request: usize) -> usize {
    let target = OUTPUT_QUEUE_MIN_TARGET.max(max_request + 2 * MIX_CHUNK_SAMPLES);
    target.saturating_sub(queued)
}

/// One mixer tick: tops the output queue up in MIX_CHUNK_SAMPLES chunks. A late
/// tick produces several chunks, a full queue none.
fn mix_tick(voices: &VoiceList, queued: &AtomicUsize, max_request: &AtomicUsize, mut send: impl FnMut(Vec<f32>)) {
    while samples_to_mix(queued.load(Ordering::Relaxed), max_request.load(Ordering::Relaxed)) >= MIX_CHUNK_SAMPLES {
        let mut acc = vec![0.0f32; MIX_CHUNK_SAMPLES];
        voices.lock().unwrap().retain_mut(|(_, voice)| {
            voice.mix_into(&mut acc);
            !voice.is_finished()
        });
        for sample in acc.iter_mut() {
            // voices are summed, not averaged, so several loud ones can
            // exceed unity: round that off gently while leaving normal levels untouched
            *sample = soft_limit(*sample);
        }
        // counted before sending, so the callback's subtraction doesn't underflow
        queued.fetch_add(acc.len(), Ordering::Relaxed);
        send(acc);
    }
}

/// The output callback's side of the mixer queue: converts mixed wire-rate
/// chunks to the device's rate/channels and hands out exactly what the card asks for.
struct OutputFeed {
    native_rate: u32,
    channels: u16,
    resampler: LinearResampler,
    /// device-format samples already converted but not yet played
    pending: Vec<f32>,
    queued: Arc<AtomicUsize>,
    max_request: Arc<AtomicUsize>,
    /// the first callback is often a one-off prefill of the device buffer
    /// (100 ms on PipeWire), not the steady request size
    seen_first_request: bool,
}

impl OutputFeed {
    fn new(native_rate: u32, channels: u16, queued: Arc<AtomicUsize>, max_request: Arc<AtomicUsize>) -> Self {
        Self {
            native_rate,
            channels,
            resampler: LinearResampler::new(WIRE_SAMPLE_RATE, native_rate),
            pending: Vec::new(),
            queued,
            max_request,
            seen_first_request: false,
        }
    }

    /// Fills `output` (interleaved device samples), with silence where the mixer fell behind.
    fn fill(&mut self, output: &mut [f32], mixed_rx: &std::sync::mpsc::Receiver<Vec<f32>>) {
        let frames = output.len() / self.channels.max(1) as usize;
        let wire_request = frames * WIRE_SAMPLE_RATE as usize / self.native_rate.max(1) as usize;
        if self.seen_first_request {
            self.max_request.fetch_max(wire_request, Ordering::Relaxed);
        }
        self.seen_first_request = true;
        while self.pending.len() < output.len() {
            match mixed_rx.try_recv() {
                Ok(mono_chunk) => {
                    self.queued.fetch_sub(mono_chunk.len(), Ordering::Relaxed);
                    let resampled = self.resampler.process(&mono_chunk);
                    self.pending.extend(upmix_from_mono(&resampled, self.channels));
                }
                Err(_) => break,
            }
        }
        let n = output.len().min(self.pending.len());
        output[..n].copy_from_slice(&self.pending[..n]);
        for sample in &mut output[n..] {
            *sample = 0.0;
        }
        self.pending.drain(..n);
    }
}

fn build_output_stream_for(device_name: &Option<String>) -> Result<OutputStreamState> {
    let host = cpal::default_host();
    let device = find_output_device(&host, device_name)?;
    let config = device.default_output_config().context("no default output config")?;
    let native_rate = config.sample_rate().0;
    let channels = config.channels();

    let (mixed_tx, mixed_rx) = std::sync::mpsc::channel::<Vec<f32>>();
    let queued = Arc::new(AtomicUsize::new(0));
    let max_request = Arc::new(AtomicUsize::new(0));
    let mut feed = OutputFeed::new(native_rate, channels, queued.clone(), max_request.clone());

    let stream = open_output_stream(
        &device,
        config,
        move |output: &mut [f32]| feed.fill(output, &mixed_rx),
        |err| eprintln!("audio playback error: {err}"),
    )?;
    stream.play().context("failed to start playback stream")?;

    Ok(OutputStreamState { stream, mixed_tx, queued, max_request })
}

/// Owns the output streams thread: builds a stream for the
/// current device, then alternates between waiting for a control command and
/// running a mix tick every MIX_CHUNK_MS. Only this thread touches the
/// `cpal::Stream`/`Device`, because they aren't `Send`.
fn output_thread_main(
    voices: VoiceList,
    initial_device: Option<String>,
    cmd_rx: std::sync::mpsc::Receiver<OutputCommand>,
    ready_tx: std::sync::mpsc::Sender<Result<()>>,
) {
    let mut device = initial_device;
    let mut state = match build_output_stream_for(&device) {
        Ok(s) => {
            let _ = ready_tx.send(Ok(()));
            Some(s)
        }
        Err(e) => {
            let _ = ready_tx.send(Err(anyhow!("{e:#}")));
            None
        }
    };

    loop {
        match cmd_rx.recv_timeout(std::time::Duration::from_millis(MIX_CHUNK_MS)) {
            Ok(OutputCommand::SetDevice(name)) => {
                device = name;
                state = match build_output_stream_for(&device) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        eprintln!("failed to switch output device: {e:#}");
                        None
                    }
                };
            }
            Ok(OutputCommand::Shutdown) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if let Some(s) = state.as_ref() {
                    mix_tick(&voices, &s.queued, &s.max_request, |chunk| {
                        let _ = s.mixed_tx.send(chunk);
                    });
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(state);
}

struct OutputEngine {
    cmd_tx: std::sync::mpsc::Sender<OutputCommand>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl OutputEngine {
    fn start(voices: VoiceList, initial_device: Option<String>) -> Result<Self> {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();

        let join = std::thread::spawn(move || {
            output_thread_main(voices, initial_device, cmd_rx, ready_tx);
        });

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self { cmd_tx, join: Some(join) }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow!("output audio thread exited before starting")),
        }
    }

    fn set_device(&self, name: Option<String>) -> Result<()> {
        self.cmd_tx
            .send(OutputCommand::SetDevice(name))
            .map_err(|_| anyhow!("output audio thread is not running"))
    }
}

impl Drop for OutputEngine {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(OutputCommand::Shutdown);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn build_input_stream_for(
    device_name: &Option<String>,
    listener: Arc<StdMutex<Option<CaptureListener>>>,
    muted: Arc<AtomicBool>,
) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = find_input_device(&host, device_name)?;
    let supported_config = device.default_input_config().context("no supported input config")?;
    let native_rate = supported_config.sample_rate().0;
    let channels = supported_config.channels();

    let mut leveler = RmsLeveler::new();
    let mut resampler = LinearResampler::new(native_rate, WIRE_SAMPLE_RATE);
    let stream = open_input_stream(
        &device,
        supported_config,
        move |data: &[f32]| {
            if muted.load(Ordering::Relaxed) {
                return;
            }
            let mono = downmix_to_mono(data, channels);
            let leveled = leveler.process(&mono, native_rate);
            let resampled = resampler.process(&leveled);
            if let Some(cb) = listener.lock().unwrap().as_mut() {
                cb(&resampled);
            }
        },
        |err| eprintln!("audio capture error: {err}"),
    )?;
    stream.play().context("failed to start capture stream")?;
    Ok(stream)
}

/// Owns the input stream's dedicated thread, mirroring `output_thread_main`.
/// Unlike output, no periodic tick here. the registered listener is
/// invoked directly from cpal capture callback with processed samples.
fn input_thread_main(
    muted: Arc<AtomicBool>,
    initial_device: Option<String>,
    cmd_rx: std::sync::mpsc::Receiver<InputCommand>,
    ready_tx: std::sync::mpsc::Sender<Result<()>>,
) {
    let listener: Arc<StdMutex<Option<CaptureListener>>> = Arc::new(StdMutex::new(None));
    let mut device = initial_device;
    let mut stream = match build_input_stream_for(&device, listener.clone(), muted.clone()) {
        Ok(s) => {
            let _ = ready_tx.send(Ok(()));
            Some(s)
        }
        Err(e) => {
            let _ = ready_tx.send(Err(anyhow!("{e:#}")));
            None
        }
    };

    loop {
        match cmd_rx.recv() {
            Ok(InputCommand::SetDevice(name)) => {
                device = name;
                stream = match build_input_stream_for(&device, listener.clone(), muted.clone()) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        eprintln!("failed to switch input device: {e:#}");
                        None
                    }
                };
            }
            Ok(InputCommand::SetListener(l)) => {
                *listener.lock().unwrap() = l;
            }
            Ok(InputCommand::Shutdown) | Err(_) => break,
        }
    }
    drop(stream);
}

struct InputEngine {
    cmd_tx: std::sync::mpsc::Sender<InputCommand>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl InputEngine {
    fn start(muted: Arc<AtomicBool>, initial_device: Option<String>) -> Result<Self> {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();

        let join = std::thread::spawn(move || {
            input_thread_main(muted, initial_device, cmd_rx, ready_tx);
        });

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self { cmd_tx, join: Some(join) }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow!("input audio thread exited before starting")),
        }
    }

    fn set_device(&self, name: Option<String>) -> Result<()> {
        self.cmd_tx
            .send(InputCommand::SetDevice(name))
            .map_err(|_| anyhow!("input audio thread is not running"))
    }

    fn set_listener(&self, listener: Option<CaptureListener>) -> Result<()> {
        self.cmd_tx
            .send(InputCommand::SetListener(listener))
            .map_err(|_| anyhow!("input audio thread is not running"))
    }
}

impl Drop for InputEngine {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(InputCommand::Shutdown);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// The shared audio engine: owns cpal device selection, the persistent
/// input/output streams, and mixes every voice into one output stream.
pub struct SoundHandler {
    input_device: StdMutex<Option<String>>,
    output_device: StdMutex<Option<String>>,
    muted: Arc<AtomicBool>,
    voices: VoiceList,
    next_voice_id: AtomicU64,
    output_engine: StdMutex<Option<OutputEngine>>,
    input_engine: StdMutex<Option<InputEngine>>,
    /// id of the capture currently owning the mic, see `start_capture`
    capture_owner: StdMutex<Option<u64>>,
    sound_cache: StdMutex<HashMap<String, Arc<DecodedSound>>>,
}

impl SoundHandler {
    pub fn new() -> Self {
        Self {
            input_device: StdMutex::new(None),
            output_device: StdMutex::new(None),
            muted: Arc::new(AtomicBool::new(false)),
            voices: Arc::new(StdMutex::new(Vec::new())),
            next_voice_id: AtomicU64::new(0),
            output_engine: StdMutex::new(None),
            input_engine: StdMutex::new(None),
            capture_owner: StdMutex::new(None),
            sound_cache: StdMutex::new(HashMap::new()),
        }
    }

    pub fn list_input_devices(&self) -> Result<Vec<AudioDeviceInfo>> {
        let host = cpal::default_host();
        let default = host.default_input_device().and_then(|d| d.name().ok());
        Ok(devices_with_default(host.input_devices()?, default))
    }

    pub fn list_output_devices(&self) -> Result<Vec<AudioDeviceInfo>> {
        let host = cpal::default_host();
        let default = host.default_output_device().and_then(|d| d.name().ok());
        Ok(devices_with_default(host.output_devices()?, default))
    }

    pub fn input_device(&self) -> Option<String> {
        self.input_device.lock().unwrap().clone()
    }

    pub fn output_device(&self) -> Option<String> {
        self.output_device.lock().unwrap().clone()
    }

    pub fn set_input_device(&self, name: Option<String>) -> Result<()> {
        *self.input_device.lock().unwrap() = name.clone();
        if let Some(engine) = self.input_engine.lock().unwrap().as_ref() {
            engine.set_device(name)?;
        }
        Ok(())
    }

    pub fn set_output_device(&self, name: Option<String>) -> Result<()> {
        *self.output_device.lock().unwrap() = name.clone();
        self.ensure_output_started()?;
        if let Some(engine) = self.output_engine.lock().unwrap().as_ref() {
            engine.set_device(name)?;
        }
        Ok(())
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    fn ensure_output_started(&self) -> Result<()> {
        let mut guard = self.output_engine.lock().unwrap();
        if guard.is_none() {
            let device = self.output_device.lock().unwrap().clone();
            *guard = Some(OutputEngine::start(self.voices.clone(), device)?);
        }
        Ok(())
    }

    /// Makes listener the single active mic listener, replacing any previous
    /// one, and returns an id identifying this capture. Starts the input stream
    /// lazily, so the mic is never open outside of active calls.
    pub(crate) fn start_capture(&self, listener: CaptureListener) -> Result<u64> {
        let mut guard = self.input_engine.lock().unwrap();
        if guard.is_none() {
            let device = self.input_device.lock().unwrap().clone();
            *guard = Some(InputEngine::start(self.muted.clone(), device)?);
        }
        guard.as_ref().unwrap().set_listener(Some(listener))?;
        let id = self.next_voice_id.fetch_add(1, Ordering::Relaxed);
        *self.capture_owner.lock().unwrap() = Some(id);
        Ok(id)
    }

    /// Closes the mic, but only if capture `id` still owns it. A stale call
    /// being dropped after a newer one started capturing must not cut the
    /// newer call's mic, same idea as `VoiceHandle` for the output mix.
    pub(crate) fn stop_capture(&self, id: u64) {
        let mut owner = self.capture_owner.lock().unwrap();
        if *owner == Some(id) {
            *owner = None;
            *self.input_engine.lock().unwrap() = None;
        }
    }

    /// Adds a persistent voice to the
    /// output mix. The returned handle removes it from the mix on drop.
    pub fn register_call_voice(&self, source: impl MixSource + 'static) -> Result<VoiceHandle> {
        self.ensure_output_started()?;
        let id = self.next_voice_id.fetch_add(1, Ordering::Relaxed);
        self.voices.lock().unwrap().push((id, Box::new(source)));
        Ok(VoiceHandle { id, voices: self.voices.clone() })
    }

    /// Plays a bundled sound effect by name, decoding it on
    /// first use via `load_bytes`. Mixes additively with any live call audio
    /// and removes itself once playback finishes.
    pub fn play_sound_effect(&self, name: &str, load_bytes: impl FnOnce() -> Result<Vec<u8>>) -> Result<()> {
        let decoded = {
            let mut cache = self.sound_cache.lock().unwrap();
            if let Some(d) = cache.get(name) {
                d.clone()
            } else {
                let bytes = load_bytes()?;
                let decoded = Arc::new(decode_mp3_to_wire_rate(&bytes)?);
                cache.insert(name.to_string(), decoded.clone());
                decoded
            }
        };
        self.ensure_output_started()?;
        let id = self.next_voice_id.fetch_add(1, Ordering::Relaxed);
        let voice = SfxVoice { samples: decoded.samples.clone(), cursor: 0 };
        self.voices.lock().unwrap().push((id, Box::new(voice)));
        Ok(())
    }
}

impl Default for SoundHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_to_mix_tops_up_to_target() {
        assert_eq!(samples_to_mix(0, 0), OUTPUT_QUEUE_MIN_TARGET);
        assert_eq!(samples_to_mix(100, 0), OUTPUT_QUEUE_MIN_TARGET - 100);
    }

    #[test]
    fn samples_to_mix_produces_nothing_when_card_stops_pulling() {
        assert_eq!(samples_to_mix(OUTPUT_QUEUE_MIN_TARGET, 0), 0);
        assert_eq!(samples_to_mix(10 * OUTPUT_QUEUE_MIN_TARGET, 0), 0);
    }

    #[test]
    fn samples_to_mix_target_grows_with_large_device_buffers() {
        let large_request = OUTPUT_QUEUE_MIN_TARGET;
        assert_eq!(samples_to_mix(0, large_request), large_request + 2 * MIX_CHUNK_SAMPLES);
    }

    const RATE: u32 = 48_000;

    fn sine(amplitude: f32, len: usize) -> Vec<f32> {
        (0..len).map(|i| amplitude * (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / RATE as f32).sin()).collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    /// Feeds `signal` through the leveler in 10 ms blocks.
    fn level(leveler: &mut RmsLeveler, signal: &[f32]) -> Vec<f32> {
        signal.chunks(480).flat_map(|block| leveler.process(block, RATE)).collect()
    }

    #[test]
    fn leveler_keeps_background_noise_silent() {
        let mut leveler = RmsLeveler::new();
        let out = level(&mut leveler, &sine(0.002, RATE as usize));
        assert!(out.iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn leveler_brings_quiet_speech_towards_target() {
        let mut leveler = RmsLeveler::new();
        let out = level(&mut leveler, &sine(0.05, 2 * RATE as usize));
        let settled = rms(&out[out.len() - 4800..]);
        assert!((settled - LEVELER_TARGET_RMS).abs() < 0.02, "settled at {settled}");
    }

    #[test]
    fn leveler_gate_holds_through_short_pause() {
        let mut leveler = RmsLeveler::new();
        level(&mut leveler, &sine(0.1, RATE as usize));
        // a 100 ms dip below the gate threshold, e.g. between two words
        let pause = level(&mut leveler, &sine(0.003, 4800));
        assert!(rms(&pause[pause.len() - 480..]) > 0.002, "gate closed inside the hold time");
        // long after the hold ran out the gate has faded closed
        let later = level(&mut leveler, &sine(0.003, RATE as usize / 2));
        assert!(rms(&later[later.len() - 480..]) < 1e-4);
    }

    #[test]
    fn leveler_gain_does_not_grow_during_silence() {
        let mut leveler = RmsLeveler::new();
        level(&mut leveler, &sine(0.15, RATE as usize));
        let gain_while_speaking = leveler.agc_gain;
        level(&mut leveler, &vec![0.0; 2 * RATE as usize]);
        assert_eq!(leveler.agc_gain, gain_while_speaking);
    }

    #[test]
    fn leveler_never_exceeds_full_scale() {
        let mut leveler = RmsLeveler::new();
        level(&mut leveler, &sine(0.01, RATE as usize));
        // sudden shout after the AGC turned the gain up
        let out = level(&mut leveler, &sine(1.0, 4800));
        assert!(out.iter().all(|s| s.abs() <= 1.0));
    }

    #[test]
    fn soft_limit_is_linear_below_knee_and_bounded_above() {
        assert_eq!(soft_limit(0.5), 0.5);
        assert_eq!(soft_limit(-LIMITER_KNEE), -LIMITER_KNEE);
        assert!(soft_limit(5.0) <= 1.0 && soft_limit(5.0) > 0.99);
        assert!(soft_limit(-5.0) >= -1.0);
        assert!(soft_limit(0.9) > soft_limit(0.85));
    }

    #[test]
    fn resampler_is_continuous_across_chunk_boundaries() {
        // a ramp resampled 3 -> 2 must stay exactly a ramp with step 1.5,
        // whatever chunk sizes it arrives in
        let ramp: Vec<f32> = (0..300).map(|i| i as f32).collect();
        let mut resampler = LinearResampler::new(3, 2);
        let mut out = Vec::new();
        for chunk in ramp.chunks(7) {
            out.extend(resampler.process(chunk));
        }
        for (i, &sample) in out.iter().enumerate() {
            assert!((sample - i as f32 * 1.5).abs() < 1e-3, "sample {i} = {sample}");
        }
        assert_eq!(out.len(), 200);
    }

    #[test]
    fn resampler_keeps_the_rate_over_many_small_chunks() {
        let mut resampler = LinearResampler::new(44_100, 48_000);
        let total: usize = (0..100).map(|_| resampler.process(&[0.0; 441]).len()).sum();
        assert!(total.abs_diff(48_000) <= 1, "produced {total}");
    }

    fn no_voices() -> VoiceList {
        Arc::new(StdMutex::new(Vec::new()))
    }

    #[test]
    fn mix_tick_tops_up_in_small_fixed_chunks() {
        // a 44.1 kHz card's first callback asks for 100 ms at once; that must
        // not turn into one huge chunk that drains every peer buffer in one go
        let queued = AtomicUsize::new(0);
        let max_request = AtomicUsize::new(4800);
        let mut chunks = Vec::new();
        mix_tick(&no_voices(), &queued, &max_request, |c| chunks.push(c));
        assert!(chunks.iter().all(|c| c.len() == MIX_CHUNK_SAMPLES));
        assert_eq!(chunks.len(), (4800 + 2 * MIX_CHUNK_SAMPLES) / MIX_CHUNK_SAMPLES);
        assert_eq!(queued.load(Ordering::Relaxed), chunks.len() * MIX_CHUNK_SAMPLES);
    }

    #[test]
    fn mix_tick_produces_nothing_while_queue_is_full() {
        let queued = AtomicUsize::new(samples_to_mix(0, 0));
        let max_request = AtomicUsize::new(0);
        let mut chunks = 0;
        mix_tick(&no_voices(), &queued, &max_request, |_| chunks += 1);
        assert_eq!(chunks, 0);
    }

    #[test]
    fn output_feed_upmixes_and_accounts_taken_chunks() {
        let queued = Arc::new(AtomicUsize::new(0));
        let max_request = Arc::new(AtomicUsize::new(0));
        let mut feed = OutputFeed::new(WIRE_SAMPLE_RATE, 2, queued.clone(), max_request.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        queued.fetch_add(2 * MIX_CHUNK_SAMPLES, Ordering::Relaxed);
        tx.send(vec![0.5; MIX_CHUNK_SAMPLES]).unwrap();
        tx.send(vec![0.5; MIX_CHUNK_SAMPLES]).unwrap();

        let mut out = vec![0.0; 2 * 600];
        feed.fill(&mut out, &rx);
        assert!(out.iter().all(|&s| s == 0.5));
        assert_eq!(queued.load(Ordering::Relaxed), 0);
        // the first request may be a prefill, it doesn't set the queue target
        assert_eq!(max_request.load(Ordering::Relaxed), 0);
        // the leftover of the second chunk plays next, then silence
        let mut out = vec![1.0; 2 * 600];
        feed.fill(&mut out, &rx);
        assert!(out[..2 * 360].iter().all(|&s| s == 0.5));
        assert!(out[2 * 360..].iter().all(|&s| s == 0.0));
        assert_eq!(max_request.load(Ordering::Relaxed), 600);
    }

    /// End-to-end playout simulation, the guard for audible call quality. Runs
    /// the real mixer tick, output feed and peer jitter buffer on a simulated
    /// clock: a sender's mic delivers bursts, packets arrive with network
    /// jitter, the mixer thread wakes late, the sound card pulls in its own
    /// period (with a big first prefill request like real devices do). The
    /// signal is a constant, so every played frame below it after warm-up is
    /// an audible gap or fade.
    mod playout {
        use super::*;
        use crate::channel::{PeerBuffer, PEER_BUFFER_MAX, PEER_BUFFER_TARGET};
        use std::collections::VecDeque;

        const LEVEL: f32 = 0.5;
        const WARMUP_US: u64 = 1_000_000;
        const STEP_US: u64 = 250;
        const NETWORK_BASE_US: u64 = 5_000;

        struct SharedPeer(Arc<StdMutex<PeerBuffer>>);

        impl MixSource for SharedPeer {
            fn mix_into(&mut self, out: &mut [f32]) {
                self.0.lock().unwrap().mix_into(out);
            }
        }

        /// deterministic xorshift, so failures are reproducible
        struct Rng(u64);

        impl Rng {
            fn below(&mut self, max: u64) -> u64 {
                if max == 0 {
                    return 0;
                }
                self.0 ^= self.0 << 13;
                self.0 ^= self.0 >> 7;
                self.0 ^= self.0 << 17;
                self.0 % max
            }
        }

        struct Scenario {
            card_rate: u32,
            card_channels: u16,
            first_request_frames: usize,
            request_frames: usize,
            mic_rate: u32,
            mic_chunk_frames: usize,
            network_jitter_us: u64,
            tick_lateness_us: u64,
        }

        #[derive(Debug)]
        struct Report {
            gap_frames: usize,
            trimmed_samples: usize,
            max_latency_ms: f64,
            played_frames: usize,
        }

        fn period_us(frames: usize, rate: u32) -> u64 {
            frames as u64 * 1_000_000 / rate as u64
        }

        fn run(sc: &Scenario, seconds: u64) -> Report {
            let peer = Arc::new(StdMutex::new(PeerBuffer::default()));
            let voices: VoiceList = Arc::new(StdMutex::new(vec![(0, Box::new(SharedPeer(peer.clone())) as Box<dyn MixSource>)]));
            let queued = Arc::new(AtomicUsize::new(0));
            let max_request = Arc::new(AtomicUsize::new(0));
            let mut feed = OutputFeed::new(sc.card_rate, sc.card_channels, queued.clone(), max_request.clone());
            let (tx, rx) = std::sync::mpsc::channel();

            let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
            let mut mic_resampler = LinearResampler::new(sc.mic_rate, WIRE_SAMPLE_RATE);
            let mut unsent: Vec<f32> = Vec::new();
            let mut in_flight: VecDeque<(u64, Vec<f32>)> = VecDeque::new();
            let mut last_arrival = 0;

            let card_period = period_us(sc.request_frames, sc.card_rate);
            let mic_period = period_us(sc.mic_chunk_frames, sc.mic_rate);
            let (mut next_callback, mut next_tick, mut next_mic) = (0, 0, mic_period);
            let mut first_callback = true;
            let mut report = Report { gap_frames: 0, trimmed_samples: 0, max_latency_ms: 0.0, played_frames: 0 };

            let end = seconds * 1_000_000;
            let mut now = 0;
            while now < end {
                if now >= next_mic {
                    next_mic += mic_period;
                    unsent.extend(mic_resampler.process(&vec![LEVEL; sc.mic_chunk_frames]));
                    // the encoder sends whole 20 ms frames
                    while unsent.len() >= 960 {
                        let frame: Vec<f32> = unsent.drain(..960).collect();
                        let arrival = (now + NETWORK_BASE_US + rng.below(sc.network_jitter_us)).max(last_arrival);
                        last_arrival = arrival;
                        in_flight.push_back((arrival, frame));
                    }
                }
                while in_flight.front().is_some_and(|(at, _)| *at <= now) {
                    let (_, frame) = in_flight.pop_front().unwrap();
                    let mut buf = peer.lock().unwrap();
                    if buf.len() + frame.len() > PEER_BUFFER_MAX {
                        report.trimmed_samples += buf.len() + frame.len() - PEER_BUFFER_TARGET;
                    }
                    buf.push(frame);
                }
                if now >= next_tick {
                    next_tick += MIX_CHUNK_MS * 1000 + rng.below(sc.tick_lateness_us);
                    mix_tick(&voices, &queued, &max_request, |chunk| tx.send(chunk).unwrap());
                }
                if now >= next_callback {
                    let frames = if first_callback { sc.first_request_frames } else { sc.request_frames };
                    first_callback = false;
                    next_callback += card_period;
                    let mut out = vec![0.0; frames * sc.card_channels as usize];
                    feed.fill(&mut out, &rx);
                    if now >= WARMUP_US {
                        let ch = sc.card_channels as usize;
                        report.played_frames += frames;
                        report.gap_frames += out.chunks(ch).filter(|f| f[0] < LEVEL - 0.01).count();
                        let buffered_wire = peer.lock().unwrap().len()
                            + queued.load(Ordering::Relaxed)
                            + feed.pending.len() / ch * WIRE_SAMPLE_RATE as usize / sc.card_rate as usize;
                        let latency_ms = buffered_wire as f64 / 48.0;
                        report.max_latency_ms = report.max_latency_ms.max(latency_ms);
                    }
                }
                now += STEP_US;
            }
            report
        }

        fn assert_clean(name: &str, sc: Scenario) {
            let report = run(&sc, 10);
            eprintln!("{name}: {report:?}");
            assert!(report.played_frames > 0);
            assert_eq!(report.gap_frames, 0, "{name}: audible gaps/fades in steady state: {report:?}");
            assert_eq!(report.trimmed_samples, 0, "{name}: jitter buffer overflowed and dropped audio: {report:?}");
            assert!(report.max_latency_ms < 200.0, "{name}: playout latency too high: {report:?}");
        }

        #[test]
        fn pipewire_44k_card_with_large_prefill() {
            // measured on a PipeWire/ALSA laptop: first callback 4410 frames,
            // then 1411 every 32 ms; mic delivers 1882 frames every 42 ms
            assert_clean("pipewire 44.1k", Scenario {
                card_rate: 44_100, card_channels: 2, first_request_frames: 4410, request_frames: 1411,
                mic_rate: 44_100, mic_chunk_frames: 1882, network_jitter_us: 15_000, tick_lateness_us: 3_000,
            });
        }

        #[test]
        fn low_latency_48k_card() {
            assert_clean("low latency 48k", Scenario {
                card_rate: 48_000, card_channels: 2, first_request_frames: 480, request_frames: 480,
                mic_rate: 48_000, mic_chunk_frames: 480, network_jitter_us: 10_000, tick_lateness_us: 2_000,
            });
        }

        #[test]
        fn large_buffer_card_and_bursty_mic() {
            assert_clean("large buffers", Scenario {
                card_rate: 48_000, card_channels: 2, first_request_frames: 4096, request_frames: 2048,
                mic_rate: 48_000, mic_chunk_frames: 2048, network_jitter_us: 20_000, tick_lateness_us: 4_000,
            });
        }

        #[test]
        fn mismatched_card_and_mic_rates() {
            assert_clean("44.1k card / 48k mic", Scenario {
                card_rate: 44_100, card_channels: 1, first_request_frames: 1024, request_frames: 512,
                mic_rate: 48_000, mic_chunk_frames: 960, network_jitter_us: 15_000, tick_lateness_us: 3_000,
            });
        }
    }
}
