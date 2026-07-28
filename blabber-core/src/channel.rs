use anyhow::Result;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use tokio::sync::broadcast;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex as StdMutex};
use uuid::Uuid;

use crate::sound::{MixSource, SoundHandler, VoiceHandle};

/// ALPN mesh call rooms use to dial each other for the group-call audio mesh.
pub const CALL_ROOM_ALPN: &[u8] = b"blabber/callroom/0";

const FALLBACK_MAX_DATAGRAM_SIZE: usize = 1100;
const SAFE_DATAGRAM_CEILING: usize = 1200;
const SAFETY_MARGIN: usize = 32;

fn samples_per_packet(connection: &Connection) -> usize {
    let max_datagram = connection
        .max_datagram_size()
        .unwrap_or(FALLBACK_MAX_DATAGRAM_SIZE)
        .min(SAFE_DATAGRAM_CEILING);
    let safe_bytes = max_datagram.saturating_sub(SAFETY_MARGIN).max(4);
    (safe_bytes / 4).max(1)
}

/// Splits an outgoing audio chunk into datagram-sized pieces and sends each over the connection.
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
}

impl MeshVoiceChannel {
    pub fn new(handle: tokio::runtime::Handle) -> Self {
        Self {
            connections: Arc::new(StdMutex::new(HashMap::new())),
            peer_buffers: Arc::new(StdMutex::new(HashMap::new())),
            handle,
        }
    }

    /// Sends processed mic audio to every connected peer.
    pub(crate) fn broadcast_samples(&self, samples: &[f32]) {
        let conns = self.connections.lock().unwrap();
        for connection in conns.values() {
            let max_samples = samples_per_packet(connection);
            send_audio_chunk(connection, samples, max_samples);
        }
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
}

/// Wraps a call's peer buffers as one mixer voice. averages every currently
/// talking peer together,
/// then that average is added into the shared output mix as a single source.
struct CallVoiceSource(MeshVoiceChannel);

impl MixSource for CallVoiceSource {
    fn mix_into(&mut self, out: &mut [f32]) {
        let mut scratch = vec![0.0f32; out.len()];
        let mut active = 0u32;
        {
            let mut buffers = self.0.peer_buffers.lock().unwrap();
            for buf in buffers.values_mut() {
                if buf.is_empty() {
                    continue;
                }
                active += 1;
                for slot in scratch.iter_mut() {
                    if let Some(sample) = buf.pop_front() {
                        *slot += sample;
                    }
                }
            }
        }
        if active > 0 {
            let inv = 1.0 / active as f32;
            for (o, s) in out.iter_mut().zip(scratch.iter()) {
                *o += s * inv;
            }
        }
    }
}

/// An active mesh call: registers this call mic capture and its mixed
/// peer audio with the shared `SoundHandler`, and unregisters both on drop.
pub struct MeshActiveCall {
    sound: Arc<SoundHandler>,
    voice_handle: Option<VoiceHandle>,
}

impl MeshActiveCall {
    pub fn start(channel: MeshVoiceChannel, sound: Arc<SoundHandler>) -> Self {
        let capture_channel = channel.clone();
        if let Err(e) = sound.set_capture_listener(Some(Box::new(move |samples: &[f32]| {
            capture_channel.broadcast_samples(samples);
        }))) {
            eprintln!("[mesh voice] failed to start capture: {e:#}");
        }

        let voice_handle = match sound.register_call_voice(CallVoiceSource(channel)) {
            Ok(handle) => Some(handle),
            Err(e) => {
                eprintln!("[mesh voice] failed to start playback: {e:#}");
                None
            }
        };

        Self { sound, voice_handle }
    }

    pub fn hang_up(self) {}
}

impl Drop for MeshActiveCall {
    fn drop(&mut self) {
        let _ = self.sound.set_capture_listener(None);
        self.voice_handle = None;
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
