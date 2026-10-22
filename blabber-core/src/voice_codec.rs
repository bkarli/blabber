//! Opus voice codec for mesh calls. One packet carries one 20 ms mono frame at
//! the 48 kHz wire rate, prefixed with a little-endian u32 sequence number so
//! the receiver can reorder and detect loss (see `channel::PeerReceiver`).

use anyhow::{anyhow, Result};
use opus::{Application, Bitrate, Channels, Decoder, Encoder};

/// Samples per packet: 20 ms at 48 kHz.
pub const FRAME_SAMPLES: usize = 960;
pub const SEQ_HEADER_LEN: usize = 4;
const SAMPLE_RATE: u32 = 48_000;
const BITRATE_BPS: i32 = 32_000;
/// Loss the encoder plans FEC redundancy for.
const EXPECTED_LOSS_PERCENT: i32 = 10;
/// Largest Opus packet possible.
const MAX_PACKET_BYTES: usize = 1275;

/// Turns captured wire-rate audio into ready-to-send packets. Buffers samples
/// until a full frame is available, so callers can push any chunk size.
pub struct VoiceEncoder {
    encoder: Encoder,
    pending: Vec<f32>,
    seq: u32,
}

impl VoiceEncoder {
    pub fn new() -> Result<Self> {
        let mut encoder = Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Voip)?;
        encoder.set_bitrate(Bitrate::Bits(BITRATE_BPS))?;
        encoder.set_inband_fec(true)?;
        encoder.set_packet_loss_perc(EXPECTED_LOSS_PERCENT)?;
        Ok(Self { encoder, pending: Vec::with_capacity(FRAME_SAMPLES), seq: 0 })
    }

    /// Returns one packet per completed frame, oldest first.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Vec<u8>> {
        self.pending.extend_from_slice(samples);
        let mut packets = Vec::new();
        let mut consumed = 0;
        while self.pending.len() - consumed >= FRAME_SAMPLES {
            let frame = &self.pending[consumed..consumed + FRAME_SAMPLES];
            consumed += FRAME_SAMPLES;
            let mut packet = vec![0u8; SEQ_HEADER_LEN + MAX_PACKET_BYTES];
            packet[..SEQ_HEADER_LEN].copy_from_slice(&self.seq.to_le_bytes());
            match self.encoder.encode_float(frame, &mut packet[SEQ_HEADER_LEN..]) {
                Ok(len) => {
                    packet.truncate(SEQ_HEADER_LEN + len);
                    packets.push(packet);
                }
                Err(e) => eprintln!("opus encode failed: {e}"),
            }
            // advance even on failure, the receiver conceals the gap
            self.seq = self.seq.wrapping_add(1);
        }
        self.pending.drain(..consumed);
        packets
    }
}

/// Per-peer decoder. Opus is stateful, every sender needs its own.
pub struct VoiceDecoder {
    decoder: Decoder,
}

impl VoiceDecoder {
    pub fn new() -> Result<Self> {
        Ok(Self { decoder: Decoder::new(SAMPLE_RATE, Channels::Mono)? })
    }

    /// Decodes a packet that arrived normally.
    pub fn decode(&mut self, payload: &[u8]) -> Result<Vec<f32>> {
        // a packet may hold up to 120 ms
        let mut out = vec![0.0f32; FRAME_SAMPLES * 6];
        let n = self.decoder.decode_float(payload, &mut out, false)?;
        out.truncate(n);
        Ok(out)
    }

    /// Reconstructs a lost frame from the FEC data carried in the packet that
    /// followed it.
    pub fn decode_fec(&mut self, next_payload: &[u8]) -> Result<Vec<f32>> {
        self.decode_lost(next_payload, true)
    }

    /// Packet loss concealment: plausible continuation when no data is left.
    pub fn conceal(&mut self) -> Result<Vec<f32>> {
        self.decode_lost(&[], false)
    }

    fn decode_lost(&mut self, payload: &[u8], fec: bool) -> Result<Vec<f32>> {
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        let n = self.decoder.decode_float(payload, &mut out, fec)?;
        if n != FRAME_SAMPLES {
            return Err(anyhow!("concealment produced {n} samples"));
        }
        Ok(out)
    }
}
