// Voice chat module: sends mic audio over an iroh QUIC connection (encrypted with ChaCha20Poly1305 on top)
// How to use: establish connection with a peer (e.g. from Node::call() on initiating side or from Voice::protocol accept on accepting side
//
//   let key: [u8; 32] = ...;                              // same key on both sides!
//   let channel = VoiceChannel::new(connection, key);
//   let _capture = channel.start_capture()?;               // mic -> encrypt -> send
//   let _playback = channel.start_playback()?;             // recv -> decrypt -> speakers
//   // keep both streams alive (don't let them drop) or audio stops
//
// notes:
// - key has to be shared somehow (not handled yet -> maybe Diffie-Hellman or such)
// - use headphones!
// - nonce counter resets on every new()-> don't reuse the same key across sessions

use anyhow::{anyhow, Context, Result};
use chacha20poly1305::{aead::Aead, ChaCha20Poly1305, KeyInit, Nonce};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::StreamConfig;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub const VOICE_ALPN: &[u8] = b"blabber/voice/0";

const NONCE_LEN: usize = 12;
const MAX_DATAGRAM_SIZE: usize = 1200;

pub struct VoiceChannel {
    connection: Connection,
    cipher: Arc<ChaCha20Poly1305>,
    send_counter: Arc<AtomicU64>,
}

impl VoiceChannel {
    pub fn new(connection: Connection, key: [u8; 32]) -> Self {
        let cipher = ChaCha20Poly1305::new((&key).into());
        Self {connection, cipher: Arc::new(cipher), send_counter: Arc::new(AtomicU64::new(0)),}
    }

    fn nonce_from_counter(counter: u64) -> [u8; NONCE_LEN] {
        let mut nonce = [0u8; NONCE_LEN];
        nonce[..8].copy_from_slice(&counter.to_be_bytes());
        nonce
    }

    ///Captures microphone input, encrypts it & sends it to the remote peer (as QUIC datagrams over the iroh connection)
    pub fn start_capture(&self) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| anyhow!("no input device available"))?;
        let supported_config = device.default_input_config().context("no supported input config")?;
        let stream_config: StreamConfig = supported_config.into();
        let connection = self.connection.clone();
        let cipher = Arc::clone(&self.cipher);
        let counter = Arc::clone(&self.send_counter);

        let stream = device.build_input_stream(&stream_config, move |data: &[f32], _: &cpal::InputCallbackInfo| {send_audio_chunk(&connection, &cipher, &counter, data);},
            move |err| { eprintln!("audio capture error: {err}");},
            None,)?;
        stream.play().context("failed to start capture stream")?;
        Ok(stream)
    }

    ///Receives datagrams over the iroh connection, decrypts them, and plays audio over speakers. 
    pub fn start_playback(&self, handle: &tokio::runtime::Handle) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| anyhow!("no output device available"))?;
        let config = device.default_output_config().context("no default output config")?;
        let stream_config: StreamConfig = config.into();
        let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = channel();
        let connection = self.connection.clone();
        let cipher = Arc::clone(&self.cipher);
        handle.spawn(async move {
            loop {
                match connection.read_datagram().await {
                    Ok(bytes) => {
                        if let Some(samples) = decrypt_packet(&cipher, &bytes) {
                            let _ = tx.send(samples);
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

#[derive(Debug, Clone)]
pub struct VoiceProtocol;

impl VoiceProtocol {
    pub fn new() -> Self {Self}}

impl ProtocolHandler for VoiceProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        //negotiate a fresh key
        let key = crate::node::perform_key_exchange_as_acceptor(&connection).await.map_err(std::io::Error::other)?;

        let channel = VoiceChannel::new(connection.clone(), key);

        let handle = tokio::runtime::Handle::current();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let audio_thread = std::thread::spawn(move || -> Result<()> {
            let _capture = channel.start_capture()?;
            let _playback = channel.start_playback(&handle)?;
            let _ = stop_rx.recv();
            Ok(())
        });
        connection.closed().await;
        let _ = stop_tx.send(());
        let _ = audio_thread.join();
        Ok(())
    }
}

const PACKET_OVERHEAD: usize = 16 + 8;
const MAX_PLAINTEXT_BYTES: usize = MAX_DATAGRAM_SIZE - PACKET_OVERHEAD;
const SAMPLES_PER_PACKET: usize = MAX_PLAINTEXT_BYTES / 4;

fn send_audio_chunk(connection: &Connection,cipher: &ChaCha20Poly1305,counter: &AtomicU64,data: &[f32],) {
    for sub_chunk in data.chunks(SAMPLES_PER_PACKET) {
        let n = counter.fetch_add(1, Ordering::SeqCst);
        let nonce_bytes = VoiceChannel::nonce_from_counter(n);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let raw: Vec<u8> = sub_chunk.iter().flat_map(|s| s.to_le_bytes()).collect();

        match cipher.encrypt(nonce, raw.as_slice()) {
            Ok(ciphertext) => {
                let mut packet = Vec::with_capacity(8 + ciphertext.len());
                packet.extend_from_slice(&n.to_be_bytes());
                packet.extend_from_slice(&ciphertext);
                if packet.len() > MAX_DATAGRAM_SIZE {
                    eprintln!("dropping oversized audio packet ({} bytes)", packet.len());
                    continue;}
                if let Err(e) = connection.send_datagram(packet.into()) {
                    eprintln!("failed to send audio datagram: {e}");}
            }
            Err(e) => eprintln!("encryption failed: {e}"),
}}}

fn decrypt_packet(cipher: &ChaCha20Poly1305, packet: &[u8]) -> Option<Vec<f32>> {
    if packet.len() < 8 { return None;}
    let (counter_bytes, ciphertext) = packet.split_at(8);
    let counter = u64::from_be_bytes(counter_bytes.try_into().ok()?);
    let nonce_bytes = VoiceChannel::nonce_from_counter(counter);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = cipher.decrypt(nonce, ciphertext).ok()?;
    if plaintext.len() % 4 != 0 {
        return None;}

    Some(plaintext.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect(),)
}

#[cfg(test)]
mod tests {
    use super::*;
    use iroh::endpoint::presets;
    use iroh::Endpoint;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn loopback_test() -> Result<()> {
        let key = [42u8; 32];
        let endpoint_a = Endpoint::builder(presets::N0)
            .alpns(vec![VOICE_ALPN.to_vec()])
            .bind()
            .await?;
        let endpoint_b = Endpoint::builder(presets::N0)
            .alpns(vec![VOICE_ALPN.to_vec()])
            .bind()
            .await?;
        let addr_b = endpoint_b.addr();
      
        let endpoint_b_for_accept = endpoint_b.clone();
        let accept_task = tokio::spawn(async move {
            let incoming = endpoint_b_for_accept
                .accept()
                .await
                .context("no incoming connection")?;
            let connection = incoming.await.context("failed to accept connection")?;
            Ok::<_, anyhow::Error>(connection)
        });

        //A connects to B
        let connection_a = endpoint_a
            .connect(addr_b, VOICE_ALPN)
            .await
            .context("A failed to connect to B")?;

        let connection_b = accept_task.await.context("accept task failed")??;

        let channel_a = VoiceChannel::new(connection_a, key);
        let channel_b = VoiceChannel::new(connection_b, key);

        let _capture = channel_a.start_capture()?; //A captures mic, sends to B
        let handle = tokio::runtime::Handle::current();
        let _playback = channel_b.start_playback(&handle)?; //B plays it over speakers/headphones
        println!("Speak into the mic (10 seconds)");
        sleep(Duration::from_secs(10)).await;
        Ok(())
    }
}