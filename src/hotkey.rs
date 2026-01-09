use crossbeam_channel::{Receiver, Sender, bounded};
use evdev::{Device, EventSummary, KeyCode};
use std::os::unix::io::AsRawFd;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HotkeyEvent {
	Pressed,
	Released,
}

pub struct HotkeyListener {
	rx: Receiver<HotkeyEvent>,
	_handle: thread::JoinHandle<()>,
}

impl HotkeyListener {
	pub fn start(key_name: &str) -> Result<Self, String> {
		let target_key = parse_key(key_name)?;
		let (tx, rx) = bounded(16);
		let handle = thread::spawn(move || listener_loop(target_key, tx));
		Ok(Self {
			rx,
			_handle: handle,
		})
	}

	pub fn try_recv(&self) -> Option<HotkeyEvent> {
		self.rx.try_recv().ok()
	}
}

fn find_keyboards() -> Vec<Device> {
	let Ok(entries) = std::fs::read_dir("/dev/input") else {
		return vec![];
	};
	let mut devices = Vec::new();
	for entry in entries.flatten() {
		let path = entry.path();
		let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
			continue;
		};
		if !name.starts_with("event") {
			continue;
		}
		if let Ok(device) = Device::open(&path) {
			let is_keyboard = device
				.supported_keys()
				.map(|k| k.contains(KeyCode::KEY_A) && k.contains(KeyCode::KEY_ENTER))
				.unwrap_or(false);
			if is_keyboard {
				let fd = device.as_raw_fd();
				unsafe {
					let flags = libc::fcntl(fd, libc::F_GETFL);
					if flags != -1 {
						libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
					}
				}
				devices.push(device);
			}
		}
	}
	devices
}

fn listener_loop(target_key: KeyCode, tx: Sender<HotkeyEvent>) {
	let mut devices = find_keyboards();
	let mut is_pressed = false;
	let mut retry_count = 0;

	loop {
		if devices.is_empty() {
			retry_count += 1;
			if retry_count > 10 {
				eprintln!("No keyboards found. Is user in 'input' group?");
				return;
			}
			thread::sleep(Duration::from_secs(1));
			devices = find_keyboards();
			continue;
		}

		let mut stale = Vec::new();
		for (idx, device) in devices.iter_mut().enumerate() {
			match device.fetch_events() {
				Ok(events) => {
					for event in events {
						if let EventSummary::Key(_, key, value) = event.destructure()
							&& key == target_key
						{
							match value {
								1 if !is_pressed => {
									is_pressed = true;
									let _ = tx.send(HotkeyEvent::Pressed);
								}
								0 if is_pressed => {
									is_pressed = false;
									let _ = tx.send(HotkeyEvent::Released);
								}
								_ => {}
							}
						}
					}
				}
				Err(e) if e.raw_os_error() == Some(libc::ENODEV) => {
					stale.push(idx);
				}
				Err(_) => {}
			}
		}

		for idx in stale.into_iter().rev() {
			devices.remove(idx);
		}

		thread::sleep(Duration::from_millis(5));
	}
}

fn parse_key(name: &str) -> Result<KeyCode, String> {
	let upper = name.to_uppercase().replace(['-', ' '], "_");
	let key_name = if upper.starts_with("KEY_") {
		upper
	} else {
		format!("KEY_{}", upper)
	};
	match key_name.as_str() {
		"KEY_SCROLLLOCK" => Ok(KeyCode::KEY_SCROLLLOCK),
		"KEY_PAUSE" => Ok(KeyCode::KEY_PAUSE),
		"KEY_CAPSLOCK" => Ok(KeyCode::KEY_CAPSLOCK),
		"KEY_NUMLOCK" => Ok(KeyCode::KEY_NUMLOCK),
		"KEY_INSERT" => Ok(KeyCode::KEY_INSERT),
		"KEY_F1" => Ok(KeyCode::KEY_F1),
		"KEY_F2" => Ok(KeyCode::KEY_F2),
		"KEY_F3" => Ok(KeyCode::KEY_F3),
		"KEY_F4" => Ok(KeyCode::KEY_F4),
		"KEY_F5" => Ok(KeyCode::KEY_F5),
		"KEY_F6" => Ok(KeyCode::KEY_F6),
		"KEY_F7" => Ok(KeyCode::KEY_F7),
		"KEY_F8" => Ok(KeyCode::KEY_F8),
		"KEY_F9" => Ok(KeyCode::KEY_F9),
		"KEY_F10" => Ok(KeyCode::KEY_F10),
		"KEY_F11" => Ok(KeyCode::KEY_F11),
		"KEY_F12" => Ok(KeyCode::KEY_F12),
		"KEY_F13" => Ok(KeyCode::KEY_F13),
		"KEY_F14" => Ok(KeyCode::KEY_F14),
		"KEY_F15" => Ok(KeyCode::KEY_F15),
		"KEY_F16" => Ok(KeyCode::KEY_F16),
		"KEY_F17" => Ok(KeyCode::KEY_F17),
		"KEY_F18" => Ok(KeyCode::KEY_F18),
		"KEY_F19" => Ok(KeyCode::KEY_F19),
		"KEY_F20" => Ok(KeyCode::KEY_F20),
		"KEY_F21" => Ok(KeyCode::KEY_F21),
		"KEY_F22" => Ok(KeyCode::KEY_F22),
		"KEY_F23" => Ok(KeyCode::KEY_F23),
		"KEY_F24" => Ok(KeyCode::KEY_F24),
		"KEY_HOME" => Ok(KeyCode::KEY_HOME),
		"KEY_END" => Ok(KeyCode::KEY_END),
		"KEY_PAGEUP" => Ok(KeyCode::KEY_PAGEUP),
		"KEY_PAGEDOWN" => Ok(KeyCode::KEY_PAGEDOWN),
		"KEY_DELETE" => Ok(KeyCode::KEY_DELETE),
		"KEY_GRAVE" | "KEY_BACKTICK" => Ok(KeyCode::KEY_GRAVE),
		"KEY_RIGHTALT" | "KEY_RALT" => Ok(KeyCode::KEY_RIGHTALT),
		"KEY_RIGHTCTRL" | "KEY_RCTRL" => Ok(KeyCode::KEY_RIGHTCTRL),
		_ => Err(format!(
			"Unknown key: {}. Try: SCROLLLOCK, PAUSE, F13-F24",
			name
		)),
	}
}
