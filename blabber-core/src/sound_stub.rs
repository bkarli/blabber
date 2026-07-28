
use anyhow::Result;

/// One entry in the device list returned to the UI.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
}

/// A single contributor to the output mix.
pub trait MixSource: Send {
    fn mix_into(&mut self, out: &mut [f32]);
    fn is_finished(&self) -> bool {
        false
    }
}

pub(crate) type CaptureListener = Box<dyn FnMut(&[f32]) + Send + 'static>;

/// No mix exists without the `audio` feature, so there is nothing to hold
/// onto or remove on drop.
pub struct VoiceHandle;

/// Audio-free stand-in: never opens a device, always succeeds as a no-op.
pub struct SoundHandler;

impl SoundHandler {
    pub fn new() -> Self {
        Self
    }

    pub fn list_input_devices(&self) -> Result<Vec<AudioDeviceInfo>> {
        Ok(Vec::new())
    }

    pub fn list_output_devices(&self) -> Result<Vec<AudioDeviceInfo>> {
        Ok(Vec::new())
    }

    pub fn input_device(&self) -> Option<String> {
        None
    }

    pub fn output_device(&self) -> Option<String> {
        None
    }

    pub fn set_input_device(&self, _name: Option<String>) -> Result<()> {
        Ok(())
    }

    pub fn set_output_device(&self, _name: Option<String>) -> Result<()> {
        Ok(())
    }

    pub fn set_muted(&self, _muted: bool) {}

    pub(crate) fn set_capture_listener(&self, _listener: Option<CaptureListener>) -> Result<()> {
        Ok(())
    }

    pub fn register_call_voice(&self, _source: impl MixSource + 'static) -> Result<VoiceHandle> {
        Ok(VoiceHandle)
    }

    pub fn play_sound_effect(&self, _name: &str, _load_bytes: impl FnOnce() -> Result<Vec<u8>>) -> Result<()> {
        Ok(())
    }
}

impl Default for SoundHandler {
    fn default() -> Self {
        Self::new()
    }
}
