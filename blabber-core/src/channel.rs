use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::StreamConfig;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use tokio::sync::broadcast;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex as StdMutex};
use uuid::Uuid;

pub const VOICE_ALPN: &[u8] = b"blabber/voice/0";
pub const CALL_ROOM_ALPN: &[u8] = b"blabber/callroom/0";

const FALLBACK_MAX_DATAGRAM_SIZE: usize = 1100;
const SAFE_DATAGRAM_CEILING: usize = 1200;
const SAFETY_MARGIN: usize = 32;
const WIRE_SAMPLE_RATE: u32 = 48000;
const MIX_CHUNK_MS: u64 = 10;
const MIX_CHUNK_SAMPLES: usize = (WIRE_SAMPLE_RATE as usize) / 1000 * (MIX_CHUNK_MS as usize);

fn samples_per_packet(connection: &Connection) -> usize {
    let max_datagram = connection
        .max_datagram_size()
        .unwrap_or(FALLBACK_MAX_DATAGRAM_SIZE)
        .min(SAFE_DATAGRAM_CEILING);
    let safe_bytes = max_datagram.saturating_sub(SAFETY_MARGIN).max(4);
    (safe_bytes / 4).max(1)
}

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

fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }

    let ratio = to_rate as f64 / from_rate as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut output = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_pos = i as f64 / ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;

        let s0 = input[idx.min(input.len() - 1)];
        let s1 = input[(idx + 1).min(input.len() - 1)];
        output.push(s0 + (s1 - s0) * frac);
    }

    output
}

pub struct VoiceChannel {
    connection: Connection,
}

impl VoiceChannel {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn start_capture(&self) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| anyhow!("no input device available"))?;
        let supported_config = device.default_input_config().context("no supported input config")?;
        let native_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels();
        let stream_config: StreamConfig = supported_config.into();

        let connection = self.connection.clone();
        let max_samples = samples_per_packet(&connection);
        let stream = device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mono = downmix_to_mono(data, channels);
                let resampled = resample_linear(&mono, native_rate, WIRE_SAMPLE_RATE);
                send_audio_chunk(&connection, &resampled, max_samples);
            },
            move |err| { eprintln!("audio capture error: {err}");},
            None,)?;
        stream.play().context("failed to start capture stream")?;
        Ok(stream)
    }

    pub fn start_playback(&self, handle: &tokio::runtime::Handle) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| anyhow!("no output device available"))?;
        let config = device.default_output_config().context("no default output config")?;
        let native_rate = config.sample_rate().0;
        let channels = config.channels();
        let stream_config: StreamConfig = config.into();

        let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = channel();
        let connection = self.connection.clone();
        handle.spawn(async move {
            loop {
                match connection.read_datagram().await {
                    Ok(bytes) => {
                        if let Some(samples) = bytes_to_samples(&bytes) {
                            let resampled = resample_linear(&samples, WIRE_SAMPLE_RATE, native_rate);
                            let upmixed = upmix_from_mono(&resampled, channels);
                            let _ = tx.send(upmixed);
                        }
                    }
                    Err(e) => {
                        eprintln!("iroh datagram recv error: {e}");
                        break;
                    }}}});

        let mut pending: Vec<f32> = Vec::new();
        let err_fn = |err| eprintln!("audio playback error: {err}");

        let stream = device.build_output_stream(
            &stream_config,
            move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                while pending.len() < output.len() {
                    match rx.try_recv() {
                        Ok(samples) => pending.extend(samples),
                        Err(_) => break,}}
                let n = output.len().min(pending.len());
                output[..n].copy_from_slice(&pending[..n]);
                for s in &mut output[n..] {
                    *s = 0.0;}
                pending.drain(..n);
            },
            err_fn,
            None,
        )?;
        stream.play().context("failed to start playback stream")?;
        Ok(stream)
    }
}

pub struct ActiveVoiceCall {
    stop_tx: std::sync::mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<Result<()>>>,
}

