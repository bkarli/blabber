use anyhow::{anyhow, Context, Result};
use chacha20poly1305::{aead::Aead, ChaCha20Poly1305, KeyInit, Nonce};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::StreamConfig;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;


// Voice chat module: sends mic audio over udp(encrypted with ChaCha20Poly1305).
// how to use:
//   let key: [u8; 32] = ...; ->same key on both sides!
//   let channel = VoiceChannel::new(local, remote, key)?;
//   let _capture = channel.start_capture()?; ->mic -> encrypt -> send
//   let _playback = channel.start_playback()?; ->recv -> decrypt -> speakers
//   
// notes:
//- key has to be shared somehow (not handled yet)
//- nonce counter resets on every new(), so dont reuse same key across sessions


const NONCE_LEN: usize = 12;
const MAX_PACKET_SIZE: usize = 1400;

pub struct VoiceChannel {
    socket: Arc<UdpSocket>,
    remote_addr: SocketAddr,
    cipher: Arc<ChaCha20Poly1305>,
    send_counter: Arc<AtomicU64>,
}

// https://docs.rs/cpal/latest/cpal/
impl VoiceChannel {
    pub fn new(local_addr: SocketAddr, remote_addr: SocketAddr, key: [u8; 32]) -> Result<Self> {
        let socket = UdpSocket::bind(local_addr).context("failed to bind udp socket")?;
        let cipher = ChaCha20Poly1305::new((&key).into());
        Ok(Self {socket: Arc::new(socket),remote_addr,cipher: Arc::new(cipher),send_counter: Arc::new(AtomicU64::new(0)),})
    }

    fn nonce_from_counter(counter: u64) -> [u8; NONCE_LEN] {
        let mut nonce = [0u8; NONCE_LEN];
        nonce[..8].copy_from_slice(&counter.to_be_bytes());
        nonce
    }

    ///captures microphone input, encrypts it & sends it to the remote peer.
    pub fn start_capture(&self) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| anyhow!("no input device available"))?;
        let supported_config = device.default_input_config().context("no supported input config")?;
        let stream_config: StreamConfig = supported_config.into();

        let socket = Arc::clone(&self.socket);
        let remote_addr = self.remote_addr;
        let cipher = Arc::clone(&self.cipher);
        let counter = Arc::clone(&self.send_counter);

        let stream = device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                send_audio_chunk(&socket, remote_addr, &cipher, &counter, data);},
            move |err| {
                eprintln!("audio capture error: {err}");},
            None,
        )?;

        stream.play().context("failed to start capture stream")?;
        Ok(stream)
    }

    ///receives UDP packets, decrypts them & plays audio over speakers.
    pub fn start_playback(&self) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| anyhow!("no output device available"))?;
        let config = device.default_output_config().context("no default output config")?;
        let stream_config: StreamConfig = config.into();
        let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = channel();
        let socket = Arc::clone(&self.socket);
        let cipher = Arc::clone(&self.cipher);
        thread::spawn(move || {let mut buf = [0u8; MAX_PACKET_SIZE];
            loop {
                match socket.recv_from(&mut buf) {
                    Ok((len, _src)) => {
                        if let Some(samples) = decrypt_packet(&cipher, &buf[..len]) {
                            let _ = tx.send(samples);
                        }
                    }
                    Err(e) => {
                        eprintln!("UDP recv error: {e}");
                        break;
                    }
                }
            }
        });

        let mut pending: Vec<f32> = Vec::new();
        let err_fn = |err| eprintln!("audio playback error: {err}");
        let stream = device.build_output_stream(
            &stream_config,
            move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                while pending.len() < output.len() {
                    match rx.try_recv() {
                        Ok(samples) => pending.extend(samples),
                        Err(_) => break,
                    }}
                let n = output.len().min(pending.len());
                output[..n].copy_from_slice(&pending[..n]);
                for s in &mut output[n..] {
                    *s = 0.0;
                }
                pending.drain(..n);},
            err_fn,
            None,
        )?;
        stream.play().context("failed to start playback stream")?;
        Ok(stream)
    }
}
const PACKET_OVERHEAD: usize = 16 + 8;
const MAX_PLAINTEXT_BYTES: usize = MAX_PACKET_SIZE - PACKET_OVERHEAD;
const SAMPLES_PER_PACKET: usize = MAX_PLAINTEXT_BYTES / 4;

fn send_audio_chunk(
    socket: &UdpSocket,
    remote_addr: SocketAddr,
    cipher: &ChaCha20Poly1305,
    counter: &AtomicU64,
    data: &[f32],
) {
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
                if packet.len() > MAX_PACKET_SIZE {
                    eprintln!("dropping oversized audio packet ({} bytes)", packet.len());
                    continue;
                }
                let _ = socket.send_to(&packet, remote_addr);
            }
            Err(e) => eprintln!("encryption failed: {e}"),
        }
    }
}

fn decrypt_packet(cipher: &ChaCha20Poly1305, packet: &[u8]) -> Option<Vec<f32>> {
    if packet.len() < 8 {return None;}
    let (counter_bytes, ciphertext) = packet.split_at(8);
    let counter = u64::from_be_bytes(counter_bytes.try_into().ok()?);
    let nonce_bytes = VoiceChannel::nonce_from_counter(counter);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher.decrypt(nonce, ciphertext).ok()?;
    if plaintext.len() % 4 != 0 {
        return None;
    }
    Some(
        plaintext.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect(),
    )
}

//test
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use std::time::Duration;
//
//     #[test]
//     fn loopback_test() -> Result<()> {
//         let key = [42u8; 32]; //hardcoded test key
//
//         let addr_a: SocketAddr = "127.0.0.1:6000".parse()?;
//         let addr_b: SocketAddr = "127.0.0.1:6001".parse()?;
//
//         //A sends to B, B sends to A
//         let channel_a = VoiceChannel::new(addr_a, addr_b, key)?;
//         let channel_b = VoiceChannel::new(addr_b, addr_a, key)?;
//         let _capture = channel_a.start_capture()?;   //A captures with mic
//         let _playback = channel_b.start_playback()?; //B plays it over speakers
//
//         println!("Sprich jetzt ins Mikrofon... (10 Sekunden)");
//         thread::sleep(Duration::from_secs(10));
//
//         Ok(())
//     }
// }
