use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::StreamConfig;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use std::sync::mpsc::{channel, Receiver, Sender};

pub const VOICE_ALPN: &[u8] = b"blabber/voice/0";
const MAX_DATAGRAM_SIZE: usize = 1200;
const SAMPLES_PER_PACKET: usize = MAX_DATAGRAM_SIZE / 4;

pub struct VoiceChannel {
    connection: Connection,
}

impl VoiceChannel {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    ///Captures microphone input & sends it to the remote peer (as QUIC datagrams over the iroh connection)
    pub fn start_capture(&self) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| anyhow!("no input device available"))?;
        let supported_config = device.default_input_config().context("no supported input config")?;
        let stream_config: StreamConfig = supported_config.into();

        let connection = self.connection.clone();
        let stream = device.build_input_stream(&stream_config, move |data: &[f32], _: &cpal::InputCallbackInfo| {send_audio_chunk(&connection, data);},
                                               move |err| { eprintln!("audio capture error: {err}");},
                                               None,)?;
        stream.play().context("failed to start capture stream")?;
        Ok(stream)
    }

    ///Receives datagrams over the iroh connection and plays audio over speakers.
    pub fn start_playback(&self, handle: &tokio::runtime::Handle) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| anyhow!("no output device available"))?;
        let config = device.default_output_config().context("no default output config")?;
        let stream_config: StreamConfig = config.into();

        let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = channel();
        let connection = self.connection.clone();
        handle.spawn(async move {
            loop {
                match connection.read_datagram().await {
                    Ok(bytes) => {
                        if let Some(samples) = bytes_to_samples(&bytes) {
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

pub struct ActiveVoiceCall {
    stop_tx: std::sync::mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<Result<()>>>,
}
impl ActiveVoiceCall {
    pub fn start(channel: VoiceChannel, handle: tokio::runtime::Handle) -> Self {
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

        let thread = std::thread::spawn(move || -> Result<()> {
            let _capture = channel.start_capture()?;
            let _playback = channel.start_playback(&handle)?;
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
pub type IncomingCallHandler = std::sync::Arc<dyn Fn(String, tokio::sync::oneshot::Sender<bool>) + Send + Sync>;
#[derive(Clone, Default)]
pub struct VoiceProtocol {
    on_incoming: Option<IncomingCallHandler>,
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
        Self { on_incoming: None }
    }
    pub fn with_incoming_handler(mut self, handler: IncomingCallHandler) -> Self {
        self.on_incoming = Some(handler);
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

        connection.closed().await;
        call.hang_up();

        Ok(())
    }
}

fn send_audio_chunk(connection: &Connection, data: &[f32]) {
    for sub_chunk in data.chunks(SAMPLES_PER_PACKET) {
        let raw: Vec<u8> = sub_chunk.iter().flat_map(|s| s.to_le_bytes()).collect();

        if raw.len() > MAX_DATAGRAM_SIZE {
            eprintln!("dropping oversized audio packet ({} bytes)", raw.len());
            continue;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use iroh::endpoint::presets;
    use iroh::Endpoint;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn loopback_test() -> Result<()> {
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

        let connection_a = endpoint_a
            .connect(addr_b, VOICE_ALPN)
            .await
            .context("A failed to connect to B")?;

        let connection_b = accept_task.await.context("accept task failed")??;

        let channel_a = VoiceChannel::new(connection_a);
        let channel_b = VoiceChannel::new(connection_b);

        let _capture = channel_a.start_capture()?;
        let handle = tokio::runtime::Handle::current();
        let _playback = channel_b.start_playback(&handle)?;
        println!("Speak into the mic (10 seconds)");
        sleep(Duration::from_secs(10)).await;
        Ok(())
    }
}