impl ActiveVoiceCall {
    pub fn start(channel: VoiceChannel, handle: tokio::runtime::Handle) -> Self {
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

        let thread = std::thread::spawn(move || -> Result<()> {
            let _capture = channel.start_capture().inspect_err(|e| {
                eprintln!("[voice] failed to start capture: {e:#}");
            })?;
            let _playback = channel.start_playback(&handle).inspect_err(|e| {
                eprintln!("[voice] failed to start playback: {e:#}");
            })?;
            let _ = stop_rx.recv();
            Ok(())
        });

        Self {
            stop_tx,
            thread: Some(thread),
        }
    }

    pub fn hang_up(mut self) {
        let _ = self.stop_tx.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for ActiveVoiceCall {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub type IncomingCallHandler = Arc<dyn Fn(String, tokio::sync::oneshot::Sender<bool>) + Send + Sync>;

#[derive(Clone)]
pub struct CallHandle {
    notify: Arc<tokio::sync::Notify>,
}

impl CallHandle {
    pub fn hang_up(&self) {
        self.notify.notify_one();
    }
}

pub type CallStartedHandler = Arc<dyn Fn(CallHandle) + Send + Sync>;

#[derive(Clone, Default)]
pub struct VoiceProtocol {
    on_incoming: Option<IncomingCallHandler>,
    on_call_started: Option<CallStartedHandler>,
}

impl std::fmt::Debug for VoiceProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceProtocol")
            .field("on_incoming", &self.on_incoming.is_some())
            .finish()
    }
}

impl VoiceProtocol {
    pub fn new() -> Self {
        Self {
            on_incoming: None,
            on_call_started: None,
        }
    }

    pub fn with_incoming_handler(mut self, handler: IncomingCallHandler) -> Self {
        self.on_incoming = Some(handler);
        self
    }

    pub fn with_call_started_handler(mut self, handler: CallStartedHandler) -> Self {
        self.on_call_started = Some(handler);
        self
    }
}

impl ProtocolHandler for VoiceProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_id = connection.remote_id();
        let accepted = if let Some(handler) = &self.on_incoming {
            let (decision_tx, decision_rx) = tokio::sync::oneshot::channel::<bool>();
            handler(remote_id.to_string(), decision_tx);
            match tokio::time::timeout(std::time::Duration::from_secs(30), decision_rx).await {
                Ok(Ok(decision)) => decision,
                Ok(Err(_)) => false,
                Err(_) => false,
            }
        } else {
            true
        };

        if !accepted {
            return Ok(());
        }

        let channel = VoiceChannel::new(connection.clone());
        let handle = tokio::runtime::Handle::current();
        let call = ActiveVoiceCall::start(channel, handle);

        let notify = Arc::new(tokio::sync::Notify::new());
        if let Some(cb) = &self.on_call_started {
            cb(CallHandle {
                notify: notify.clone(),
            });
        }

        tokio::select! {
            _ = connection.closed() => {}
            _ = notify.notified() => {
                connection.close(0u32.into(), b"hung up");
            }
        }
        call.hang_up();

        Ok(())
    }
}

fn send_audio_chunk(connection: &Connection, data: &[f32], max_samples: usize) {
    for sub_chunk in data.chunks(max_samples) {
        let raw: Vec<u8> = sub_chunk.iter().flat_map(|s| s.to_le_bytes()).collect();

        if let Err(e) = connection.send_datagram(raw.into()) {
            eprintln!("failed to send audio datagram: {e}");
        }
    }
}

fn bytes_to_samples(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect(),
    )
}
#[derive(Clone)]
pub struct MeshVoiceChannel {
    connections: Arc<StdMutex<HashMap<String, Connection>>>,
    peer_buffers: Arc<StdMutex<HashMap<String, VecDeque<f32>>>>,
    handle: tokio::runtime::Handle,
    muted: Arc<AtomicBool>,
}

