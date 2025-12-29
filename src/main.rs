mod models;

use adw::prelude::*;
use gtk::glib;
use models::{Model, MODELS};
use std::cell::RefCell;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};

const APP_ID: &str = "io.github.sotto";
const DEFAULT_MODEL: &str = "ggml-base.en.bin";

fn models_dir() -> Option<PathBuf> {
	dirs::data_dir().map(|d| d.join("sotto").join("models"))
}

fn config_path() -> Option<PathBuf> {
	dirs::config_dir().map(|d| d.join("sotto").join("settings.ini"))
}

fn load_settings() -> (String, Option<String>) {
	let Some(path) = config_path() else {
		return (DEFAULT_MODEL.to_string(), None);
	};

	let keyfile = glib::KeyFile::new();
	if keyfile.load_from_file(&path, glib::KeyFileFlags::NONE).is_err() {
		return (DEFAULT_MODEL.to_string(), None);
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

	let device = keyfile
		.string("sotto", "device")
		.ok()
		.map(|s| s.to_string());

	(model, device)
}

fn save_settings(model: &str, device: Option<&str>) {
	let Some(path) = config_path() else { return };

	if let Some(parent) = path.parent() {
		let _ = std::fs::create_dir_all(parent);
	}

	let keyfile = glib::KeyFile::new();
	keyfile.set_string("sotto", "model", model);
	if let Some(dev) = device {
		keyfile.set_string("sotto", "device", dev);
	}

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
		} else if let Some(desc) = line.strip_prefix("\tDescription: ") {
			if !current_name.contains(".monitor") && !desc.starts_with("Monitor of") {
				devices.push(AudioDevice {
					name: current_name.clone(),
					description: desc.to_string(),
				});
			}
		}
	}
	devices
}

struct Recorder {
	process: Child,
	audio_buffer: Arc<Mutex<Vec<u8>>>,
}

impl Recorder {
	fn start(device: Option<&str>) -> Result<Self, String> {
		let mut cmd = Command::new("pw-record");
		cmd.args(["--rate=16000", "--channels=1", "--format=s16"]);
		if let Some(dev) = device {
			cmd.arg(format!("--target={}", dev));
		}
		cmd.arg("-");

		let mut process = cmd
			.stdout(Stdio::piped())
			.stderr(Stdio::null())
			.spawn()
			.map_err(|e| format!("Failed to start pw-record: {}", e))?;

		let audio_buffer = Arc::new(Mutex::new(Vec::new()));
		let buffer_clone = Arc::clone(&audio_buffer);

		let mut stdout = process.stdout.take().ok_or("Failed to capture stdout")?;

		std::thread::spawn(move || {
			let mut buf = [0u8; 4096];
			loop {
				match stdout.read(&mut buf) {
					Ok(0) => break,
					Ok(n) => {
						if let Ok(mut buffer) = buffer_clone.lock() {
							buffer.extend_from_slice(&buf[..n]);
						}
					}
					Err(_) => break,
				}
			}
		});

		Ok(Self { process, audio_buffer })
	}

	fn stop(mut self) -> Vec<u8> {
		unsafe {
			libc::kill(self.process.id() as i32, libc::SIGINT);
		}
		let _ = self.process.wait();
		std::thread::sleep(std::time::Duration::from_millis(100));
		match Arc::try_unwrap(self.audio_buffer) {
			Ok(mutex) => mutex.into_inner().unwrap_or_default(),
			Err(arc) => arc.lock().unwrap().clone(),
		}
	}
}

