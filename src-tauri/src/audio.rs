use std::{
    fs::File,
    io::BufWriter,
    path::PathBuf,
    sync::Arc,
};

use anyhow::{anyhow, Context, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, SampleFormat, SampleRate, Stream, StreamConfig,
};
use parking_lot::Mutex;
use uuid::Uuid;

use crate::config;

pub struct ActiveRecording {
    stream: Stream,
    sink: Arc<Mutex<RecordingSink>>,
}

struct RecordingSink {
    path: PathBuf,
    writer: Option<hound::WavWriter<BufWriter<File>>>,
    channels: usize,
    resampler: LinearResampler,
}

struct LinearResampler {
    source_rate: f64,
    target_rate: f64,
    next_output_pos: f64,
    current_input_index: u64,
    previous_sample: Option<f32>,
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
        let sink = Arc::new(Mutex::new(RecordingSink::new(
            config.sample_rate.0,
            config.channels as usize,
        )?));
        let input_buffer = Arc::clone(&sink);
        let err_fn = |error| eprintln!("audio stream error: {error}");

        let stream = match sample_format {
            SampleFormat::F32 => build_stream_f32(&device, &config, input_buffer, err_fn)?,
            SampleFormat::I16 => build_stream_i16(&device, &config, input_buffer, err_fn)?,
            SampleFormat::U16 => build_stream_u16(&device, &config, input_buffer, err_fn)?,
            other => return Err(anyhow!("unsupported sample format: {other:?}")),
        };

        stream.play().context("failed to start input stream")?;

        Ok(Self { stream, sink })
    }

    pub fn stop(self) -> Result<PathBuf> {
        self.stream.pause().ok();

        let mut sink = self.sink.lock();
        sink.finalize()
    }
}

impl RecordingSink {
    fn new(source_rate: u32, channels: usize) -> Result<Self> {
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
        let writer = hound::WavWriter::create(&path, spec)
            .with_context(|| format!("failed to create wav file {}", path.display()))?;

        Ok(Self {
            path,
            writer: Some(writer),
            channels,
            resampler: LinearResampler::new(source_rate),
        })
    }

    fn ingest_frames(&mut self, data: &[f32]) {
        if self.channels == 0 || data.is_empty() {
            return;
        }

        for frame in data.chunks(self.channels) {
            let mono = frame.iter().copied().sum::<f32>() / frame.len() as f32;
            let outputs = self.resampler.push(mono);
            if outputs.is_empty() {
                continue;
            }

            let Some(writer) = self.writer.as_mut() else {
                return;
            };

            for sample in outputs {
                let clamped = sample.clamp(-1.0, 1.0);
                if writer.write_sample((clamped * i16::MAX as f32) as i16).is_err() {
                    return;
                }
            }
        }
    }

    fn finalize(&mut self) -> Result<PathBuf> {
        if !self.resampler.has_output() {
            if let Some(writer) = self.writer.take() {
                let _ = writer.finalize();
            }
            let _ = std::fs::remove_file(&self.path);
            return Err(anyhow!("no audio captured"));
        }

        let writer = self
            .writer
            .take()
            .ok_or_else(|| anyhow!("recording writer already finalized"))?;
        writer.finalize()?;
        Ok(self.path.clone())
    }
}

impl LinearResampler {
    fn new(source_rate: u32) -> Self {
        Self {
            source_rate: source_rate as f64,
            target_rate: 16_000.0,
            next_output_pos: 0.0,
            current_input_index: 0,
            previous_sample: None,
        }
    }

    fn push(&mut self, sample: f32) -> Vec<f32> {
        let mut output = Vec::new();
        match self.previous_sample {
            None => {
                self.previous_sample = Some(sample);
                output.push(sample);
                self.next_output_pos += self.source_rate / self.target_rate;
            }
            Some(previous) => {
                self.current_input_index += 1;
                let current_index = self.current_input_index as f64;
                let previous_index = current_index - 1.0;

                while self.next_output_pos <= current_index {
                    let mix = (self.next_output_pos - previous_index) as f32;
                    output.push(previous + (sample - previous) * mix);
                    self.next_output_pos += self.source_rate / self.target_rate;
                }

                self.previous_sample = Some(sample);
            }
        }

        output
    }

    fn has_output(&self) -> bool {
        self.next_output_pos > 0.0
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
    sink: Arc<Mutex<RecordingSink>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<Stream> {
    Ok(device.build_input_stream(
        config,
        move |data: &[f32], _| {
            sink.lock().ingest_frames(data);
        },
        err_fn,
        None,
    )?)
}

fn build_stream_i16(
    device: &Device,
    config: &StreamConfig,
    sink: Arc<Mutex<RecordingSink>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<Stream> {
    Ok(device.build_input_stream(
        config,
        move |data: &[i16], _| {
            let normalized = data
                .iter()
                .map(|value| *value as f32 / i16::MAX as f32)
                .collect::<Vec<_>>();
            sink.lock().ingest_frames(&normalized);
        },
        err_fn,
        None,
    )?)
}

fn build_stream_u16(
    device: &Device,
    config: &StreamConfig,
    sink: Arc<Mutex<RecordingSink>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<Stream> {
    Ok(device.build_input_stream(
        config,
        move |data: &[u16], _| {
            let normalized = data
                .iter()
                .map(|value| (*value as f32 / u16::MAX as f32) * 2.0 - 1.0)
                .collect::<Vec<_>>();
            sink.lock().ingest_frames(&normalized);
        },
        err_fn,
        None,
    )?)
}
