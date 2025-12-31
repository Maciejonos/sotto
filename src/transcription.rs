use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Utterance {
    pub samples: Vec<f32>,
    pub language: String,
}

pub struct TranscriptionWorker {
    handle: Option<JoinHandle<()>>,
    utterance_tx: Sender<Utterance>,
    result_rx: Receiver<String>,
    stop_tx: Sender<()>,
}

impl TranscriptionWorker {
    pub fn start(model_path: PathBuf) -> Result<Self, String> {
        let model_path_str = model_path
            .to_str()
            .ok_or("Model path contains invalid UTF-8")?
            .to_string();

        let (utterance_tx, utterance_rx) = mpsc::channel::<Utterance>();
        let (result_tx, result_rx) = mpsc::channel::<String>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();

        let handle = thread::spawn(move || {
            let ctx = match WhisperContext::new_with_params(
                &model_path_str,
                WhisperContextParameters::default(),
            ) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to load model: {}", e);
                    return;
                }
            };

            let mut state = match ctx.create_state() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to create state: {}", e);
                    return;
                }
            };

            loop {
                match utterance_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(utterance) => {
                        if utterance.samples.is_empty() {
                            let _ = result_tx.send(String::new());
                            continue;
                        }

                        let lang = if utterance.language == "auto" {
                            None
                        } else {
                            Some(utterance.language.as_str())
                        };

                        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
                        params.set_language(lang);
                        params.set_print_special(false);
                        params.set_print_progress(false);
                        params.set_print_realtime(false);
                        params.set_print_timestamps(false);

                        if let Err(e) = state.full(params, &utterance.samples) {
                            eprintln!("Transcription failed: {}", e);
                            let _ = result_tx.send(String::new());
                            continue;
                        }

                        let mut text = String::new();
                        for i in 0..state.full_n_segments() {
                            if let Some(seg) = state.get_segment(i)
                                && let Ok(s) = seg.to_str()
                            {
                                text.push_str(s);
                            }
                        }

                        let _ = result_tx.send(text.trim().to_string());
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if stop_rx.try_recv().is_ok() {
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        Ok(Self {
            handle: Some(handle),
            utterance_tx,
            result_rx,
            stop_tx,
        })
    }

    pub fn transcribe(&self, samples: Vec<f32>, language: &str) {
        let _ = self.utterance_tx.send(Utterance {
            samples,
            language: language.to_string(),
        });
    }

    pub fn try_recv_result(&self) -> Option<String> {
        match self.result_rx.try_recv() {
            Ok(text) => Some(text),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => None,
        }
    }
}

impl Drop for TranscriptionWorker {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
