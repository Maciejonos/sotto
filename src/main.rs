mod audio;
#[cfg(unix)]
mod hotkey;
mod indicator;
mod models;
mod signal_mode;
mod text;
mod transcription;

use gtk::glib;
use gtk::prelude::*;
use indicator::{Indicator, IndicatorState};
use models::{MODELS, Model};
use signal_mode::{RecordingSession, paste_text, start_signal_listener};
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;
use transcription::TranscriptionWorker;

const APP_ID: &str = "io.github.sotto";
const DEFAULT_MODEL: &str = "ggml-base.en.bin";

fn models_dir() -> Option<PathBuf> {
	dirs::data_dir().map(|d| d.join("sotto").join("models"))
}

fn config_path() -> Option<PathBuf> {
	dirs::config_dir().map(|d| d.join("sotto").join("settings.ini"))
}

struct Settings {
	model: String,
	device: Option<String>,
	language: String,
	mode: String,
	hotkey: String,
}

fn load_settings() -> Settings {
	let defaults = Settings {
		model: DEFAULT_MODEL.to_string(),
		device: None,
		language: "en".to_string(),
		mode: "signal".to_string(),
		hotkey: "INSERT".to_string(),
	};

	let Some(path) = config_path() else {
		return defaults;
	};

	let keyfile = glib::KeyFile::new();
	if keyfile
		.load_from_file(&path, glib::KeyFileFlags::NONE)
		.is_err()
	{
		return defaults;
	}

	let model = keyfile
		.string("sotto", "model")
		.map(|s| s.to_string())
		.unwrap_or_else(|_| DEFAULT_MODEL.to_string());

	let model = if is_model_downloaded(&model) {
		model
	} else {
		DEFAULT_MODEL.to_string()
	};

	Settings {
		model,
		device: keyfile
			.string("sotto", "device")
			.ok()
			.map(|s| s.to_string()),
		language: keyfile
			.string("sotto", "language")
			.map(|s| s.to_string())
			.unwrap_or_else(|_| "en".to_string()),
		mode: keyfile
			.string("sotto", "mode")
			.map(|s| s.to_string())
			.unwrap_or_else(|_| "signal".to_string()),
		hotkey: keyfile
			.string("sotto", "hotkey")
			.map(|s| s.to_string())
			.unwrap_or_else(|_| "INSERT".to_string()),
	}
}

fn save_settings(settings: &Settings) {
	let Some(path) = config_path() else { return };
	if let Some(parent) = path.parent() {
		let _ = std::fs::create_dir_all(parent);
	}
	let keyfile = glib::KeyFile::new();
	keyfile.set_string("sotto", "model", &settings.model);
	if let Some(dev) = &settings.device {
		keyfile.set_string("sotto", "device", dev);
	}
	keyfile.set_string("sotto", "language", &settings.language);
	keyfile.set_string("sotto", "mode", &settings.mode);
	keyfile.set_string("sotto", "hotkey", &settings.hotkey);
	let _ = keyfile.save_to_file(&path);
}

#[derive(Clone)]
struct AudioDevice {
	name: String,
	description: String,
}

fn list_input_devices() -> Vec<AudioDevice> {
	let output = Command::new("pactl")
		.args(["list", "sources"])
		.output()
		.ok();

	let Some(output) = output else { return vec![] };
	let text = String::from_utf8_lossy(&output.stdout);

	let mut devices = Vec::new();
	let mut current_name = String::new();

	for line in text.lines() {
		if let Some(name) = line.strip_prefix("\tName: ") {
			current_name = name.to_string();
		} else if let Some(desc) = line.strip_prefix("\tDescription: ")
			&& !current_name.contains(".monitor")
			&& !desc.starts_with("Monitor of")
		{
			devices.push(AudioDevice {
				name: current_name.clone(),
				description: desc.to_string(),
			});
		}
	}
	devices
}

fn is_model_downloaded(model_id: &str) -> bool {
	models_dir()
		.map(|d| d.join(model_id).exists())
		.unwrap_or(false)
}

fn delete_model(model_id: &str) -> Result<(), std::io::Error> {
	if let Some(path) = models_dir().map(|d| d.join(model_id)) {
		std::fs::remove_file(path)?;
	}
	Ok(())
}

