use anyhow::Result;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh_blobs::store::fs::FsStore;
use tokio::sync::broadcast;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use uuid::Uuid;

use crate::sound::{MixSource, SoundHandler, VoiceHandle};
use crate::space::Space;
use crate::voice_codec::{VoiceDecoder, VoiceEncoder, FRAME_SAMPLES, SEQ_HEADER_LEN};

/// ALPN mesh call rooms use to dial each other for the group-call audio mesh.
pub const CALL_ROOM_ALPN: &[u8] = b"blabber/callroom/1";

/// How long an inbound dial waits for the caller's member record to show up
/// before being rejected.
const MEMBER_SYNC_GRACE: Duration = Duration::from_secs(3);
const MEMBER_SYNC_POLL: Duration = Duration::from_millis(100);

/// Per-peer jitter buffer bounds, in wire samples. Audio can arrive
/// faster than it's played. Once a buffer exceeds the max it's trimmed back to
/// the target, memory stays bounded and latency recovers.
pub const PEER_BUFFER_TARGET: usize = 48_000 * 60 / 1000;
pub const PEER_BUFFER_MAX: usize = 48_000 * 200 / 1000;

/// A peer starts and restarts playing only once this
/// much audio is buffered, so normal network jitter doesn't cause gaps.
pub(crate) const PEER_BUFFER_PRIME: usize = 48_000 * 40 / 1000;
/// Each time a peer runs dry its prime grows by this, up to PEER_PRIME_MAX:
/// senders whose mic delivers large bursts, or bad networks, get a deeper
/// buffer, everyone else keeps the low latency.
const PEER_PRIME_STEP: usize = 48_000 * 20 / 1000;
/// Stays well below PEER_BUFFER_MAX, so a full prime plus a burst never trims.
const PEER_PRIME_MAX: usize = 48_000 * 100 / 1000;
/// Fade length for starting/stopping a peer's playback without a click.
const PEER_FADE_SAMPLES: usize = 48_000 * 5 / 1000;

/// How many newer packets may arrive before a missing one is declared lost.
const REORDER_WINDOW: u32 = 2;
/// Lost frames in a row that get concealed. Longer gaps are skipped.
const MAX_CONCEALED_FRAMES: u32 = 5;
/// A sequence jump this large means the sender restarted, start over there.
const RESYNC_DISTANCE: u32 = 50;

/// Appends received samples to a peer's jitter buffer, dropping the oldest audio
/// down to PEER_BUFFER_TARGET once it grows past PEER_BUFFER_MAX.
fn push_samples(buf: &mut VecDeque<f32>, samples: Vec<f32>) {
    buf.extend(samples);
    if buf.len() > PEER_BUFFER_MAX {
        let excess = buf.len() - PEER_BUFFER_TARGET;
        buf.drain(..excess);
    }
}

/// One peer's decoded audio waiting to be played, plus its playout state.
/// Plays only once primed, fades out when it runs
/// dry, then waits to be primed again and fades back in.
pub(crate) struct PeerBuffer {
    samples: VecDeque<f32>,
    playing: bool,
    fade_in_left: usize,
    /// current adaptive prime, see PEER_PRIME_STEP
    prime: usize,
}

impl Default for PeerBuffer {
    fn default() -> Self {
        Self { samples: VecDeque::new(), playing: false, fade_in_left: 0, prime: PEER_BUFFER_PRIME }
    }
}

impl PeerBuffer {
    pub(crate) fn push(&mut self, samples: Vec<f32>) {
        push_samples(&mut self.samples, samples);
    }

    pub(crate) fn len(&self) -> usize {
        self.samples.len()
    }

