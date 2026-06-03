use std::sync::Arc;
use parking_lot::Mutex;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

const TARGET_RATE: u32 = 16000;

#[allow(dead_code)]
/// Wrapper to make cpal::Stream Send + Sync.
/// cpal::Stream is !Send on some platforms (ALSA), but in practice
/// we only drop it on the same thread. This is the standard workaround.
struct SendStream(Option<cpal::Stream>);
unsafe impl Send for SendStream {}
unsafe impl Sync for SendStream {}

pub struct Recorder {
    device_rate: u32,
    stream: SendStream,
    buffer: Arc<Mutex<Vec<f32>>>,
    recording: Arc<Mutex<bool>>,
    amplitudes: Arc<Mutex<Vec<f32>>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            device_rate: 0,
            stream: SendStream(None),
            buffer: Arc::new(Mutex::new(Vec::new())),
            recording: Arc::new(Mutex::new(false)),
            amplitudes: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "no input device found".to_string())?;

        let config = device
            .default_input_config()
            .map_err(|e| format!("input config: {}", e))?;

        let device_rate = config.sample_rate().0;
        log::info!("audio device: {} ({} Hz)", device.name().unwrap_or_default(), device_rate);
        self.device_rate = device_rate;

        let buffer = self.buffer.clone();
        let recording = self.recording.clone();
        let amplitudes = self.amplitudes.clone();

        *recording.lock() = true;

        let err_fn = move |err| {
            log::error!("audio stream error: {}", err);
        };

        let stream = device
            .build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if *recording.lock() {
                        let mut buf = buffer.lock();
                        buf.extend_from_slice(data);

                        let chunk = data.len() / 20;
                        if chunk > 0 {
                            let mut amp = amplitudes.lock();
                            amp.clear();
                            for c in data.chunks(chunk) {
                                let rms =
                                    (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32).sqrt();
                                amp.push(rms.min(1.0));
                            }
                        }
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| format!("build stream: {}", e))?;

        stream
            .play()
            .map_err(|e| format!("play stream: {}", e))?;

        self.stream = SendStream(Some(stream));
        Ok(())
    }

    pub fn stop(&mut self) -> Option<Vec<f32>> {
        *self.recording.lock() = false;
        self.stream = SendStream(None);
        let audio = {
            let mut buf = self.buffer.lock();
            if buf.is_empty() {
                None
            } else {
                Some(buf.drain(..).collect())
            }
        };
        *self.amplitudes.lock() = Vec::new();

        // Resample to TARGET_RATE if device rate differs
        audio.map(|samples: Vec<f32>| {
            if self.device_rate != 0 && self.device_rate != TARGET_RATE {
                log::info!("resampling from {} Hz to {} Hz ({} samples)", self.device_rate, TARGET_RATE, samples.len());
                resample(&samples, self.device_rate, TARGET_RATE)
            } else {
                samples
            }
        })
    }

    pub fn is_recording(&self) -> bool {
        *self.recording.lock()
    }

    pub fn get_amplitudes(&self) -> Vec<f32> {
        self.amplitudes.lock().clone()
    }
}

/// Linear interpolation resampling
fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (input.len() as f64 / ratio).ceil() as usize;
    let mut output = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let src_idx = src_pos as usize;
        let frac = src_pos - src_idx as f64;

        if src_idx + 1 < input.len() {
            let a = input[src_idx] as f64;
            let b = input[src_idx + 1] as f64;
            output.push((a + (b - a) * frac) as f32);
        } else if src_idx < input.len() {
            output.push(input[src_idx]);
        }
    }
    output
}

/// Encode f32 audio samples to WAV bytes for API submission
pub fn audio_to_wav(audio: &[f32], sample_rate: u32) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::new(std::io::Cursor::new(&mut buf), spec)
            .map_err(|e| format!("wav writer: {}", e))?;

    for &sample in audio {
        let amp = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
        writer
            .write_sample(amp)
            .map_err(|e| format!("wav sample: {}", e))?;
    }

    writer
        .finalize()
        .map_err(|e| format!("wav finalize: {}", e))?;

    Ok(buf)
}