fn main() {
	let args: Vec<String> = std::env::args().collect();
	let cmd = args.get(1).map(|s| s.as_str());

	match cmd {
		Some("daemon") => run_daemon(),
		Some("enable") => enable_systemd(),
		Some("disable") => disable_systemd(),
		Some("help") | Some("--help") | Some("-h") => print_help(),
		Some(other) => {
			eprintln!("Unknown command: {}", other);
			print_help();
			std::process::exit(1);
		}
		None => run_gui(),
	}
}

fn print_help() {
	println!("Usage: sotto [command]");
	println!();
	println!("Commands:");
	println!("  (none)    Launch GUI");
	println!("  daemon    Run as background daemon");
	println!("  enable    Enable systemd user service");
	println!("  disable   Disable systemd user service");
	println!("  help      Show this help");
	println!();
	println!("Daemon modes (set in ~/.config/sotto/settings.ini):");
	println!("  mode=signal   Toggle via compositor keybinding (pkill -USR1 sotto)");
	println!("  mode=evdev    Push-to-talk via hotkey (requires input group)");
	println!("  hotkey=KEY    Key for evdev mode (default: INSERT)");
}

fn run_gui() {
	use adw::prelude::*;

	adw::init().expect("Failed to initialize libadwaita");
	let app = adw::Application::builder().application_id(APP_ID).build();
	app.connect_activate(build_ui);
	app.run();
}

fn run_daemon() {
	let settings = load_settings();
	let mut language = settings.language.clone();

	if settings.model.contains(".en") {
		language = "en".to_string();
	}

	let model_path = match models_dir() {
		Some(d) => d.join(&settings.model),
		None => {
			eprintln!("Cannot find data directory");
			std::process::exit(1);
		}
	};

	if !model_path.exists() {
		eprintln!("Model not downloaded: {}", settings.model);
		eprintln!("Run sotto (GUI) first to download a model");
		std::process::exit(1);
	}

	println!("Loading model...");
	let worker = match TranscriptionWorker::start(model_path) {
		Ok(w) => w,
		Err(e) => {
			eprintln!("Failed to load model: {}", e);
			std::process::exit(1);
		}
	};

	let use_evdev = settings.mode == "evdev";
	if use_evdev {
		println!(
			"Sotto daemon started (mode: push-to-talk, key: {}, language: {})",
			settings.hotkey, language
		);
	} else {
		println!(
			"Sotto daemon started (mode: signal, language: {})",
			language
		);
		println!("Send SIGUSR1 to toggle recording (pkill -USR1 sotto)");
	}

	gtk::init().expect("Failed to initialize GTK");
	let app = gtk::Application::builder()
		.application_id("io.github.sotto.daemon")
		.build();

	let worker = Rc::new(worker);
	let worker_clone = worker.clone();
	let device = settings.device.clone();
	let hotkey = settings.hotkey.clone();

	app.connect_activate(move |app| {
		let indicator = Rc::new(Indicator::new(app));
		let session: Rc<RefCell<Option<RecordingSession>>> = Rc::new(RefCell::new(None));
		let device = device.clone();
		let language = language.clone();
		let worker = worker_clone.clone();
		let indicator_clone = indicator.clone();

		#[cfg(unix)]
		let hotkey_listener = if use_evdev {
			match hotkey::HotkeyListener::start(&hotkey) {
				Ok(l) => Some(l),
				Err(e) => {
					eprintln!("Failed to start hotkey listener: {}", e);
					std::process::exit(1);
				}
			}
		} else {
			None
		};

		let signal_rx = if use_evdev {
			None
		} else {
			Some(start_signal_listener())
		};

		glib::timeout_add_local(std::time::Duration::from_millis(20), move || {
			if let Some(ref rx) = signal_rx {
				match rx.try_recv() {
					Ok(_) => {
						if let Some(s) = session.borrow_mut().take() {
							indicator_clone.show(IndicatorState::Transcribing);
							let samples = s.stop();
							if !samples.is_empty() {
								worker.transcribe(samples, &language);
							} else {
								indicator_clone.hide();
							}
						} else {
							match RecordingSession::start(device.clone()) {
								Ok(s) => {
									*session.borrow_mut() = Some(s);
									indicator_clone.show(IndicatorState::Recording);
								}
								Err(e) => eprintln!("Failed to start recording: {}", e),
							}
						}
					}
					Err(mpsc::TryRecvError::Empty) => {}
					Err(mpsc::TryRecvError::Disconnected) => return glib::ControlFlow::Break,
				}
			}

			#[cfg(unix)]
			if let Some(ref listener) = hotkey_listener {
				while let Some(event) = listener.try_recv() {
					match event {
						hotkey::HotkeyEvent::Pressed => {
							if session.borrow().is_none() {
								match RecordingSession::start(device.clone()) {
									Ok(s) => {
										*session.borrow_mut() = Some(s);
										indicator_clone.show(IndicatorState::Recording);
									}
									Err(e) => eprintln!("Failed to start recording: {}", e),
								}
							}
						}
						hotkey::HotkeyEvent::Released => {
							if let Some(s) = session.borrow_mut().take() {
								indicator_clone.show(IndicatorState::Transcribing);
								let samples = s.stop();
								if !samples.is_empty() {
									worker.transcribe(samples, &language);
								} else {
									indicator_clone.hide();
								}
							}
						}
					}
				}
			}

			while let Some(result) = worker.try_recv_result() {
				indicator_clone.hide();
				if !result.is_empty() {
					paste_text(&text::process_punctuation(&result));
				}
			}

			glib::ControlFlow::Continue
		});
	});

	app.run_with_args::<&str>(&[]);
}

