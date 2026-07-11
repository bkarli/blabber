use anyhow::{anyhow, Context, Result};
use chacha20poly1305::{aead::Aead, ChaCha20Poly1305, KeyInit, Nonce};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

const NONCE_LEN: usize = 12;
const MAX_PACKET_SIZE: usize = 1400;

struct VoiceChannel{
    socket: Arc<UdpSocket>,
    remote_addr: SocketAddr,
    cipher: Arc<ChaCha20Poly1305>,
    send_counter: Arc<AtomicU64>,
}


impl VoiceChannel{


    //https://docs.rs/cpal/latest/cpal/
    pub fn start_capture(&self)->Result<cpal::Stream>{
    let host = cpal::default_host();
    let device = host.default_output_device().expect("no output device available");
    let mut supported_configs_range = device.supported_output_configs().expect("error while querying configs");
    let supported_config = supported_configs_range.next().expect("no supported config").with_max_sample_rate();
}
}