    /// Adds up to out.len() samples of this peer onto out.
    pub(crate) fn mix_into(&mut self, out: &mut [f32]) {
        if !self.playing {
            if self.samples.len() < self.prime {
                return;
            }
            self.playing = true;
            self.fade_in_left = PEER_FADE_SAMPLES;
        }
        let n = out.len().min(self.samples.len());
        let runs_dry = self.samples.len() <= out.len();
        let fade_out_len = if runs_dry { n.min(PEER_FADE_SAMPLES) } else { 0 };
        for (i, slot) in out[..n].iter_mut().enumerate() {
            let sample = self.samples.pop_front().unwrap_or(0.0);
            let mut gain = 1.0;
            if self.fade_in_left > 0 {
                gain *= 1.0 - self.fade_in_left as f32 / PEER_FADE_SAMPLES as f32;
                self.fade_in_left -= 1;
            }
            let left = n - i;
            if left <= fade_out_len {
                gain *= (left - 1) as f32 / fade_out_len as f32;
            }
            *slot += sample * gain;
        }
        if runs_dry {
            self.playing = false;
            self.prime = (self.prime + PEER_PRIME_STEP).min(PEER_PRIME_MAX);
        }
    }
}

/// Splits a received packet into sequence number and Opus payload.
fn parse_packet(bytes: &[u8]) -> Option<(u32, &[u8])> {
    if bytes.len() <= SEQ_HEADER_LEN {
        return None;
    }
    let seq = u32::from_le_bytes(bytes[..SEQ_HEADER_LEN].try_into().unwrap());
    Some((seq, &bytes[SEQ_HEADER_LEN..]))
}

/// `a - b` in sequence space, correct across u32 wrap-around.
fn seq_diff(a: u32, b: u32) -> i32 {
    a.wrapping_sub(b) as i32
}

/// Turns one peer's packets back into an in-order stream of 20 ms frames.
/// Packets are held for a short reorder window; a frame that stays missing is
/// rebuilt from the FEC data of its successor when that arrived, otherwise
/// concealed by the decoder.
struct PeerReceiver {
    decoder: VoiceDecoder,
    next_seq: Option<u32>,
    pending: BTreeMap<u32, Vec<u8>>,
    concealed_run: u32,
}

impl PeerReceiver {
    fn new(decoder: VoiceDecoder) -> Self {
        Self { decoder, next_seq: None, pending: BTreeMap::new(), concealed_run: 0 }
    }

    /// Accepts a packet and returns the frames that became playable, in order.
    fn receive(&mut self, seq: u32, payload: &[u8]) -> Vec<Vec<f32>> {
        let next = *self.next_seq.get_or_insert(seq);
        let ahead = seq_diff(seq, next);
        if ahead < 0 {
            // too late, its slot was already filled or concealed
            return Vec::new();
        }
        if ahead as u32 > RESYNC_DISTANCE {
            self.pending.clear();
            self.next_seq = Some(seq);
            self.concealed_run = 0;
        }
        self.pending.insert(seq, payload.to_vec());

        let mut frames = Vec::new();
        while let Some(next) = self.next_seq {
            if let Some(payload) = self.pending.remove(&next) {
                self.concealed_run = 0;
                frames.push(self.decoder.decode(&payload).unwrap_or_else(|_| self.conceal()));
                self.next_seq = Some(next.wrapping_add(1));
                continue;
            }
            let newest = self.pending.keys().copied().max_by_key(|&k| seq_diff(k, next));
            let Some(newest) = newest else { break };
            if (seq_diff(newest, next) as u32) < REORDER_WINDOW {
                break;
            }
            // next is lost
            if self.concealed_run >= MAX_CONCEALED_FRAMES {
                let oldest = self.pending.keys().copied().min_by_key(|&k| seq_diff(k, next)).unwrap();
                self.next_seq = Some(oldest);
                self.concealed_run = 0;
                continue;
            }
            self.concealed_run += 1;
            let frame = match self.pending.get(&next.wrapping_add(1)) {
                Some(following) => self.decoder.decode_fec(following).unwrap_or_else(|_| self.conceal()),
                None => self.conceal(),
            };
            frames.push(frame);
            self.next_seq = Some(next.wrapping_add(1));
        }
        frames
    }