fn systemd_dir() -> Option<PathBuf> {
	dirs::config_dir().map(|d| d.join("systemd").join("user"))
}

fn enable_systemd() {
	let Some(dir) = systemd_dir() else {
		eprintln!("Cannot find systemd user directory");
		std::process::exit(1);
	};

	if let Err(e) = std::fs::create_dir_all(&dir) {
		eprintln!("Failed to create directory: {}", e);
		std::process::exit(1);
	}

	let service_path = dir.join("sotto.service");
	let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("sotto"));

	let unit = format!(
		"[Unit]\n\
         Description=Sotto speech-to-text daemon\n\
         After=graphical-session.target pipewire.service\n\
         Wants=pipewire.service\n\
         PartOf=graphical-session.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={} daemon\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         \n\
         [Install]\n\
         WantedBy=graphical-session.target\n",
		exe_path.display()
	);

	if let Err(e) = std::fs::write(&service_path, unit) {
		eprintln!("Failed to write service file: {}", e);
		std::process::exit(1);
	}

	let status = Command::new("systemctl")
		.args(["--user", "daemon-reload"])
		.status();

	if status.is_err() || !status.unwrap().success() {
		eprintln!("Failed to reload systemd");
		std::process::exit(1);
	}

	let status = Command::new("systemctl")
		.args(["--user", "enable", "--now", "sotto"])
		.status();

	if status.is_err() || !status.unwrap().success() {
		eprintln!("Failed to enable service");
		std::process::exit(1);
	}

	println!("Sotto service enabled and started");
}

fn disable_systemd() {
	let status = Command::new("systemctl")
		.args(["--user", "disable", "--now", "sotto"])
		.status();

	if status.is_err() || !status.unwrap().success() {
		eprintln!("Failed to disable service");
		std::process::exit(1);
	}

	if let Some(dir) = systemd_dir() {
		let _ = std::fs::remove_file(dir.join("sotto.service"));
	}

	let _ = Command::new("systemctl")
		.args(["--user", "daemon-reload"])
		.status();

	println!("Sotto service disabled");
}

fn get_daemon_status() -> (bool, bool) {
	let enabled = Command::new("systemctl")
		.args(["--user", "is-enabled", "sotto"])
		.output()
		.map(|o| o.status.success())
		.unwrap_or(false);
	let active = Command::new("systemctl")
		.args(["--user", "is-active", "sotto"])
		.output()
		.map(|o| o.status.success())
		.unwrap_or(false);
	(enabled, active)
}

fn downloaded_models() -> Vec<&'static Model> {
	MODELS
		.iter()
		.filter(|m| is_model_downloaded(m.id))
		.collect()
}

