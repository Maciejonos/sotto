use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use voice_activity_detector::VoiceActivityDetector;

const SAMPLE_RATE: u32 = 16000;
const CHANNELS: u16 = 1;
const CHUNK_SIZE: usize = 512;
const SPEECH_THRESHOLD: f32 = 0.5;

pub struct AudioRecorder {
    _stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    vad_handle: Option<JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
}

impl AudioRecorder {
    pub fn start(device_name: Option<&str>) -> Result<Self, String> {
        let host = cpal::default_host();

        let device = match device_name {
            Some(name) => host
                .input_devices()
                .map_err(|e| format!("Failed to enumerate devices: {}", e))?
                .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                .ok_or_else(|| format!("Device '{}' not found", name))?,
            None => host
                .default_input_device()
                .ok_or("No default input device")?,
        };

        let config = cpal::StreamConfig {
            channels: CHANNELS,
            sample_rate: cpal::SampleRate(SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Fixed(CHUNK_SIZE as u32),
        };

        let (tx, rx) = crossbeam_channel::bounded::<Vec<f32>>(100);
        let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let stop_flag = Arc::new(AtomicBool::new(false));

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let _ = tx.try_send(data.to_vec());
                },
                |err| eprintln!("Audio error: {}", err),
                None,
            )
            .map_err(|e| format!("Failed to build input stream: {}", e))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start stream: {}", e))?;

        let samples_clone = Arc::clone(&samples);
        let stop_clone = Arc::clone(&stop_flag);

        let vad_handle = thread::spawn(move || {
            let mut vad = match VoiceActivityDetector::builder()
                .sample_rate(SAMPLE_RATE)
                .chunk_size(CHUNK_SIZE)
                .build()
            {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Failed to create VAD: {}", e);
                    return;
                }
            };

            let mut pending: Vec<f32> = Vec::new();

            while !stop_clone.load(Ordering::Relaxed) {
                match rx.recv_timeout(std::time::Duration::from_millis(50)) {
                    Ok(chunk) => {
                        pending.extend_from_slice(&chunk);

                        while pending.len() >= CHUNK_SIZE {
                            let frame: Vec<f32> = pending.drain(..CHUNK_SIZE).collect();
                            let prob = vad.predict(frame.clone());

                            if prob > SPEECH_THRESHOLD
                                && let Ok(mut buf) = samples_clone.lock()
                            {
                                buf.extend_from_slice(&frame);
                            }
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        Ok(Self {
            _stream: stream,
            samples,
            vad_handle: Some(vad_handle),
            stop_flag,
        })
    }

    pub fn stop(mut self) -> Vec<f32> {
        self.stop_flag.store(true, Ordering::Relaxed);

        if let Some(handle) = self.vad_handle.take() {
            let _ = handle.join();
        }

        match Arc::try_unwrap(self.samples) {
            Ok(mutex) => mutex.into_inner().unwrap_or_default(),
            Err(arc) => arc.lock().map(|g| g.clone()).unwrap_or_default(),
        }
    }
}
