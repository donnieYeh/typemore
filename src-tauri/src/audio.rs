use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, SampleFormat, SampleRate, Stream, StreamConfig,
};
use uuid::Uuid;

use crate::config;

pub struct ActiveRecording {
    stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
}

impl ActiveRecording {
    pub fn start(device_id: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();
        let device = pick_device(&host, device_id)?;
        let supported = device
            .default_input_config()
            .context("failed to read default input config")?;
        let sample_format = supported.sample_format();
        let config = supported.config();

        let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
        let input_buffer = Arc::clone(&samples);
        let err_fn = |error| eprintln!("audio stream error: {error}");

        let stream = match sample_format {
            SampleFormat::F32 => build_stream_f32(&device, &config, input_buffer, err_fn)?,
            SampleFormat::I16 => build_stream_i16(&device, &config, input_buffer, err_fn)?,
            SampleFormat::U16 => build_stream_u16(&device, &config, input_buffer, err_fn)?,
            other => return Err(anyhow!("unsupported sample format: {other:?}")),
        };

        stream.play().context("failed to start input stream")?;

        Ok(Self {
            stream,
            samples,
            sample_rate: config.sample_rate.0,
            channels: config.channels,
        })
    }

    pub fn stop(self) -> Result<PathBuf> {
        self.stream.pause().ok();

        let samples = self
            .samples
            .lock()
            .map_err(|_| anyhow!("recording buffer poisoned"))?
            .clone();
        if samples.is_empty() {
            return Err(anyhow!("no audio captured"));
        }

        let mono = downmix_to_mono(&samples, self.channels as usize);
        let resampled = resample_to_16k(&mono, self.sample_rate);
        write_wav(resampled)
    }
}

fn pick_device(host: &cpal::Host, wanted_id: Option<&str>) -> Result<Device> {
    if let Some(id) = wanted_id {
        let mut devices = host.input_devices().context("failed to enumerate microphones")?;
        if let Some(device) = devices.find(|device| device.name().map(|name| name == id).unwrap_or(false)) {
            return Ok(device);
        }
    }

    host.default_input_device()
        .context("no default input microphone available")
}

fn build_stream_f32(
    device: &Device,
    config: &StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<Stream> {
    Ok(device.build_input_stream(
        config,
        move |data: &[f32], _| {
            if let Ok(mut buffer) = samples.lock() {
                buffer.extend_from_slice(data);
            }
        },
        err_fn,
        None,
    )?)
}

fn build_stream_i16(
    device: &Device,
    config: &StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<Stream> {
    Ok(device.build_input_stream(
        config,
        move |data: &[i16], _| {
            if let Ok(mut buffer) = samples.lock() {
                buffer.extend(data.iter().map(|value| *value as f32 / i16::MAX as f32));
            }
        },
        err_fn,
        None,
    )?)
}

fn build_stream_u16(
    device: &Device,
    config: &StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<Stream> {
    Ok(device.build_input_stream(
        config,
        move |data: &[u16], _| {
            if let Ok(mut buffer) = samples.lock() {
                buffer.extend(data.iter().map(|value| (*value as f32 / u16::MAX as f32) * 2.0 - 1.0));
            }
        },
        err_fn,
        None,
    )?)
}

fn downmix_to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }

    samples
        .chunks(channels)
        .map(|chunk| chunk.iter().copied().sum::<f32>() / channels as f32)
        .collect()
}

fn resample_to_16k(samples: &[f32], source_rate: u32) -> Vec<f32> {
    if source_rate == 16_000 {
        return samples.to_vec();
    }

    let ratio = 16_000.0 / source_rate as f32;
    let target_len = (samples.len() as f32 * ratio).round() as usize;
    let mut output = Vec::with_capacity(target_len);

    for index in 0..target_len {
        let source_position = index as f32 / ratio;
        let left = source_position.floor() as usize;
        let right = (left + 1).min(samples.len().saturating_sub(1));
        let frac = source_position - left as f32;
        let left_value = *samples.get(left).unwrap_or(&0.0);
        let right_value = *samples.get(right).unwrap_or(&left_value);
        output.push(left_value + (right_value - left_value) * frac);
    }

    output
}

fn write_wav(samples: Vec<f32>) -> Result<PathBuf> {
    let temp_dir = config::temp_dir()?;
    std::fs::create_dir_all(&temp_dir)
        .with_context(|| format!("failed to create temp dir {}", temp_dir.display()))?;

    let path = temp_dir.join(format!("recording-{}.wav", Uuid::new_v4()));
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SampleRate(16_000).0,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(&path, spec)
        .with_context(|| format!("failed to create wav file {}", path.display()))?;
    for sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        writer.write_sample((clamped * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;

    Ok(path)
}