fn build_ui(app: &adw::Application) {
	use adw::prelude::*;

	let saved = load_settings();
	let current_settings: Rc<RefCell<Settings>> = Rc::new(RefCell::new(saved));

	let header_icon = gtk::Image::builder()
		.icon_name("audio-input-microphone-symbolic")
		.css_classes(["accent"])
		.build();
	let header_title = gtk::Label::builder()
		.label("Sotto")
		.css_classes(["heading"])
		.build();
	let header_box = gtk::Box::builder()
		.orientation(gtk::Orientation::Horizontal)
		.spacing(8)
		.build();
	header_box.append(&header_icon);
	header_box.append(&header_title);

	let header = adw::HeaderBar::builder().title_widget(&header_box).build();

	let toast_overlay = adw::ToastOverlay::new();

	let settings_group = adw::PreferencesGroup::builder()
		.title("Transcription")
		.build();

	let models = downloaded_models();
	let model_names: Vec<&str> = models.iter().map(|m| m.name).collect();
	let model_list = gtk::StringList::new(&model_names);
	let model_row = adw::ComboRow::builder()
		.title("Model")
		.model(&model_list)
		.build();
	model_row.add_prefix(&gtk::Image::from_icon_name("application-x-addon-symbolic"));
	let model_idx = models
		.iter()
		.position(|m| m.id == current_settings.borrow().model)
		.unwrap_or(0);
	model_row.set_selected(model_idx as u32);

	let devices = list_input_devices();
	let device_names: Vec<&str> = std::iter::once("Default")
		.chain(devices.iter().map(|d| d.description.as_str()))
		.collect();
	let device_list = gtk::StringList::new(&device_names);
	let device_row = adw::ComboRow::builder()
		.title("Input Device")
		.model(&device_list)
		.build();
	device_row.add_prefix(&gtk::Image::from_icon_name(
		"audio-input-microphone-symbolic",
	));
	let device_idx = current_settings
		.borrow()
		.device
		.as_ref()
		.and_then(|name| devices.iter().position(|d| &d.name == name))
		.map(|i| i + 1)
		.unwrap_or(0);
	device_row.set_selected(device_idx as u32);
	device_row.set_subtitle(device_names[device_idx]);

	let languages = [
		("auto", "Auto"),
		("de", "German"),
		("en", "English"),
		("es", "Spanish"),
		("fr", "French"),
		("it", "Italian"),
		("ja", "Japanese"),
		("ko", "Korean"),
		("nl", "Dutch"),
		("pl", "Polish"),
		("pt", "Portuguese"),
		("ru", "Russian"),
		("zh", "Chinese"),
	];
	let lang_names: Vec<&str> = languages.iter().map(|(_, name)| *name).collect();
	let lang_list = gtk::StringList::new(&lang_names);
	let lang_row = adw::ComboRow::builder()
		.title("Language")
		.subtitle("Auto is slower and less accurate")
		.model(&lang_list)
		.build();
	lang_row.add_prefix(&gtk::Image::from_icon_name(
		"preferences-desktop-locale-symbolic",
	));
	let lang_idx = languages
		.iter()
		.position(|(code, _)| *code == current_settings.borrow().language)
		.unwrap_or(2);
	lang_row.set_selected(lang_idx as u32);
	let is_en_model = current_settings.borrow().model.contains(".en");
	lang_row.set_sensitive(!is_en_model);

	settings_group.add(&model_row);
	settings_group.add(&device_row);
	settings_group.add(&lang_row);

	let models_row = adw::ActionRow::builder()
		.title("Manage Models")
		.subtitle("Download or remove Whisper models")
		.activatable(true)
		.build();
	models_row.add_prefix(&gtk::Image::from_icon_name("folder-download-symbolic"));
	models_row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
	settings_group.add(&models_row);

	let daemon_group = adw::PreferencesGroup::builder()
		.title("Background Service")
		.description("Runs in background, triggered by keyboard shortcut")
		.build();

	let (enabled, _active) = get_daemon_status();
	let daemon_switch = gtk::Switch::builder()
		.valign(gtk::Align::Center)
		.active(enabled)
		.build();
	let daemon_row = adw::ActionRow::builder()
		.title("Enable Daemon")
		.subtitle("Start automatically with your session")
		.activatable_widget(&daemon_switch)
		.build();
	daemon_row.add_prefix(&gtk::Image::from_icon_name("system-run-symbolic"));
	daemon_row.add_suffix(&daemon_switch);
	daemon_group.add(&daemon_row);

	let mode_list = gtk::StringList::new(&["Toggle (compositor)", "Push-to-talk (hotkey)"]);
	let mode_row = adw::ComboRow::builder()
		.title("Activation Mode")
		.model(&mode_list)
		.build();
	mode_row.add_prefix(&gtk::Image::from_icon_name("input-keyboard-symbolic"));
	let mode_idx = if current_settings.borrow().mode == "evdev" {
		1
	} else {
		0
	};
	mode_row.set_selected(mode_idx);
	daemon_group.add(&mode_row);

	let hotkeys = [
		"INSERT",
		"SCROLLLOCK",
		"PAUSE",
		"F13",
		"F14",
		"F15",
		"F16",
		"RIGHTALT",
		"Custom...",
	];
	let hotkey_list = gtk::StringList::new(&hotkeys);
	let hotkey_row = adw::ComboRow::builder()
		.title("Hotkey")
		.subtitle("Requires user in 'input' group")
		.model(&hotkey_list)
		.build();
	hotkey_row.add_prefix(&gtk::Image::from_icon_name(
		"preferences-desktop-keyboard-shortcuts-symbolic",
	));
	let cur_hotkey = current_settings.borrow().hotkey.to_uppercase();
	let hotkey_idx = hotkeys[..8]
		.iter()
		.position(|&k| k == cur_hotkey)
		.unwrap_or(8);
	hotkey_row.set_selected(hotkey_idx as u32);
	hotkey_row.set_sensitive(mode_idx == 1);
	daemon_group.add(&hotkey_row);

	let custom_entry = gtk::Entry::builder()
		.placeholder_text("e.g. F20, PAUSE, HOME")
		.valign(gtk::Align::Center)
		.build();
	if hotkey_idx == 8 {
		custom_entry.set_text(&cur_hotkey);
	}
	let custom_hotkey_row = adw::ActionRow::builder()
		.title("Custom Key")
		.subtitle("Enter evdev key name")
		.build();
	custom_hotkey_row.add_suffix(&custom_entry);
	custom_hotkey_row.set_visible(hotkey_idx == 8);
	custom_hotkey_row.set_sensitive(mode_idx == 1);
	daemon_group.add(&custom_hotkey_row);

	let help_row = adw::ActionRow::builder()
		.title("Setup Instructions")
		.subtitle("Configure your compositor keybinding")
		.activatable(true)
		.build();
	help_row.add_prefix(&gtk::Image::from_icon_name("dialog-information-symbolic"));
	help_row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
	daemon_group.add(&help_row);

	let about_group = adw::PreferencesGroup::new();
	let github_row = adw::ActionRow::builder()
		.title("Source Code")
		.subtitle("github.com/Maciejonos/sotto")
		.activatable(true)
		.build();
	github_row.add_prefix(&gtk::Image::from_icon_name("web-browser-symbolic"));
	github_row.add_suffix(&gtk::Image::from_icon_name("external-link-symbolic"));
	about_group.add(&github_row);

	let page = adw::PreferencesPage::new();
	page.add(&settings_group);
	page.add(&daemon_group);
	page.add(&about_group);

	let clamp = adw::Clamp::builder()
		.child(&page)
		.maximum_size(600)
		.margin_start(16)
		.margin_end(16)
		.build();
	toast_overlay.set_child(Some(&clamp));

	let main_box = gtk::Box::builder()
		.orientation(gtk::Orientation::Vertical)
		.build();
	main_box.append(&header);
	main_box.append(&toast_overlay);

	let window = adw::ApplicationWindow::builder()
		.application(app)
		.title("Sotto")
		.default_width(520)
		.default_height(580)
		.resizable(false)
		.content(&main_box)
		.build();

	model_row.connect_selected_notify(glib::clone!(
		#[strong]
		current_settings,
		#[weak]
		lang_row,
		move |row| {
			let models = downloaded_models();
			if let Some(model) = models.get(row.selected() as usize) {
				current_settings.borrow_mut().model = model.id.to_string();
				lang_row.set_sensitive(!model.id.contains(".en"));
				save_settings(&current_settings.borrow());
			}
		}
	));

	device_row.connect_selected_notify(glib::clone!(
		#[strong]
		current_settings,
		#[strong]
		devices,
		move |row| {
			let selected = row.selected() as usize;
			if selected == 0 {
				current_settings.borrow_mut().device = None;
				row.set_subtitle("Default");
			} else if let Some(dev) = devices.get(selected - 1) {
				current_settings.borrow_mut().device = Some(dev.name.clone());
				row.set_subtitle(&dev.description);
			}
			save_settings(&current_settings.borrow());
		}
	));

	let lang_codes = [
		"auto", "de", "en", "es", "fr", "it", "ja", "ko", "nl", "pl", "pt", "ru", "zh",
	];
	lang_row.connect_selected_notify(glib::clone!(
		#[strong]
		current_settings,
		move |row| {
			let idx = row.selected() as usize;
			if let Some(code) = lang_codes.get(idx) {
				current_settings.borrow_mut().language = code.to_string();
				save_settings(&current_settings.borrow());
			}
		}
	));

	models_row.connect_activated(glib::clone!(
		#[weak]
		window,
		#[strong]
		current_settings,
		#[weak]
		model_row,
		move |_| {
			show_model_manager(&window, &current_settings, &model_row);
		}
	));

	daemon_switch.connect_active_notify(glib::clone!(
		#[weak]
		toast_overlay,
		move |sw| {
			let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("sotto"));
			if sw.is_active() {
				match Command::new(&exe).arg("enable").status() {
					Ok(s) if s.success() => {
						toast_overlay.add_toast(adw::Toast::new("Daemon enabled"));
					}
					_ => {
						sw.set_active(false);
						toast_overlay.add_toast(adw::Toast::new("Failed to enable daemon"));
					}
				}
			} else {
				match Command::new(&exe).arg("disable").status() {
					Ok(s) if s.success() => {
						toast_overlay.add_toast(adw::Toast::new("Daemon disabled"));
					}
					_ => {
						sw.set_active(true);
						toast_overlay.add_toast(adw::Toast::new("Failed to disable daemon"));
					}
				}
			}
		}
	));

	mode_row.connect_selected_notify(glib::clone!(
		#[strong]
		current_settings,
		#[weak]
		hotkey_row,
		#[weak]
		custom_hotkey_row,
		move |row| {
			let is_evdev = row.selected() == 1;
			current_settings.borrow_mut().mode = if is_evdev {
				"evdev".to_string()
			} else {
				"signal".to_string()
			};
			hotkey_row.set_sensitive(is_evdev);
			custom_hotkey_row.set_sensitive(is_evdev);
			save_settings(&current_settings.borrow());
		}
	));

	let hotkey_names = [
		"INSERT",
		"SCROLLLOCK",
		"PAUSE",
		"F13",
		"F14",
		"F15",
		"F16",
		"RIGHTALT",
	];
	hotkey_row.connect_selected_notify(glib::clone!(
		#[strong]
		current_settings,
		#[weak]
		custom_hotkey_row,
		move |row| {
			let idx = row.selected() as usize;
			let is_custom = idx == 8;
			custom_hotkey_row.set_visible(is_custom);
			if let Some(key) = hotkey_names.get(idx) {
				current_settings.borrow_mut().hotkey = key.to_string();
				save_settings(&current_settings.borrow());
			}
		}
	));

	custom_entry.connect_changed(glib::clone!(
		#[strong]
		current_settings,
		move |entry| {
			let key = entry.text().to_uppercase();
			if !key.is_empty() {
				current_settings.borrow_mut().hotkey = key.to_string();
				save_settings(&current_settings.borrow());
			}
		}
	));

	help_row.connect_activated(glib::clone!(
		#[weak]
		window,
		move |_| {
			show_setup_help(&window);
		}
	));

	github_row.connect_activated(|_| {
		let _ = Command::new("xdg-open")
			.arg("https://github.com/Maciejonos/sotto")
			.spawn();
	});

	window.present();
}

