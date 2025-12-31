mod audio;
mod models;
mod signal_mode;
mod transcription;

use gtk::glib;
use models::{MODELS, Model};
use signal_mode::{
    RecordingSession, notify_recording_started, notify_recording_stopped, notify_transcribing,
    paste_text, start_signal_listener,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::{Command, Stdio};
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

fn load_settings() -> (String, Option<String>, String) {
    let Some(path) = config_path() else {
        return (DEFAULT_MODEL.to_string(), None, "en".to_string());
    };

    let keyfile = glib::KeyFile::new();
    if keyfile
        .load_from_file(&path, glib::KeyFileFlags::NONE)
        .is_err()
    {
        return (DEFAULT_MODEL.to_string(), None, "en".to_string());
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

    let language = keyfile
        .string("sotto", "language")
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "en".to_string());

    (model, device, language)
}

fn save_settings(model: &str, device: Option<&str>, language: &str) {
    let Some(path) = config_path() else { return };

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let keyfile = glib::KeyFile::new();
    keyfile.set_string("sotto", "model", model);
    if let Some(dev) = device {
        keyfile.set_string("sotto", "device", dev);
    }
    keyfile.set_string("sotto", "language", language);

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
    println!("  daemon    Run as background daemon (signal mode only)");
    println!("  enable    Enable systemd user service");
    println!("  disable   Disable systemd user service");
    println!("  help      Show this help");
}

fn run_gui() {
    use adw::prelude::*;

    adw::init().expect("Failed to initialize libadwaita");
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn run_daemon() {
    let (model, device, mut language) = load_settings();

    if model.contains(".en") {
        language = "en".to_string();
    }

    let model_path = match models_dir() {
        Some(d) => d.join(&model),
        None => {
            eprintln!("Cannot find data directory");
            std::process::exit(1);
        }
    };

    if !model_path.exists() {
        eprintln!("Model not downloaded: {}", model);
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

    println!("Sotto daemon started (language: {})", language);
    println!("Send SIGUSR1 to toggle recording (pkill -USR1 sotto)");

    let signal_rx = start_signal_listener();
    let mut session: Option<RecordingSession> = None;

    loop {
        match signal_rx.try_recv() {
            Ok(_) => {
                if let Some(s) = session.take() {
                    notify_recording_stopped();
                    let samples = s.stop();
                    if !samples.is_empty() {
                        notify_transcribing();
                        worker.transcribe(samples, &language);
                    }
                } else {
                    match RecordingSession::start(device.clone()) {
                        Ok(s) => {
                            session = Some(s);
                            notify_recording_started();
                        }
                        Err(e) => eprintln!("Failed to start recording: {}", e),
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => break,
        }

        while let Some(text) = worker.try_recv_result() {
            if !text.is_empty() {
                paste_text(&text);
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(20));
    }
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
         After=graphical-session.target\n\
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

fn build_ui(app: &adw::Application) {
    use adw::prelude::*;

    let (saved_model, saved_device, saved_language) = load_settings();

    let model_display_name = MODELS
        .iter()
        .find(|m| m.id == saved_model)
        .map(|m| m.name)
        .unwrap_or("Base (EN)");

    let current_model: Rc<RefCell<String>> = Rc::new(RefCell::new(saved_model));
    let current_device: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(saved_device));
    let current_language: Rc<RefCell<String>> = Rc::new(RefCell::new(saved_language));
    let gui_session: Rc<RefCell<Option<RecordingSession>>> = Rc::new(RefCell::new(None));
    let gui_worker: Rc<RefCell<Option<TranscriptionWorker>>> = Rc::new(RefCell::new(None));
    let worker_model: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    let model_label = gtk::Label::builder()
        .label(model_display_name)
        .css_classes(["heading"])
        .build();

    let settings_button = gtk::Button::builder()
        .icon_name("emblem-system-symbolic")
        .css_classes(["flat"])
        .build();

    let languages = [
        "en", "auto", "pl", "de", "fr", "es", "it", "pt", "nl", "ja", "zh", "ko", "ru",
    ];
    let lang_list = gtk::StringList::new(&languages);
    let lang_dropdown = gtk::DropDown::builder().model(&lang_list).build();

    let current_lang = current_language.borrow();
    if let Some(idx) = languages.iter().position(|&l| l == *current_lang) {
        lang_dropdown.set_selected(idx as u32);
    }
    let is_en_model = current_model.borrow().contains(".en");
    lang_dropdown.set_sensitive(!is_en_model);
    drop(current_lang);

    lang_dropdown.connect_selected_notify(glib::clone!(
        #[strong]
        current_model,
        #[strong]
        current_device,
        #[strong]
        current_language,
        move |dropdown| {
            if let Some(item) = dropdown.selected_item()
                && let Some(string_obj) = item.downcast_ref::<gtk::StringObject>()
            {
                let lang = string_obj.string().to_string();
                *current_language.borrow_mut() = lang.clone();
                save_settings(
                    &current_model.borrow(),
                    current_device.borrow().as_deref(),
                    &lang,
                );
            }
        }
    ));

    let header = adw::HeaderBar::builder().title_widget(&model_label).build();
    header.pack_start(&lang_dropdown);
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

    let text_overlay = gtk::Overlay::builder().child(&text_view).build();
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
        gui_session,
        #[strong]
        gui_worker,
        #[strong]
        worker_model,
        #[strong]
        current_model,
        #[strong]
        current_device,
        #[strong]
        current_language,
        move |btn| {
            let is_recording = gui_session.borrow().is_some();
            if is_recording {
                if let Some(session) = gui_session.borrow_mut().take() {
                    let samples = session.stop();
                    if !samples.is_empty()
                        && let Some(ref worker) = *gui_worker.borrow()
                    {
                        let mut lang = current_language.borrow().clone();
                        if current_model.borrow().contains(".en") {
                            lang = "en".to_string();
                        }
                        worker.transcribe(samples, &lang);
                        status_label.set_label("Transcribing...");
                    } else {
                        status_label.set_label("Click to record");
                        placeholder_label.set_visible(true);
                    }
                }
                btn.remove_css_class("destructive-action");
                btn.add_css_class("suggested-action");
                btn.set_icon_name("media-record-symbolic");
            } else {
                let model_id = current_model.borrow().clone();
                let device = current_device.borrow().clone();

                let model_path = match models_dir() {
                    Some(d) => d.join(&model_id),
                    None => {
                        status_label.set_label("Error: Cannot find data directory");
                        return;
                    }
                };

                if !model_path.exists() {
                    status_label.set_label("Error: Model not downloaded");
                    return;
                }

                let needs_reload = worker_model
                    .borrow()
                    .as_ref()
                    .map(|m| m != &model_id)
                    .unwrap_or(true);

                if needs_reload {
                    status_label.set_label("Loading model...");
                    match TranscriptionWorker::start(model_path) {
                        Ok(w) => {
                            *gui_worker.borrow_mut() = Some(w);
                            *worker_model.borrow_mut() = Some(model_id);
                        }
                        Err(e) => {
                            status_label.set_label(&format!("Error: {}", e));
                            return;
                        }
                    }
                }

                match RecordingSession::start(device) {
                    Ok(session) => {
                        *gui_session.borrow_mut() = Some(session);
                        text_view.buffer().set_text("");
                        placeholder_label.set_visible(false);
                        copy_button.set_visible(false);
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
        #[strong]
        current_language,
        #[weak]
        model_label,
        #[weak]
        lang_dropdown,
        move |_| {
            show_settings(
                &window,
                &current_model,
                &current_device,
                &current_language,
                &model_label,
                &lang_dropdown,
            );
        }
    ));

    glib::timeout_add_local(
        std::time::Duration::from_millis(50),
        glib::clone!(
            #[strong]
            gui_worker,
            #[weak]
            text_view,
            #[weak]
            copy_button,
            #[weak]
            status_label,
            #[weak]
            placeholder_label,
            #[upgrade_or]
            glib::ControlFlow::Break,
            move || {
                if let Some(ref worker) = *gui_worker.borrow() {
                    while let Some(text) = worker.try_recv_result() {
                        status_label.set_label("Click to record");
                        if !text.is_empty() {
                            let buffer = text_view.buffer();
                            buffer.set_text(&text);
                            copy_button.set_visible(true);
                        } else {
                            placeholder_label.set_visible(true);
                        }
                    }
                }
                glib::ControlFlow::Continue
            }
        ),
    );

    window.present();
}

#[allow(clippy::too_many_arguments)]
fn setup_downloaded_row(
    row: &adw::ActionRow,
    settings: &adw::PreferencesWindow,
    current_model: &Rc<RefCell<String>>,
    current_device: &Rc<RefCell<Option<String>>>,
    current_language: &Rc<RefCell<String>>,
    model_label: &gtk::Label,
    lang_dropdown: &gtk::DropDown,
    model_id: &str,
    model_name: &str,
) {
    use adw::prelude::*;

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
        #[strong]
        current_language,
        #[weak]
        model_label,
        #[weak]
        lang_dropdown,
        #[weak]
        settings,
        #[strong]
        model_id,
        #[strong]
        model_name,
        move |_| {
            *current_model.borrow_mut() = model_id.clone();
            model_label.set_label(&model_name);
            lang_dropdown.set_sensitive(!model_id.contains(".en"));
            save_settings(
                &model_id,
                current_device.borrow().as_deref(),
                &current_language.borrow(),
            );
            settings.close();
        }
    ));
}

fn show_settings(
    parent: &adw::ApplicationWindow,
    current_model: &Rc<RefCell<String>>,
    current_device: &Rc<RefCell<Option<String>>>,
    current_language: &Rc<RefCell<String>>,
    model_label: &gtk::Label,
    lang_dropdown: &gtk::DropDown,
) {
    use adw::prelude::*;

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
    if let Some(ref dev_name) = *current_dev
        && let Some(idx) = devices.iter().position(|d| &d.name == dev_name)
    {
        device_row.set_selected((idx + 1) as u32);
    }
    drop(current_dev);

    device_row.connect_selected_notify(glib::clone!(
        #[strong]
        current_model,
        #[strong]
        current_device,
        #[strong]
        current_language,
        #[strong]
        devices,
        move |row| {
            let selected = row.selected() as usize;
            if selected == 0 {
                *current_device.borrow_mut() = None;
            } else if let Some(dev) = devices.get(selected - 1) {
                *current_device.borrow_mut() = Some(dev.name.clone());
            }
            save_settings(
                &current_model.borrow(),
                current_device.borrow().as_deref(),
                &current_language.borrow(),
            );
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
            setup_downloaded_row(
                &row,
                &settings,
                current_model,
                current_device,
                current_language,
                model_label,
                lang_dropdown,
                model.id,
                model.name,
            );
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
                #[strong]
                current_language,
                #[weak]
                model_label,
                #[weak]
                lang_dropdown,
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

                    glib::timeout_add_local(
                        std::time::Duration::from_millis(50),
                        glib::clone!(
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
                            #[strong]
                            current_language,
                            #[weak]
                            model_label,
                            #[weak]
                            lang_dropdown,
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
                                            setup_downloaded_row(
                                                &row,
                                                &settings,
                                                &current_model,
                                                &current_device,
                                                &current_language,
                                                &model_label,
                                                &lang_dropdown,
                                                &model_id,
                                                &model_name,
                                            );
                                            settings
                                                .add_toast(adw::Toast::new("Download complete"));
                                            return glib::ControlFlow::Break;
                                        }
                                        Ok(DownloadProgress::Error(e)) => {
                                            let toast =
                                                adw::Toast::new(&format!("Download failed: {}", e));
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
                        ),
                    );
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