    fn conceal(&mut self) -> Vec<f32> {
        self.decoder.conceal().unwrap_or_else(|_| vec![0.0; FRAME_SAMPLES])
    }
}

/// checks what space a given call room belongs to.
fn space_id_for_room(room_spaces: &RoomSpaceMap, room_id: Uuid) -> Option<Uuid> {
    room_spaces.lock().unwrap().get(&room_id).copied()
}

///
#[derive(Clone)]
pub struct MeshVoiceChannel {
    connections: Arc<StdMutex<HashMap<String, Connection>>>,
    peer_buffers: Arc<StdMutex<HashMap<String, PeerBuffer>>>,
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

    /// Sends one encoded voice packet to every connected peer.
    pub(crate) fn broadcast_packet(&self, packet: &[u8]) {
        let conns = self.connections.lock().unwrap();
        for connection in conns.values() {
            if let Err(e) = connection.send_datagram(packet.to_vec().into()) {
                eprintln!("failed to send audio datagram: {e}");
            }
        }
    }

    /// add a new peer to mesh and spawns background task that consumes the incoming audio from this peer.
    pub fn add_peer(&self, peer_id: String, connection: Connection) {
        // create fresh jitter buffer for peer in map peer_buffers
        self.peer_buffers
            .lock()
            .unwrap()
            .insert(peer_id.clone(), PeerBuffer::default());

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
            // without the audio feature there is no decoder, datagrams are
            // still read so a disconnect is noticed
            let mut receiver = match VoiceDecoder::new() {
                Ok(decoder) => Some(PeerReceiver::new(decoder)),
                Err(e) => {
                    eprintln!("[mesh voice] no decoder for {peer_id_for_task}: {e:#}");
                    None
                }
            };
            //the spawned tasks life cycle
            loop {
                match connection.read_datagram().await {
                    Ok(bytes) => {
                        let (Some(receiver), Some((seq, payload))) = (receiver.as_mut(), parse_packet(&bytes)) else {
                            continue;
                        };
                        let frames = receiver.receive(seq, payload);
                        if frames.is_empty() {
                            continue;
                        }
                        let mut buffers = peer_buffers.lock().unwrap();
                        if let Some(buf) = buffers.get_mut(&peer_id_for_task) {
                            for frame in frames {
                                buf.push(frame);
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

/// Wraps a call's peer buffers as one mixer voice.
struct CallVoiceSource(MeshVoiceChannel);

impl MixSource for CallVoiceSource {
    /// Engine calls this once for every registered source to create the final output audio stream.
    fn mix_into(&mut self, out: &mut [f32]) {
        let mut buffers = self.0.peer_buffers.lock().unwrap();
        for buf in buffers.values_mut() {
            buf.mix_into(out);
        }
    }
}

/// An active mesh call: registers this call mic capture and its mixed
/// peer audio with the shared `SoundHandler`, and unregisters both on drop.
pub struct MeshActiveCall {
    sound: Arc<SoundHandler>,
    /// `None` if the mic couldn't be opened
    capture_id: Option<u64>,
    voice_handle: Option<VoiceHandle>,
    channel: MeshVoiceChannel,
}

/// Connects the sound engine to one specific active call.
impl MeshActiveCall {

    pub fn start(channel: MeshVoiceChannel, sound: Arc<SoundHandler>) -> Self {
        // Outgoing half of the wiring to the sound engine: encode once,
        // send the same packet to every peer.
        let capture_channel = channel.clone();
        let capture_id = match VoiceEncoder::new().and_then(|mut encoder| {
            sound.start_capture(Box::new(move |samples: &[f32]| {
                for packet in encoder.push(samples) {
                    capture_channel.broadcast_packet(&packet);
                }
            }))
        }) {
            Ok(id) => Some(id),
            Err(e) => {
                eprintln!("[mesh voice] failed to start capture: {e:#}");
                None
            }
        };
        // incoming half of the current active call.
        let voice_handle = match sound.register_call_voice(CallVoiceSource(channel.clone())) {
            Ok(handle) => Some(handle),
            Err(e) => {
                eprintln!("[mesh voice] failed to start playback: {e:#}");
                None
            }
        };

        Self { sound, capture_id, voice_handle, channel }
    }

    pub fn hang_up(self) {}
}

impl Drop for MeshActiveCall {
    // self is out of scope after this call
    fn drop(&mut self) {
        // no-op if a newer call has taken over the mic since
        if let Some(id) = self.capture_id {
            self.sound.stop_capture(id);
        }
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
    spaces: Arc<tokio::sync::Mutex<Vec<Space>>>, // node's spaces, to check the dialer is a member
    blobs: FsStore, // to read member records straight from the members doc
}

impl CallRoomProtocol {
    pub fn new(
        active_rooms: ActiveCallRooms,
        room_spaces: RoomSpaceMap,
        events: broadcast::Sender<crate::events::AppEvent>,
        spaces: Arc<tokio::sync::Mutex<Vec<Space>>>,
        blobs: FsStore,
    ) -> Self {
        Self { active_rooms, room_spaces, events, spaces, blobs }
    }

    /// True if endpoint_id belongs to a member of the space. Reads the members
    /// doc directly, not `member_cache`: the cache can permanently miss records
    /// that synced between its initial seed and its live subscription.
    async fn is_space_member(&self, space_id: Uuid, endpoint_id: &str) -> bool {
        let space = {
            let spaces = self.spaces.lock().await;
            match spaces.iter().find(|space| space.id() == space_id) {
                Some(space) => space.clone(),
                None => return false,
            }
        };
        let Ok(members) = space.list_members(&self.blobs).await else {
            return false;
        };
        members
            .iter()
            .any(|member| !member.is_relay && member.endpoint_id == endpoint_id)
    }

    /// is_space_member, retried for MEMBER_SYNC_GRACE so a newly joined
    /// member isn't rejected.
    async fn wait_for_space_member(&self, space_id: Uuid, endpoint_id: &str) -> bool {
        let deadline = tokio::time::Instant::now() + MEMBER_SYNC_GRACE;
        loop {
            if self.is_space_member(space_id, endpoint_id).await {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(MEMBER_SYNC_POLL).await;
        }
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

        // only members of the space owning the room can join the mesh.
        let authorized = match (&channel_opt, space_id_for_room(&self.room_spaces, room_id)) {
            (Some(_), Some(space_id)) => {
                if self.wait_for_space_member(space_id, &remote_id).await {
                    Some(space_id)
                } else {
                    eprintln!("rejected dial from {remote_id} for room {room_id}");
                    None
                }
            }
            _ => None,
        };

        // responds according to membership of room (ack[0] 1 byte for accept)
        match (channel_opt, authorized) {
            (Some(mesh_channel), Some(space_id)) => {
                send.write_all(&[1u8]).await.map_err(std::io::Error::other)?;
                send.finish().map_err(std::io::Error::other)?;
                mesh_channel.add_peer(remote_id.clone(), connection);

                let _ = self.events.send(crate::events::AppEvent::NewCallParticipant {
                    space_id,
                    room_id,
                    endpoint_id: remote_id,
                });
            }
            // not in this room, or the dialer isnt a member of its space: write back 0
            _ => {
                send.write_all(&[0u8]).await.map_err(std::io::Error::other)?;
                send.finish().map_err(std::io::Error::other)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_samples_keeps_buffer_below_max() {
        let mut buf = VecDeque::new();
        for _ in 0..100 {
            push_samples(&mut buf, vec![0.0; 292]);
            assert!(buf.len() <= PEER_BUFFER_MAX);
        }
    }

    #[test]
    fn push_samples_trims_to_target_and_keeps_newest() {
        let mut buf: VecDeque<f32> = (0..PEER_BUFFER_MAX).map(|i| i as f32).collect();
        // one more packet pushes it over the max
        let newest: Vec<f32> = (0..292).map(|i| (PEER_BUFFER_MAX + i) as f32).collect();
        push_samples(&mut buf, newest);

        assert_eq!(buf.len(), PEER_BUFFER_TARGET);
        assert_eq!(*buf.back().unwrap(), (PEER_BUFFER_MAX + 291) as f32);
        assert_eq!(*buf.front().unwrap(), (PEER_BUFFER_MAX + 292 - PEER_BUFFER_TARGET) as f32);
    }

    #[test]
    fn push_samples_below_max_keeps_everything() {
        let mut buf = VecDeque::new();
        push_samples(&mut buf, vec![1.0; PEER_BUFFER_MAX]);
        assert_eq!(buf.len(), PEER_BUFFER_MAX);
    }

    fn primed_buffer(value: f32) -> PeerBuffer {
        let mut buf = PeerBuffer::default();
        buf.push(vec![value; PEER_BUFFER_PRIME]);
        buf
    }

    #[test]
    fn peer_buffer_waits_until_primed() {
        let mut buf = PeerBuffer::default();
        buf.push(vec![1.0; PEER_BUFFER_PRIME - 1]);
        let mut out = vec![0.0; 480];
        buf.mix_into(&mut out);
        assert!(out.iter().all(|&s| s == 0.0));
        assert_eq!(buf.samples.len(), PEER_BUFFER_PRIME - 1);
    }

    #[test]
    fn peer_buffer_fades_in_then_plays_at_full_level() {
        let mut buf = primed_buffer(1.0);
        let mut out = vec![0.0; 480];
        buf.mix_into(&mut out);
        assert_eq!(out[0], 0.0);
        assert!(out.windows(2).take(PEER_FADE_SAMPLES).all(|w| w[1] >= w[0]));
        assert_eq!(out[479], 1.0);
    }

    #[test]
    fn peer_buffer_fades_out_and_reprimes_when_running_dry() {
        let mut buf = primed_buffer(1.0);
        let mut out = vec![0.0; PEER_BUFFER_PRIME];
        buf.mix_into(&mut out);
        assert_eq!(*out.last().unwrap(), 0.0);
        assert!(out[PEER_BUFFER_PRIME / 2] == 1.0);
        assert!(!buf.playing);

        // a trickle of audio is not played until primed again
        buf.push(vec![1.0; 480]);
        let mut out = vec![0.0; 480];
        buf.mix_into(&mut out);
        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn peer_buffer_deepens_prime_each_time_it_runs_dry() {
        let mut buf = PeerBuffer::default();
        let mut primes = Vec::new();
        for _ in 0..10 {
            primes.push(buf.prime);
            buf.push(vec![1.0; buf.prime]);
            let mut out = vec![0.0; buf.prime];
            buf.mix_into(&mut out);
            assert!(!buf.playing);
        }
        assert_eq!(primes[0], PEER_BUFFER_PRIME);
        assert_eq!(primes[1], PEER_BUFFER_PRIME + PEER_PRIME_STEP);
        assert_eq!(*primes.last().unwrap(), PEER_PRIME_MAX);
        assert!(PEER_PRIME_MAX + 3 * 960 < PEER_BUFFER_MAX, "a full prime plus a burst must not trim");
    }

    #[test]
    fn peer_buffer_adds_onto_existing_mix() {
        let mut a = primed_buffer(0.25);
        let mut b = primed_buffer(0.5);
        let mut out = vec![0.0; 480];
        a.mix_into(&mut out);
        b.mix_into(&mut out);
        assert_eq!(out[479], 0.75);
    }

    #[test]
    fn parse_packet_rejects_runts() {
        assert!(parse_packet(&[1, 0, 0, 0]).is_none());
        assert_eq!(parse_packet(&[2, 0, 0, 0, 9]), Some((2, &[9u8][..])));
    }

    #[cfg(feature = "audio")]
    mod receiver {
        use super::*;

        /// `count` encoded packets of a 440 Hz tone, split into (seq, payload).
        fn packets(count: usize) -> Vec<(u32, Vec<u8>)> {
            let mut encoder = VoiceEncoder::new().unwrap();
            let tone: Vec<f32> = (0..count * FRAME_SAMPLES)
                .map(|i| 0.3 * (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / 48_000.0).sin())
                .collect();
            encoder.push(&tone)
                .iter()
                .map(|p| {
                    let (seq, payload) = parse_packet(p).unwrap();
                    (seq, payload.to_vec())
                })
                .collect()
        }

        fn receiver() -> PeerReceiver {
            PeerReceiver::new(VoiceDecoder::new().unwrap())
        }

        #[test]
        fn in_order_packets_decode_immediately() {
            let mut rx = receiver();
            for (seq, payload) in packets(5) {
                let frames = rx.receive(seq, &payload);
                assert_eq!(frames.len(), 1);
                assert_eq!(frames[0].len(), FRAME_SAMPLES);
            }
        }

        #[test]
        fn swapped_packets_are_reordered() {
            let p = packets(3);
            let mut rx = receiver();
            assert_eq!(rx.receive(p[0].0, &p[0].1).len(), 1);
            assert_eq!(rx.receive(p[2].0, &p[2].1).len(), 0, "should wait for the missing packet");
            assert_eq!(rx.receive(p[1].0, &p[1].1).len(), 2);
            assert_eq!(rx.next_seq, Some(3));
        }

        #[test]
        fn lost_packet_is_filled_after_reorder_window() {
            let p = packets(4);
            let mut rx = receiver();
            rx.receive(p[0].0, &p[0].1);
            // packet 1 never arrives
            assert_eq!(rx.receive(p[2].0, &p[2].1).len(), 0);
            let frames = rx.receive(p[3].0, &p[3].1);
            // FEC/concealed frame for 1, then 2 and 3
            assert_eq!(frames.len(), 3);
            assert!(frames.iter().all(|f| f.len() == FRAME_SAMPLES));
        }

        #[test]
        fn late_packet_is_dropped() {
            let p = packets(4);
            let mut rx = receiver();
            rx.receive(p[0].0, &p[0].1);
            rx.receive(p[2].0, &p[2].1);
            rx.receive(p[3].0, &p[3].1);
            assert_eq!(rx.receive(p[1].0, &p[1].1).len(), 0);
        }

        #[test]
        fn long_gap_is_skipped_not_concealed() {
            let p = packets(20);
            let mut rx = receiver();
            rx.receive(p[0].0, &p[0].1);
            let mut produced = 0;
            for (seq, payload) in &p[15..] {
                produced += rx.receive(*seq, payload).len();
            }
            // 5 concealed frames for the gap, then 15..20 decoded
            assert_eq!(produced, MAX_CONCEALED_FRAMES as usize + 5);
            assert_eq!(rx.next_seq, Some(20));
        }

        #[test]
        fn sender_restart_resyncs() {
            let mut rx = receiver();
            let p = packets(2);
            rx.receive(p[0].0, &p[0].1);
            let far = 1_000_000;
            assert_eq!(rx.receive(far, &p[1].1).len(), 1);
            assert_eq!(rx.next_seq, Some(far + 1));
        }

        #[test]
        fn encoder_buffers_partial_frames() {
            let mut encoder = VoiceEncoder::new().unwrap();
            assert!(encoder.push(&[0.0; FRAME_SAMPLES - 1]).is_empty());
            let packets = encoder.push(&[0.0; FRAME_SAMPLES + 1]);
            assert_eq!(packets.len(), 2);
            assert_eq!(parse_packet(&packets[0]).unwrap().0, 0);
            assert_eq!(parse_packet(&packets[1]).unwrap().0, 1);
            assert!(packets.iter().all(|p| p.len() < 400), "voice packets should be small");
        }
    }
}