fn show_model_manager(
	parent: &adw::ApplicationWindow,
	current_settings: &Rc<RefCell<Settings>>,
	model_row: &adw::ComboRow,
) {
	use adw::prelude::*;

	let manager = adw::Window::builder()
		.title("Model Manager")
		.transient_for(parent)
		.modal(true)
		.default_width(400)
		.default_height(500)
		.build();

	let header = adw::HeaderBar::new();
	let toast_overlay = adw::ToastOverlay::new();

	let models_group = adw::PreferencesGroup::builder()
		.title("Available Models")
		.description("Download models for transcription")
		.build();

	for model in MODELS {
		let row = adw::ActionRow::builder()
			.title(model.name)
			.subtitle(model.size)
			.build();

		let is_current = current_settings.borrow().model == model.id;
		let is_downloaded = is_model_downloaded(model.id);

		if is_downloaded {
			if is_current {
				row.add_suffix(&gtk::Image::from_icon_name("object-select-symbolic"));
			}
			let delete_btn = gtk::Button::builder()
				.icon_name("user-trash-symbolic")
				.css_classes(["flat"])
				.valign(gtk::Align::Center)
				.build();
			let model_id = model.id.to_string();
			delete_btn.connect_clicked(glib::clone!(
				#[weak]
				row,
				#[weak]
				toast_overlay,
				#[weak]
				model_row,
				#[strong]
				current_settings,
				#[strong]
				model_id,
				move |btn| {
					if current_settings.borrow().model == model_id {
						toast_overlay.add_toast(adw::Toast::new("Cannot delete active model"));
						return;
					}
					if delete_model(&model_id).is_ok() {
						row.set_subtitle("Deleted");
						btn.set_visible(false);
						let models = downloaded_models();
						let model_names: Vec<&str> = models.iter().map(|m| m.name).collect();
						model_row.set_model(Some(&gtk::StringList::new(&model_names)));
						let idx = models
							.iter()
							.position(|m| m.id == current_settings.borrow().model)
							.unwrap_or(0);
						model_row.set_selected(idx as u32);
					}
				}
			));
			row.add_suffix(&delete_btn);
		} else {
			let download_btn = gtk::Button::builder()
				.icon_name("folder-download-symbolic")
				.css_classes(["flat"])
				.valign(gtk::Align::Center)
				.build();
			let model_clone = model.clone();
			download_btn.connect_clicked(glib::clone!(
				#[weak]
				row,
				#[weak]
				toast_overlay,
				#[weak]
				model_row,
				#[strong]
				current_settings,
				move |btn| {
					btn.set_sensitive(false);
					let progress = gtk::ProgressBar::builder()
						.valign(gtk::Align::Center)
						.width_request(100)
						.build();
					row.add_suffix(&progress);
					btn.set_visible(false);

					let model = model_clone.clone();
					let (tx, rx) = mpsc::channel::<DownloadProgress>();
					std::thread::spawn(move || download_model(&model, tx));

					glib::timeout_add_local(
						std::time::Duration::from_millis(50),
						glib::clone!(
							#[weak]
							progress,
							#[weak]
							row,
							#[weak]
							toast_overlay,
							#[weak]
							model_row,
							#[strong]
							current_settings,
							#[upgrade_or]
							glib::ControlFlow::Break,
							move || {
								loop {
									match rx.try_recv() {
										Ok(DownloadProgress::Progress(p)) => {
											progress.set_fraction(p);
										}
										Ok(DownloadProgress::Done) => {
											progress.set_visible(false);
											row.add_suffix(&gtk::Image::from_icon_name(
												"emblem-ok-symbolic",
											));
											toast_overlay
												.add_toast(adw::Toast::new("Download complete"));
											let models = downloaded_models();
											let model_names: Vec<&str> =
												models.iter().map(|m| m.name).collect();
											model_row.set_model(Some(&gtk::StringList::new(
												&model_names,
											)));
											let idx = models
												.iter()
												.position(|m| {
													m.id == current_settings.borrow().model
												})
												.unwrap_or(0);
											model_row.set_selected(idx as u32);
											return glib::ControlFlow::Break;
										}
										Ok(DownloadProgress::Error(e)) => {
											toast_overlay.add_toast(adw::Toast::new(&format!(
												"Download failed: {}",
												e
											)));
											return glib::ControlFlow::Break;
										}
										Err(mpsc::TryRecvError::Empty) => {
											return glib::ControlFlow::Continue;
										}
										Err(mpsc::TryRecvError::Disconnected) => {
											return glib::ControlFlow::Break;
										}
									}
								}
							}
						),
					);
				}
			));
			row.add_suffix(&download_btn);
		}
		models_group.add(&row);
	}

	let page = adw::PreferencesPage::new();
	page.add(&models_group);

	let clamp = adw::Clamp::builder().child(&page).maximum_size(500).build();
	toast_overlay.set_child(Some(&clamp));

	let content = gtk::Box::builder()
		.orientation(gtk::Orientation::Vertical)
		.build();
	content.append(&header);
	content.append(&toast_overlay);
	manager.set_content(Some(&content));

	manager.present();
}