fn pcm_to_samples(pcm: &[u8]) -> Vec<f32> {
	pcm.chunks_exact(2)
		.map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
		.collect()
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

fn main() -> glib::ExitCode {
	adw::init().expect("Failed to initialize libadwaita");
	let app = adw::Application::builder().application_id(APP_ID).build();
	app.connect_activate(build_ui);
	app.run()
}

enum TranscribeMessage {
	Done(String),
	Error(String),
}

fn build_ui(app: &adw::Application) {
	let (saved_model, saved_device) = load_settings();

	let model_display_name = MODELS
		.iter()
		.find(|m| m.id == saved_model)
		.map(|m| m.name)
		.unwrap_or("Base (EN)");

	let current_model: Rc<RefCell<String>> = Rc::new(RefCell::new(saved_model));
	let current_device: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(saved_device));
	let recorder: Rc<RefCell<Option<Recorder>>> = Rc::new(RefCell::new(None));

	let model_label = gtk::Label::builder()
		.label(model_display_name)
		.css_classes(["heading"])
		.build();

	let settings_button = gtk::Button::builder()
		.icon_name("emblem-system-symbolic")
		.css_classes(["flat"])
		.build();

	let header = adw::HeaderBar::builder()
		.title_widget(&model_label)
		.build();
	header.pack_end(&settings_button);

	let text_view = gtk::TextView::builder()
		.editable(false)
		.cursor_visible(false)
		.wrap_mode(gtk::WrapMode::Word)
		.left_margin(12)
		.right_margin(12)
		.top_margin(12)
		.bottom_margin(12)
		.vexpand(true)
		.build();

	let placeholder_label = gtk::Label::builder()
		.label("Transcribed text will appear here")
		.css_classes(["dim-label"])
		.halign(gtk::Align::Start)
		.valign(gtk::Align::Start)
		.margin_start(12)
		.margin_top(12)
		.build();

	let copy_button = gtk::Button::builder()
		.icon_name("edit-copy-symbolic")
		.css_classes(["flat", "circular"])
		.halign(gtk::Align::End)
		.valign(gtk::Align::End)
		.margin_end(8)
		.margin_bottom(8)
		.visible(false)
		.tooltip_text("Copy to clipboard")
		.build();

	let text_overlay = gtk::Overlay::builder()
		.child(&text_view)
		.build();
	text_overlay.add_overlay(&placeholder_label);
	text_overlay.add_overlay(&copy_button);

	let text_scroll = gtk::ScrolledWindow::builder()
		.hscrollbar_policy(gtk::PolicyType::Never)
		.vscrollbar_policy(gtk::PolicyType::Automatic)
		.child(&text_overlay)
		.vexpand(true)
		.build();

	let text_frame = gtk::Frame::builder()
		.child(&text_scroll)
		.margin_start(12)
		.margin_end(12)
		.margin_top(12)
		.build();

	let status_label = gtk::Label::builder()
		.label("Click to record")
		.css_classes(["dim-label"])
		.margin_top(12)
		.build();

	let record_button = gtk::Button::builder()
		.icon_name("media-record-symbolic")
		.css_classes(["circular", "suggested-action"])
		.width_request(64)
		.height_request(64)
		.halign(gtk::Align::Center)
		.margin_top(12)
		.margin_bottom(24)
		.build();

	let record_area = gtk::Box::builder()
		.orientation(gtk::Orientation::Vertical)
		.halign(gtk::Align::Center)
		.build();
	record_area.append(&status_label);
	record_area.append(&record_button);

	let content = gtk::Box::builder()
		.orientation(gtk::Orientation::Vertical)
		.vexpand(true)
		.build();
	content.append(&text_frame);
	content.append(&record_area);

	let toast_overlay = adw::ToastOverlay::new();
	toast_overlay.set_child(Some(&content));
	toast_overlay.set_vexpand(true);

	let main_box = gtk::Box::builder()
		.orientation(gtk::Orientation::Vertical)
		.build();
	main_box.append(&header);
	main_box.append(&toast_overlay);

	let window = adw::ApplicationWindow::builder()
		.application(app)
		.title("Sotto")
		.default_width(550)
		.default_height(550)
		.resizable(false)
		.content(&main_box)
		.build();

	copy_button.connect_clicked(glib::clone!(
		#[weak]
		text_view,
		#[weak]
		toast_overlay,
		move |_| {
			let buffer = text_view.buffer();
			let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
			if copy_to_clipboard(&text) {
				let toast = adw::Toast::new("Copied to clipboard");
				toast_overlay.add_toast(toast);
			}
		}
	));

	record_button.connect_clicked(glib::clone!(
		#[weak]
		status_label,
		#[weak]
		text_view,
		#[weak]
		placeholder_label,
		#[weak]
		copy_button,
		#[strong]
		recorder,
		#[strong]
		current_model,
		#[strong]
		current_device,
		move |btn| {
			let is_recording = recorder.borrow().is_some();
			if is_recording {
				btn.remove_css_class("destructive-action");
				btn.add_css_class("suggested-action");
				btn.set_icon_name("media-record-symbolic");
				btn.set_sensitive(false);
				status_label.set_label("Transcribing...");

				text_view.buffer().set_text("");
				placeholder_label.set_visible(false);
				copy_button.set_visible(false);

				let rec = recorder.borrow_mut().take().unwrap();
				let model_id = current_model.borrow().clone();
				let (tx, rx) = mpsc::channel::<TranscribeMessage>();

				std::thread::spawn(move || {
					let pcm = rec.stop();
					let samples = pcm_to_samples(&pcm);
					transcribe(&model_id, &samples, tx);
				});

				glib::timeout_add_local(std::time::Duration::from_millis(50), glib::clone!(
					#[weak]
					btn,
					#[weak]
					status_label,
					#[weak]
					text_view,
					#[weak]
					placeholder_label,
					#[weak]
					copy_button,
					#[upgrade_or]
					glib::ControlFlow::Break,
					move || {
						match rx.try_recv() {
							Ok(TranscribeMessage::Done(text)) => {
								if text.is_empty() {
									placeholder_label.set_visible(true);
									status_label.set_label("No speech detected");
								} else {
									text_view.buffer().set_text(&text);
									copy_button.set_visible(true);
									status_label.set_label("Click to record");
								}
								btn.set_sensitive(true);
								glib::ControlFlow::Break
							}
							Ok(TranscribeMessage::Error(e)) => {
								status_label.set_label(&format!("Error: {}", e));
								placeholder_label.set_visible(true);
								btn.set_sensitive(true);
								glib::ControlFlow::Break
							}
							Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
							Err(mpsc::TryRecvError::Disconnected) => {
								status_label.set_label("Transcription failed");
								placeholder_label.set_visible(true);
								btn.set_sensitive(true);
								glib::ControlFlow::Break
							}
						}
					}
				));
			} else {
				let device = current_device.borrow().clone();
				match Recorder::start(device.as_deref()) {
					Ok(rec) => {
						*recorder.borrow_mut() = Some(rec);
						btn.remove_css_class("suggested-action");
						btn.add_css_class("destructive-action");
						btn.set_icon_name("media-playback-stop-symbolic");
						status_label.set_label("Recording...");
					}
					Err(e) => {
						status_label.set_label(&format!("Error: {}", e));
					}
				}
			}
		}
	));

	settings_button.connect_clicked(glib::clone!(
		#[weak]
		window,
		#[strong]
		current_model,
		#[strong]
		current_device,
		#[weak]
		model_label,
		move |_| {
			show_settings(&window, &current_model, &current_device, &model_label);
		}
	));

	window.present();
}