impl MeshVoiceChannel {
    pub fn new(handle: tokio::runtime::Handle) -> Self {
        Self {
            connections: Arc::new(StdMutex::new(HashMap::new())),
            peer_buffers: Arc::new(StdMutex::new(HashMap::new())),
            handle,
            muted: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    pub fn add_peer(&self, peer_id: String, connection: Connection) {
        self.peer_buffers
            .lock()
            .unwrap()
            .insert(peer_id.clone(), VecDeque::new());
        self.connections
            .lock()
            .unwrap()
            .insert(peer_id.clone(), connection.clone());

        let peer_buffers = self.peer_buffers.clone();
        let peer_id_for_task = peer_id.clone();
        let mesh_channel = self.clone();
        self.handle.spawn(async move {
            loop {
                match connection.read_datagram().await {
                    Ok(bytes) => {
                        if let Some(samples) = bytes_to_samples(&bytes) {
                            let mut buffers = peer_buffers.lock().unwrap();
                            if let Some(buf) = buffers.get_mut(&peer_id_for_task) {
                                buf.extend(samples);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("iroh datagram recv error from {peer_id_for_task}: {e}");
                        break;
                    }
                }
            }
            mesh_channel.remove_peer(&peer_id_for_task);
        });
    }

    pub fn remove_peer(&self, peer_id: &str) {
        self.connections.lock().unwrap().remove(peer_id);
        self.peer_buffers.lock().unwrap().remove(peer_id);
    }

    pub fn peer_ids(&self) -> Vec<String> {
        self.connections.lock().unwrap().keys().cloned().collect()
    }

    pub fn connection_for(&self, peer_id: &str) -> Option<Connection> {
        self.connections.lock().unwrap().get(peer_id).cloned()
    }

    pub fn buffered_sample_count(&self, peer_id: &str) -> Option<usize> {
        self.peer_buffers.lock().unwrap().get(peer_id).map(|b| b.len())
    }

    pub fn connection_count(&self)->usize{
        self.connections.lock().unwrap().len()
    }

    pub fn start_capture(&self) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| anyhow!("no input device available"))?;
        let supported_config = device.default_input_config().context("no supported input config")?;
        let native_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels();
        let stream_config: StreamConfig = supported_config.into();

        let connections = self.connections.clone();
        let muted = self.muted.clone();
        let stream = device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if muted.load(Ordering::Relaxed) {
                    return;
                }

                let mono = downmix_to_mono(data, channels);
                let resampled = resample_linear(&mono, native_rate, WIRE_SAMPLE_RATE);

                let conns = connections.lock().unwrap();
                for connection in conns.values() {
                    let max_samples = samples_per_packet(connection);
                    send_audio_chunk(connection, &resampled, max_samples);
                }
            },
            move |err| { eprintln!("audio capture error: {err}"); },
            None,
        )?;
        stream.play().context("failed to start capture stream")?;
        Ok(stream)
    }
    pub fn start_playback(&self, handle: &tokio::runtime::Handle) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| anyhow!("no output device available"))?;
        let config = device.default_output_config().context("no default output config")?;
        let native_rate = config.sample_rate().0;
        let channels = config.channels();
        let stream_config: StreamConfig = config.into();

        let (mixed_tx, mixed_rx) = std::sync::mpsc::channel::<Vec<f32>>();
        let peer_buffers = self.peer_buffers.clone();

        handle.spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(MIX_CHUNK_MS));
            loop {
                interval.tick().await;
                let mut mixed = vec![0.0f32; MIX_CHUNK_SAMPLES];
                let mut active = 0u32;
                {
                    let mut buffers = peer_buffers.lock().unwrap();
                    for buf in buffers.values_mut() {
                        if buf.is_empty() {
                            continue;
                        }
                        active += 1;
                        for slot in mixed.iter_mut() {
                            if let Some(sample) = buf.pop_front() {
                                *slot += sample;
                            }
                        }
                    }
                }
                if active > 0 {
                    for s in mixed.iter_mut() {
                        *s /= active as f32;
                    }
                    let _ = mixed_tx.send(mixed);
                }
            }
        });

        let mut pending: Vec<f32> = Vec::new();
        let err_fn = |err| eprintln!("audio playback error: {err}");

        let stream = device.build_output_stream(
            &stream_config,
            move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                while pending.len() < output.len() {
                    match mixed_rx.try_recv() {
                        Ok(mono_chunk) => {
                            let resampled = resample_linear(&mono_chunk, WIRE_SAMPLE_RATE, native_rate);
                            let upmixed = upmix_from_mono(&resampled, channels);
                            pending.extend(upmixed);
                        }
                        Err(_) => break,
                    }
                }
                let n = output.len().min(pending.len());
                output[..n].copy_from_slice(&pending[..n]);
                for s in &mut output[n..] {
                    *s = 0.0;
                }
                pending.drain(..n);
            },
            err_fn,
            None,
        )?;
        stream.play().context("failed to start playback stream")?;
        Ok(stream)
    }
}