fn show_setup_help(parent: &adw::ApplicationWindow) {
	use adw::prelude::*;

	let dialog = adw::Window::builder()
		.title("Setup Instructions")
		.transient_for(parent)
		.modal(true)
		.default_width(420)
		.default_height(620)
		.build();

	let header = adw::HeaderBar::new();

	let content = gtk::Box::builder()
		.orientation(gtk::Orientation::Vertical)
		.margin_start(24)
		.margin_end(24)
		.margin_top(16)
		.margin_bottom(24)
		.spacing(16)
		.build();

	let steps = [
		("1", "Enable the daemon using the toggle"),
		("2", "Add a keybinding in your compositor"),
		("3", "Press the keybinding to start/stop recording"),
		("4", "Text will be automatically pasted at cursor"),
	];

	for (num, text) in steps {
		let row = gtk::Box::builder()
			.orientation(gtk::Orientation::Horizontal)
			.spacing(12)
			.build();
		let badge = gtk::Label::builder()
			.label(num)
			.css_classes(["accent", "heading"])
			.build();
		let label = gtk::Label::builder()
			.label(text)
			.halign(gtk::Align::Start)
			.wrap(true)
			.build();
		row.append(&badge);
		row.append(&label);
		content.append(&row);
	}

	let code_group = adw::PreferencesGroup::builder()
		.title("Compositor Keybindings")
		.margin_top(8)
		.build();

	let hypr_row = adw::ActionRow::builder()
		.title("Hyprland")
		.subtitle("bind = $mod, V, exec, pkill -USR1 sotto")
		.build();
	let niri_row = adw::ActionRow::builder()
		.title("Niri")
		.subtitle("Mod+V { spawn \"pkill\" \"-USR1\" \"sotto\"; }")
		.build();
	code_group.add(&hypr_row);
	code_group.add(&niri_row);
	content.append(&code_group);

	let ptt_group = adw::PreferencesGroup::builder()
		.title("Push-to-talk Mode")
		.description("Alternative: hold a key to record, release to transcribe")
		.margin_top(8)
		.build();

	let ptt_row1 = adw::ActionRow::builder()
		.title("1. Add user to input group")
		.subtitle("sudo usermod -aG input $USER")
		.build();
	let ptt_row2 = adw::ActionRow::builder()
		.title("2. Log out and back in")
		.subtitle("Required for group changes to take effect")
		.build();
	let ptt_row3 = adw::ActionRow::builder()
		.title("3. Select Push-to-talk mode")
		.subtitle("Change Activation Mode in settings above")
		.build();
	ptt_group.add(&ptt_row1);
	ptt_group.add(&ptt_row2);
	ptt_group.add(&ptt_row3);
	content.append(&ptt_group);

	let main_box = gtk::Box::builder()
		.orientation(gtk::Orientation::Vertical)
		.build();
	main_box.append(&header);
	main_box.append(&content);
	dialog.set_content(Some(&main_box));

	dialog.present();
}

