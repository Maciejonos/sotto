use crate::audio::AudioRecorder;
use std::process::Command;
use std::sync::mpsc;
use std::thread;

#[cfg(unix)]
use signal_hook::consts::SIGUSR1;
#[cfg(unix)]
use signal_hook::iterator::Signals;

pub enum SignalCommand {
	Toggle,
}

#[cfg(unix)]
pub fn start_signal_listener() -> mpsc::Receiver<SignalCommand> {
	let (tx, rx) = mpsc::channel();

	thread::spawn(move || {
		if let Ok(mut signals) = Signals::new([SIGUSR1]) {
			for _ in signals.forever() {
				let _ = tx.send(SignalCommand::Toggle);
			}
		}
	});

	rx
}

#[cfg(not(unix))]
pub fn start_signal_listener() -> mpsc::Receiver<SignalCommand> {
	let (_tx, rx) = mpsc::channel();
	rx
}

pub fn paste_text(text: &str) -> bool {
	if text.is_empty() {
		return true;
	}

	Command::new("wtype")
		.arg("--")
		.arg(text)
		.status()
		.map(|s| s.success())
		.unwrap_or(false)
}

pub struct RecordingSession {
	audio: AudioRecorder,
}

impl RecordingSession {
	pub fn start(device_name: Option<String>) -> Result<Self, String> {
		let audio = AudioRecorder::start(device_name.as_deref())?;
		Ok(Self { audio })
	}

	pub fn stop(self) -> Vec<f32> {
		self.audio.stop()
	}
}