pub struct MeshActiveCall {
    stop_tx: std::sync::mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<Result<()>>>,
    channel: MeshVoiceChannel,
}

impl MeshActiveCall {
    pub fn start(channel: MeshVoiceChannel, handle: tokio::runtime::Handle) -> Self {
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let channel_handle = channel.clone();

        let thread = std::thread::spawn(move || -> Result<()> {
            let _capture = channel.start_capture().inspect_err(|e| {
                eprintln!("[mesh voice] failed to start capture: {e:#}");
            })?;
            let _playback = channel.start_playback(&handle).inspect_err(|e| {
                eprintln!("[mesh voice] failed to start playback: {e:#}");
            })?;
            let _ = stop_rx.recv();
            Ok(())
        });

        Self {
            stop_tx,
            thread: Some(thread),
            channel: channel_handle,
        }
    }
    pub fn hang_up(mut self) {
        let _ = self.stop_tx.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    pub fn set_muted(&self, muted: bool) {
        self.channel.set_muted(muted);
    }
}
impl Drop for MeshActiveCall {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
pub type RoomSpaceMap = Arc<StdMutex<HashMap<Uuid, Uuid>>>;
pub type ActiveCallRooms = Arc<StdMutex<HashMap<Uuid, MeshVoiceChannel>>>;
#[derive(Clone)]
pub struct CallRoomProtocol {
    active_rooms: ActiveCallRooms,
    room_spaces: RoomSpaceMap,
    events: broadcast::Sender<crate::events::AppEvent>,
}
impl CallRoomProtocol {
    pub fn new(
        active_rooms: ActiveCallRooms,
        room_spaces: RoomSpaceMap,
        events: broadcast::Sender<crate::events::AppEvent>,
    ) -> Self {
        Self { active_rooms, room_spaces, events }
    }
}
impl std::fmt::Debug for CallRoomProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallRoomProtocol").finish()
    }
}
impl ProtocolHandler for CallRoomProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_id = connection.remote_id().to_string();

        let (mut send, mut recv) = connection.accept_bi().await.map_err(std::io::Error::other)?;

        let mut room_id_bytes = [0u8; 16];
        recv.read_exact(&mut room_id_bytes).await.map_err(std::io::Error::other)?;
        let room_id = Uuid::from_bytes(room_id_bytes);

        let channel_opt = {
            let rooms = self.active_rooms.lock().unwrap();
            rooms.get(&room_id).cloned()
        };

        match channel_opt {
            Some(mesh_channel) => {
                send.write_all(&[1u8]).await.map_err(std::io::Error::other)?;
                send.finish().map_err(std::io::Error::other)?;
                mesh_channel.add_peer(remote_id.clone(), connection);

                let space_id = {
                    let map = self.room_spaces.lock().unwrap();
                    map.get(&room_id).copied()
                };

                if let Some(space_id) = space_id {
                    let _ = self.events.send(crate::events::AppEvent::NewCallParticipant {
                        space_id,
                        room_id,
                        endpoint_id: remote_id,
                    });
                }
            }
            None => {
                send.write_all(&[0u8]).await.map_err(std::io::Error::other)?;
                send.finish().map_err(std::io::Error::other)?;
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {}