enum DownloadProgress {
	Progress(f64),
	Done,
	Error(String),
}

fn download_model(model: &Model, tx: mpsc::Sender<DownloadProgress>) {
	use std::io::{Read, Write};

	let dir = match models_dir() {
		Some(d) => d,
		None => {
			let _ = tx.send(DownloadProgress::Error(
				"Cannot find data directory".to_string(),
			));
			return;
		}
	};

	if let Err(e) = std::fs::create_dir_all(&dir) {
		let _ = tx.send(DownloadProgress::Error(e.to_string()));
		return;
	}

	let path = dir.join(model.id);
	let temp_path = dir.join(format!("{}.part", model.id));

	let result: Result<(), String> = (|| {
		let response = ureq::get(model.url)
			.header("Accept-Encoding", "identity")
			.call()
			.map_err(|e| e.to_string())?;

		let total_size: u64 = response
			.headers()
			.get("content-length")
			.and_then(|v| v.to_str().ok())
			.and_then(|v| v.parse().ok())
			.unwrap_or(0);

		let mut file = std::fs::File::create(&temp_path).map_err(|e| e.to_string())?;

		let mut reader = response.into_body().into_reader();
		let mut buf = [0u8; 65536];
		let mut downloaded: u64 = 0;

		loop {
			let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
			if n == 0 {
				break;
			}
			file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
			downloaded += n as u64;
			if total_size > 0 {
				let _ = tx.send(DownloadProgress::Progress(
					downloaded as f64 / total_size as f64,
				));
			}
		}

		std::fs::rename(&temp_path, &path).map_err(|e| e.to_string())?;
		Ok(())
	})();

	match result {
		Ok(_) => {
			let _ = tx.send(DownloadProgress::Progress(1.0));
			let _ = tx.send(DownloadProgress::Done);
		}
		Err(e) => {
			let _ = std::fs::remove_file(&temp_path);
			let _ = tx.send(DownloadProgress::Error(e));
		}
	}
}