fn setup_downloaded_row(
	row: &adw::ActionRow,
	settings: &adw::PreferencesWindow,
	current_model: &Rc<RefCell<String>>,
	current_device: &Rc<RefCell<Option<String>>>,
	model_label: &gtk::Label,
	model_id: &str,
	model_name: &str,
) {
	let delete_btn = gtk::Button::builder()
		.icon_name("user-trash-symbolic")
		.css_classes(["flat"])
		.valign(gtk::Align::Center)
		.build();

	let model_id = model_id.to_string();
	let model_name = model_name.to_string();

	delete_btn.connect_clicked(glib::clone!(
		#[weak]
		row,
		#[weak]
		settings,
		#[strong]
		current_model,
		#[strong]
		model_id,
		move |btn| {
			if *current_model.borrow() == model_id {
				settings.add_toast(adw::Toast::new("Cannot delete active model"));
				return;
			}
			if delete_model(&model_id).is_ok() {
				row.set_subtitle("Deleted");
				btn.set_visible(false);
			}
		}
	));

	row.add_suffix(&delete_btn);
	row.set_activatable(true);

	row.connect_activated(glib::clone!(
		#[strong]
		current_model,
		#[strong]
		current_device,
		#[weak]
		model_label,
		#[weak]
		settings,
		#[strong]
		model_id,
		#[strong]
		model_name,
		move |_| {
			*current_model.borrow_mut() = model_id.clone();
			model_label.set_label(&model_name);
			save_settings(&model_id, current_device.borrow().as_deref());
			settings.close();
		}
	));
}

