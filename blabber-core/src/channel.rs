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

/// computes safe amount of audio samples to send in one datagram.
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

/// converts received audio datagrams into f32 samples to play,
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

/// checks what space a given call room belongs to.
fn space_id_for_room(room_spaces: &RoomSpaceMap, room_id: Uuid) -> Option<Uuid> {
    room_spaces.lock().unwrap().get(&room_id).copied()
}

///
#[derive(Clone)]
pub struct MeshVoiceChannel {
    connections: Arc<StdMutex<HashMap<String, Connection>>>,
    peer_buffers: Arc<StdMutex<HashMap<String, VecDeque<f32>>>>,
    handle: tokio::runtime::Handle,
    room_id: Uuid,
    room_spaces: RoomSpaceMap,
    events: broadcast::Sender<crate::events::AppEvent>,
}

impl MeshVoiceChannel {
    /// creates shared state the call needs. (used once per new call join)
    pub fn new(
        handle: tokio::runtime::Handle,
        room_id: Uuid,
        room_spaces: RoomSpaceMap,
        events: broadcast::Sender<crate::events::AppEvent>,
    ) -> Self {
        Self {
            connections: Arc::new(StdMutex::new(HashMap::new())),
            peer_buffers: Arc::new(StdMutex::new(HashMap::new())),
            handle,
            room_id,
            room_spaces,
            events,
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

    /// add a new peer to mesh and spawns background task that consumes the incoming audio from this peer.
    pub fn add_peer(&self, peer_id: String, connection: Connection) {
        // create fresh jitter buffer for peer in map peer_buffers
        self.peer_buffers
            .lock()
            .unwrap()
            .insert(peer_id.clone(), VecDeque::new());

        //connection is registered under peer_id. Checks for old connections under this id and replaces them.
        let previous = self
            .connections
            .lock()
            .unwrap()
            .insert(peer_id.clone(), connection.clone());
        if let Some(old) = previous {
            old.close(0u32.into(), b"superseded by a newer connection to the same peer");
        }

        //takes ownership of the needed handles througgh clone to prepare for the task creation.
        let stable_id = connection.stable_id();
        let peer_buffers = self.peer_buffers.clone();
        let peer_id_for_task = peer_id.clone();
        let mesh_channel = self.clone();
        self.handle.spawn(async move {
            //the spawned tasks life cycle
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
            // when loop exits, a newer connection might already exist. Check for a race below.
            let left = mesh_channel.remove_peer_if_current(&peer_id_for_task, stable_id);
            if left {
                let space_id = space_id_for_room(&mesh_channel.room_spaces, mesh_channel.room_id);
                if let Some(space_id) = space_id {
                    let _ = mesh_channel.events.send(crate::events::AppEvent::CallParticipantLeft {
                        space_id,
                        room_id: mesh_channel.room_id,
                        endpoint_id: peer_id_for_task,
                    });
                }
            }
        });
    }

    /// remove a peer from the active mesh voice call on this clients side.
    pub fn remove_peer(&self, peer_id: &str) {
        self.connections.lock().unwrap().remove(peer_id);
        self.peer_buffers.lock().unwrap().remove(peer_id);
    }

    /// removes the peers connection only if it hasnt been superseded by a new (reconect) connection.
    fn remove_peer_if_current(&self, peer_id: &str, stable_id: usize) -> bool {
        let mut connections = self.connections.lock().unwrap();
        let is_current = connections.get(peer_id).map(|c| c.stable_id()) == Some(stable_id);
        if is_current {
            connections.remove(peer_id);
            drop(connections);
            self.peer_buffers.lock().unwrap().remove(peer_id);
        }
        is_current
    }

    /// Closes every peer connection and clears local state. Called when a
    /// call ends, so connections are torn down immediately.
    pub(crate) fn close_all(&self) {
        let connections = std::mem::take(&mut *self.connections.lock().unwrap());
        self.peer_buffers.lock().unwrap().clear();
        for connection in connections.into_values() {
            connection.close(0u32.into(), b"call ended");
        }
    }

    /// returns list of all currently connected peers
    pub fn peer_ids(&self) -> Vec<String> {
        self.connections.lock().unwrap().keys().cloned().collect()
    }

    /// looks for a specific peers registered connection and returns it if there.
    pub fn connection_for(&self, peer_id: &str) -> Option<Connection> {
        self.connections.lock().unwrap().get(peer_id).cloned()
    }

    /// returns the amount of audio samples that havent been processed in the jitter buffer.
    pub fn buffered_sample_count(&self, peer_id: &str) -> Option<usize> {
        self.peer_buffers.lock().unwrap().get(peer_id).map(|b| b.len())
    }

    /// returns the amount of currently registered connections
    pub fn connection_count(&self) -> usize {
        self.connections.lock().unwrap().len()
    }
}

/// Wraps a call's peer buffers as one mixer voice: averages every
/// currently-talking peer together, then adds that average into the
/// shared output mix as a single source.
struct CallVoiceSource(MeshVoiceChannel);

impl MixSource for CallVoiceSource {
    /// Engine calls this once for every registered source to create the final output audio stream.
    fn mix_into(&mut self, out: &mut [f32]) {
        // used to gather the peers buffers before normalizing.
        let mut scratch = vec![0.0f32; out.len()];
        // count of peers that have audio ready to process.
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
        // guard to not divide by zero
        if active > 0 {
            // the aveeraging factor
            let inv = 1.0 / active as f32;
            // pairs output to scratch buffer then adds the calls averaged contribution to shared buffer
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
    channel: MeshVoiceChannel,
}

/// Connects the sound engine to one specific active call.
impl MeshActiveCall {

    pub fn start(channel: MeshVoiceChannel, sound: Arc<SoundHandler>) -> Self {
        // Outgoing half of the wiring to the sound engine.
        let capture_channel = channel.clone();
        if let Err(e) = sound.set_capture_listener(Some(Box::new(move |samples: &[f32]| {
            capture_channel.broadcast_samples(samples);
        }))) {
            eprintln!("[mesh voice] failed to start capture: {e:#}");
        }
        // incoming half of the current active call.
        let voice_handle = match sound.register_call_voice(CallVoiceSource(channel.clone())) {
            Ok(handle) => Some(handle),
            Err(e) => {
                eprintln!("[mesh voice] failed to start playback: {e:#}");
                None
            }
        };

        Self { sound, voice_handle, channel }
    }

    pub fn hang_up(self) {}
}

impl Drop for MeshActiveCall {
    // self is out of scope after this call
    fn drop(&mut self) {
        let _ = self.sound.set_capture_listener(None);
        self.voice_handle = None;
        // close every peer connection
        self.channel.close_all();
    }
}

/// Maps Rooms to their Spaces
pub type RoomSpaceMap = Arc<StdMutex<HashMap<Uuid, Uuid>>>;
/// Node wide table of room id to live meshvoicechannel mapping
pub type ActiveCallRooms = Arc<StdMutex<HashMap<Uuid, MeshVoiceChannel>>>;

/// Iroh protocol handler for responding to incoming connections for call rooms.
/// Get registered with nodes Router.
#[derive(Clone)]
pub struct CallRoomProtocol {
    // shared node state
    active_rooms: ActiveCallRooms, //check if in the room that is being dialed for
    room_spaces: RoomSpaceMap, //to resolve room_id into space_id to correclty create the resulting event.
    events: broadcast::Sender<crate::events::AppEvent>, // for publishing newcallparticipant event
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
    /// mesh calls handshake client response when being dialed.
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_id = connection.remote_id().to_string(); //dialing peers iroh endpoint id

        let (mut send, mut recv) = connection.accept_bi().await.map_err(std::io::Error::other)?; // waits for bidirectional stream and splits.

        // read handshakes payload
        let mut room_id_bytes = [0u8; 16];
        recv.read_exact(&mut room_id_bytes).await.map_err(std::io::Error::other)?;
        let room_id = Uuid::from_bytes(room_id_bytes);

        // check for clients membership of dialed room
        let channel_opt = {
            let rooms = self.active_rooms.lock().unwrap();
            rooms.get(&room_id).cloned()
        };

        // responds according to membership of room (ack[0] 1 byte for accept)
        match channel_opt {
            Some(mesh_channel) => {
                send.write_all(&[1u8]).await.map_err(std::io::Error::other)?;
                send.finish().map_err(std::io::Error::other)?;
                mesh_channel.add_peer(remote_id.clone(), connection);

                // resolves owning space and publish newcallparticipant event.
                let space_id = space_id_for_room(&self.room_spaces, room_id);
                if let Some(space_id) = space_id {
                    let _ = self.events.send(crate::events::AppEvent::NewCallParticipant {
                        space_id,
                        room_id,
                        endpoint_id: remote_id,
                    });
                }
            }
            // if no active channel for specified room, write back 0
            None => {
                send.write_all(&[0u8]).await.map_err(std::io::Error::other)?;
                send.finish().map_err(std::io::Error::other)?;
            }
        }
        Ok(())
    }
}
