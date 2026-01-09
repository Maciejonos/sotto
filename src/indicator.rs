use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

const MARGIN: i32 = 48;

#[derive(Clone, Copy, PartialEq)]
pub enum IndicatorState {
	Hidden,
	Recording,
	Transcribing,
}

pub struct Indicator {
	window: gtk::Window,
	icon: gtk::Image,
	label: gtk::Label,
	state: Rc<Cell<IndicatorState>>,
	start_time: Rc<Cell<Option<Instant>>>,
}

impl Indicator {
	pub fn new(app: &gtk::Application) -> Self {
		let window = gtk::Window::builder()
			.application(app)
			.decorated(false)
			.resizable(false)
			.build();

		window.init_layer_shell();
		window.set_layer(Layer::Overlay);
		window.set_anchor(Edge::Top, true);
		window.set_anchor(Edge::Right, true);
		window.set_margin(Edge::Top, MARGIN);
		window.set_margin(Edge::Right, MARGIN);
		window.set_exclusive_zone(-1);
		window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);

		let container = gtk::Box::builder()
			.orientation(gtk::Orientation::Horizontal)
			.spacing(8)
			.margin_start(12)
			.margin_end(12)
			.margin_top(8)
			.margin_bottom(8)
			.build();

		let icon = gtk::Image::builder()
			.icon_name("audio-input-microphone-symbolic")
			.pixel_size(18)
			.build();

		let label = gtk::Label::builder().label("0:00").build();

		container.append(&icon);
		container.append(&label);
		window.set_child(Some(&container));

		let css = gtk::CssProvider::new();
		css.load_from_data(
			"window { background: alpha(@window_bg_color, 0.9); border-radius: 8px; }
			 image { color: @error_color; }
			 label { font-weight: bold; font-size: 14px; }
			 window.transcribing image { color: @accent_color; }
			 window.transcribing label { color: @accent_color; }",
		);
		gtk::style_context_add_provider_for_display(
			&gtk::gdk::Display::default().unwrap(),
			&css,
			gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
		);

		let state = Rc::new(Cell::new(IndicatorState::Hidden));
		let start_time: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));

		let this = Self {
			window,
			icon,
			label,
			state,
			start_time,
		};

		this.start_timer();
		this
	}

	fn start_timer(&self) {
		let state = self.state.clone();
		let start_time = self.start_time.clone();
		let label = self.label.clone();
		let icon = self.icon.clone();
		let pulse_phase: Rc<Cell<f64>> = Rc::new(Cell::new(0.0));

		glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
			let current_state = state.get();
			if current_state == IndicatorState::Hidden {
				return glib::ControlFlow::Continue;
			}

			if current_state == IndicatorState::Recording
				&& let Some(start) = start_time.get()
			{
				let elapsed = start.elapsed().as_secs();
				let mins = elapsed / 60;
				let secs = elapsed % 60;
				label.set_text(&format!("{}:{:02}", mins, secs));
			}

			let p = pulse_phase.get();
			pulse_phase.set((p + 0.025) % 1.0);
			let opacity = 0.7 + 0.3 * (p * std::f64::consts::TAU).sin();
			icon.set_opacity(opacity);

			glib::ControlFlow::Continue
		});
	}

	pub fn show(&self, new_state: IndicatorState) {
		self.state.set(new_state);
		self.window.remove_css_class("transcribing");

		match new_state {
			IndicatorState::Recording => {
				self.start_time.set(Some(Instant::now()));
				self.label.set_text("0:00");
				self.window.present();
			}
			IndicatorState::Transcribing => {
				self.window.add_css_class("transcribing");
				self.label.set_text("...");
				self.window.present();
			}
			IndicatorState::Hidden => {
				self.window.set_visible(false);
			}
		}
	}

	pub fn hide(&self) {
		self.state.set(IndicatorState::Hidden);
		self.start_time.set(None);
		self.window.set_visible(false);
	}
}