fn show_settings(
	parent: &adw::ApplicationWindow,
	current_model: &Rc<RefCell<String>>,
	current_device: &Rc<RefCell<Option<String>>>,
	model_label: &gtk::Label,
) {
	let settings = adw::PreferencesWindow::builder()
		.title("Settings")
		.transient_for(parent)
		.modal(true)
		.build();

	let devices_group = adw::PreferencesGroup::builder()
		.title("Input Device")
		.build();

	let devices = list_input_devices();
	let device_names: Vec<&str> = std::iter::once("Default")
		.chain(devices.iter().map(|d| d.description.as_str()))
		.collect();
	let device_list = gtk::StringList::new(&device_names);

	let device_row = adw::ComboRow::builder()
		.title("Microphone")
		.model(&device_list)
		.use_subtitle(true)
		.build();
	device_row.set_subtitle_lines(2);

	let current_dev = current_device.borrow();
	if let Some(ref dev_name) = *current_dev {
		if let Some(idx) = devices.iter().position(|d| &d.name == dev_name) {
			device_row.set_selected((idx + 1) as u32);
		}
	}
	drop(current_dev);

	device_row.connect_selected_notify(glib::clone!(
		#[strong]
		current_model,
		#[strong]
		current_device,
		#[strong]
		devices,
		move |row| {
			let selected = row.selected() as usize;
			if selected == 0 {
				*current_device.borrow_mut() = None;
			} else if let Some(dev) = devices.get(selected - 1) {
				*current_device.borrow_mut() = Some(dev.name.clone());
			}
			save_settings(&current_model.borrow(), current_device.borrow().as_deref());
		}
	));

	devices_group.add(&device_row);

	let models_group = adw::PreferencesGroup::builder()
		.title("Models")
		.description("Select a model for transcription")
		.build();

	for model in MODELS {
		let row = adw::ActionRow::builder()
			.title(model.name)
			.subtitle(model.size)
			.build();

		let is_current = *current_model.borrow() == model.id;
		let is_downloaded = is_model_downloaded(model.id);

		if is_downloaded {
			if is_current {
				row.add_suffix(&gtk::Image::from_icon_name("object-select-symbolic"));
			}
			setup_downloaded_row(&row, &settings, current_model, current_device, model_label, model.id, model.name);
		} else {
			let download_btn = gtk::Button::builder()
				.icon_name("folder-download-symbolic")
				.css_classes(["flat"])
				.valign(gtk::Align::Center)
				.build();

			let model_clone = model.clone();
			let model_id = model.id.to_string();
			let model_name = model.name.to_string();

			download_btn.connect_clicked(glib::clone!(
				#[weak]
				row,
				#[weak]
				settings,
				#[strong]
				current_model,
				#[strong]
				current_device,
				#[weak]
				model_label,
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

					std::thread::spawn(move || {
						download_model(&model, tx);
					});

					let model_id = model_id.clone();
					let model_name = model_name.clone();

					glib::timeout_add_local(std::time::Duration::from_millis(50), glib::clone!(
						#[weak]
						progress,
						#[weak]
						row,
						#[weak]
						settings,
						#[strong]
						current_model,
						#[strong]
						current_device,
						#[weak]
						model_label,
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
										setup_downloaded_row(&row, &settings, &current_model, &current_device, &model_label, &model_id, &model_name);
										settings.add_toast(adw::Toast::new("Download complete"));
										return glib::ControlFlow::Break;
									}
									Ok(DownloadProgress::Error(e)) => {
										let toast = adw::Toast::new(&format!("Download failed: {}", e));
										settings.add_toast(toast);
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
					));
				}
			));

			row.add_suffix(&download_btn);
		}

		models_group.add(&row);
	}

	let page = adw::PreferencesPage::new();
	page.add(&devices_group);
	page.add(&models_group);
	settings.add(&page);

	settings.present();
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
			let _ = tx.send(DownloadProgress::Error("Cannot find data directory".to_string()));
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
				let _ = tx.send(DownloadProgress::Progress(downloaded as f64 / total_size as f64));
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

fn transcribe(model_id: &str, samples: &[f32], tx: mpsc::Sender<TranscribeMessage>) {
	use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

	const MIN_SAMPLES: usize = 16000 / 2;

	let result: Result<String, String> = (|| {
		if samples.len() < MIN_SAMPLES {
			return Ok(String::new());
		}

		let model_path = models_dir()
			.ok_or("Cannot find data directory")?
			.join(model_id);

		if !model_path.exists() {
			return Err("Model not downloaded".to_string());
		}

		let ctx = WhisperContext::new_with_params(
			model_path.to_str().ok_or("Invalid model path")?,
			WhisperContextParameters::default(),
		)
		.map_err(|e| format!("Failed to load model: {}", e))?;

		let mut state = ctx.create_state().map_err(|e| format!("Failed to create state: {}", e))?;
		let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
		params.set_print_special(false);
		params.set_print_progress(false);
		params.set_print_realtime(false);
		params.set_print_timestamps(false);

		state.full(params, samples).map_err(|e| format!("Transcription failed: {}", e))?;

		let mut text = String::new();
		for i in 0..state.full_n_segments() {
			if let Some(seg) = state.get_segment(i) {
				if let Ok(s) = seg.to_str() {
					text.push_str(s);
				}
			}
		}
		Ok(text.trim().to_string())
	})();

	match result {
		Ok(text) => { let _ = tx.send(TranscribeMessage::Done(text)); }
		Err(e) => { let _ = tx.send(TranscribeMessage::Error(e)); }
	}
}

fn copy_to_clipboard(text: &str) -> bool {
	use std::io::Write;
	if let Ok(mut child) = Command::new("wl-copy").stdin(Stdio::piped()).spawn() {
		if let Some(mut stdin) = child.stdin.take() {
			let _ = stdin.write_all(text.as_bytes());
		}
		return child.wait().is_ok();
	}
	false
}
