//! Codec stand-in without the `audio` feature: such builds (the headless
//! relay) never capture or play call audio, so creating a codec fails and the
//! call code skips encoding/decoding.

use anyhow::{anyhow, Result};

pub const FRAME_SAMPLES: usize = 960;
pub const SEQ_HEADER_LEN: usize = 4;

pub struct VoiceEncoder;

impl VoiceEncoder {
    pub fn new() -> Result<Self> {
        Err(anyhow!("built without the audio feature"))
    }

    pub fn push(&mut self, _samples: &[f32]) -> Vec<Vec<u8>> {
        Vec::new()
    }
}

pub struct VoiceDecoder;

impl VoiceDecoder {
    pub fn new() -> Result<Self> {
        Err(anyhow!("built without the audio feature"))
    }

    pub fn decode(&mut self, _payload: &[u8]) -> Result<Vec<f32>> {
        Ok(Vec::new())
    }

    pub fn decode_fec(&mut self, _next_payload: &[u8]) -> Result<Vec<f32>> {
        Ok(Vec::new())
    }

    pub fn conceal(&mut self) -> Result<Vec<f32>> {
        Ok(Vec::new())
    }
}
