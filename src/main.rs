use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use gdk::EventMask;
use gio::prelude::*;
use gpio_cdev::{AsyncLineEventHandle, Chip, EventRequestFlags, LineRequestFlags};
use gtk::prelude::*;

use futures::StreamExt;
use rodio::Source;
use serde::{Deserialize, Serialize};
use tokio::time::{Instant, Sleep};

struct Timer {
    start_time: Option<Instant>,
    offset: Duration,
}
impl Timer {
    pub fn new() -> Self {
        return Timer {
            start_time: None,
            offset: Duration::new(0, 0),
        };
    }
    pub fn is_running(&self) -> bool {
        return self.start_time.is_some();
    }
    pub fn start(&mut self) {
        if self.start_time.is_some() {
            return;
        }
        self.start_time = Some(Instant::now());
    }
    pub fn stop(&mut self) {
        let elapsed = match self.start_time {
            Some(start_time) => start_time.elapsed(),
            None => return,
        };
        self.offset += elapsed;
        self.start_time = None;
    }
    pub fn clear(&mut self) {
        self.start_time = None;
        self.offset = Duration::from_secs(0);
    }
    pub fn get_duration(&self) -> Duration {
        return match self.start_time {
            Some(start_time) => self.offset + start_time.elapsed(),
            None => self.offset,
        };
    }
}

struct AudioController {
    output_stream: rodio::OutputStream,
    running_player: Option<rodio::Sink>,
}
impl AudioController {
    pub fn new() -> Self {
        return Self {
            output_stream: rodio::OutputStreamBuilder::open_default_stream().unwrap(),
            running_player: None,
        };
    }
    pub fn play_file(&mut self, file_path: &Path) {
        // Drop existing player to make it stop
        self.running_player.take();

        // Start new player
        let file = std::fs::File::open(file_path).unwrap();
        let sink = rodio::Sink::connect_new(self.output_stream.mixer());
        sink.append(rodio::Decoder::try_from(file).unwrap().repeat_infinite());
        self.running_player = Some(sink);
    }
    pub fn stop(&mut self) {
        // Drop existing player to make it stop
        self.running_player.take();
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TimerConfig {
    color: String,
    music_file: Option<PathBuf>,
    #[serde(default)]
    flipped: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Config {
    button_toggle: bool,
    left_timer: TimerConfig,
    right_timer: TimerConfig,
}

struct ApplicationState {
    config: Config,
    left_timer: Timer,
    right_timer: Timer,
    audio_controller: AudioController,
}
impl ApplicationState {
    pub fn new(config: Config) -> Self {
        return Self {
            config,
            left_timer: Timer::new(),
            right_timer: Timer::new(),
            audio_controller: AudioController::new(),
        };
    }

    pub fn clear_timers(&mut self) {
        self.left_timer.clear();
        self.right_timer.clear();
        self.audio_controller.stop();
    }
    pub fn start_left_timer(&mut self) {
        if self.left_timer.is_running() && self.config.button_toggle {
            self.left_timer.stop();
            return;
        }
        self.right_timer.stop();
        self.left_timer.start();
        if let Some(ref music_path) = self.config.left_timer.music_file {
            self.audio_controller.play_file(music_path);
        }
    }
    pub fn start_right_timer(&mut self) {
        if self.right_timer.is_running() && self.config.button_toggle {
            self.right_timer.stop();
            return;
        }
        self.left_timer.stop();
        self.right_timer.start();
        if let Some(ref music_path) = self.config.right_timer.music_file {
            self.audio_controller.play_file(music_path);
        }
    }
}

fn activate(application: &gtk::Application, timers: Arc<Mutex<ApplicationState>>) {
    let state = timers.lock().unwrap();

    // Set up the window
    let window = gtk::ApplicationWindow::new(application);
    window.style_context().add_class("archery-timer");
    window.fullscreen();

    // Create basic structure within window
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    window.set_events(EventMask::KEY_PRESS_MASK);
    window.set_child(Some(&bar));

    let left = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    bar.pack_start(&left, true, true, 0);
    let left_style = left.style_context();
    left_style.add_class("left-timer");
    left_style.add_provider(
        &{
            let provider = gtk::CssProvider::new();
            provider
                .load_from_data(
                    format!(
                        "* {{ background-color: {}; }}",
                        state.config.left_timer.color
                    )
                    .as_bytes(),
                )
                .unwrap();
            provider
        },
        100,
    );
    // left_style.set_property("background-color", &state.config.left_timer.color);

    let left_label = gtk::Label::new(Some("Test left"));
    if state.config.left_timer.flipped {
        left_label.set_angle(180.0);
    }
    left.pack_start(&left_label, true, true, 3);

    let right = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    bar.pack_end(&right, true, true, 0);
    let right_style = right.style_context();
    right_style.add_class("right-timer");
    right_style.add_provider(
        &{
            let provider = gtk::CssProvider::new();
            provider
                .load_from_data(
                    format!(
                        "* {{ background-color: {}; }}",
                        state.config.right_timer.color
                    )
                    .as_bytes(),
                )
                .unwrap();
            provider
        },
        100,
    );
    // right_style.set_property("background-color", &state.config.right_timer.color);

    let right_label = gtk::Label::new(Some("Test right"));
    if state.config.right_timer.flipped {
        right_label.set_angle(180.0);
    }
    right.pack_start(&right_label, true, true, 3);

    drop(state);

    {
        let state = Arc::clone(&timers);
        window.connect_key_press_event(move |_, key| {
            let key = key.keyval();
            match key {
                gdk::keys::constants::r => {
                    let mut state = state.lock().unwrap();
                    state.clear_timers();
                    return glib::Propagation::Stop;
                }
                gdk::keys::constants::j => {
                    let mut state = state.lock().unwrap();
                    state.start_left_timer();
                    return glib::Propagation::Stop;
                }
                gdk::keys::constants::k => {
                    let mut state = state.lock().unwrap();
                    state.start_right_timer();
                    return glib::Propagation::Stop;
                }
                _ => {
                    return glib::Propagation::Proceed;
                }
            }
        });
    }

    {
        let window = window.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            if let Ok(timers) = timers.try_lock() {
                let left_duration = timers.left_timer.get_duration().as_millis();
                let right_duration = timers.right_timer.get_duration().as_millis();
                drop(timers);

                left_label.set_text(&format_timestamp(left_duration));
                right_label.set_text(&format_timestamp(right_duration));
            }
            if let (Some(gdk_window), Some(display)) = (window.window(), gdk::Display::default()) {
                let cursor = gdk::Cursor::for_display(&display, gdk::CursorType::BlankCursor);
                gdk_window.set_cursor(cursor.as_ref());
            }
            return glib::ControlFlow::Continue;
        });
    }

    // Get ready for activation
    application.connect_activate(move |_| {
        window.show_all();
    });
}

fn format_timestamp(timestamp_ms: u128) -> String {
    let timestamp_s = timestamp_ms / 1000;
    let s = timestamp_s % 60;
    let timestamp_m = timestamp_s / 60;
    let m = timestamp_m;
    format!("{m:02}:{s:02}")
}

fn main() {
    let config_file = std::fs::File::open("./config.yml").unwrap();
    let config = serde_yaml::from_reader(config_file).unwrap();
    let timers = Arc::new(Mutex::new(ApplicationState::new(config)));

    let application =
        gtk::Application::new(Some("com.shaunkeys.archery-timer"), Default::default());

    {
        let timers = Arc::clone(&timers);
        application.connect_startup(move |app| {
            eprintln!("Application startup");
            let provider = gtk::CssProvider::new();
            provider
                .load_from_data(include_bytes!("../css/main.css"))
                .expect("Failed to load css");
            let screen = gdk::Screen::default().expect("Error initializing gtk css provider.");
            gtk::StyleContext::add_provider_for_screen(
                &screen,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );

            activate(app, Arc::clone(&timers));
        });
    }

    if cfg!(feature = "gpio") {
        let timers = Arc::clone(&timers);
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(track_gpio(Arc::clone(&timers)));
        });
    }

    application.run();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ButtonSide {
    Left,
    Right,
}

struct MaybeFuture<F: Future<Output = T> + Unpin, T>(Option<F>);
impl<F: Future<Output = T> + Unpin, T> Future for MaybeFuture<F, T> {
    type Output = T;
    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        if let Some(ref mut future) = self.as_mut().0 {
            return std::future::Future::poll(Pin::new(future), cx);
        }
        return std::task::Poll::Pending;
    }
}

enum TimeoutEvent {
    TickTimeout,
    ResetTimeout,
}

struct ButtonTracker {
    app: Arc<Mutex<ApplicationState>>,
    left_state: bool,
    right_state: bool,
    // Allows us to check for more button events before executing the action
    tick_timeout: Option<Pin<Box<Sleep>>>,
    // Allows us to re-trigger ourselves when the reset sequnce has elapsed.
    reset_timeout: Option<Pin<Box<Sleep>>>,
    // Allows us to wait for buttons to be released before counting them
    // after a reset sequence
    reset_debounce: bool,
}
impl ButtonTracker {
    pub fn new(app: Arc<Mutex<ApplicationState>>) -> Self {
        return Self {
            app,
            left_state: false,
            right_state: false,
            tick_timeout: None,
            reset_timeout: None,
            reset_debounce: false,
        };
    }
    pub async fn get_timeout(&mut self) -> TimeoutEvent {
        return tokio::select! {
            _ = MaybeFuture(self.tick_timeout.as_mut()) => {
                self.tick_timeout = None;
                TimeoutEvent::TickTimeout
            },
            _ = MaybeFuture(self.reset_timeout.as_mut()) => {
                self.reset_timeout = None;
                TimeoutEvent::ResetTimeout
            },
        };
    }
    pub fn timeout_update(&mut self, event: TimeoutEvent) {
        match event {
            TimeoutEvent::TickTimeout => {
                log::debug!("Ticking on {:?}", (self.left_state, self.right_state));
                match (self.left_state, self.right_state) {
                    (true, true) if !self.reset_debounce => {
                        self.reset_timeout =
                            Some(Box::pin(tokio::time::sleep(Duration::from_secs(3))));
                    }
                    (true, false) if !self.reset_debounce => {
                        self.reset_timeout = None;
                        self.app.lock().unwrap().start_left_timer();
                    }
                    (false, true) if !self.reset_debounce => {
                        self.reset_timeout = None;
                        self.app.lock().unwrap().start_right_timer();
                    }
                    (false, false) => {
                        self.reset_debounce = false;
                    }
                    _ => {}
                }
            }
            TimeoutEvent::ResetTimeout => {
                self.reset_debounce = true;
                self.app.lock().unwrap().clear_timers();
            }
        }
    }
    pub fn update(&mut self, side: ButtonSide, state: bool) {
        let existing_state = match side {
            ButtonSide::Left => &mut self.left_state,
            ButtonSide::Right => &mut self.right_state,
        };
        if *existing_state == state {
            return;
        }
        log::debug!("{side:?} button set to {state}");
        *existing_state = state;

        self.tick_timeout = Some(Box::pin(tokio::time::sleep(Duration::from_millis(25))));
    }
}

async fn track_gpio(timers: Arc<Mutex<ApplicationState>>) {
    let mut chip = Chip::new("/dev/gpiochip0").unwrap();
    let mut left_button = AsyncLineEventHandle::new(
        chip.get_line(23)
            .unwrap()
            .events(
                LineRequestFlags::INPUT | LineRequestFlags::ACTIVE_LOW,
                EventRequestFlags::BOTH_EDGES,
                "read-input",
            )
            .unwrap(),
    )
    .unwrap();
    let mut right_button = AsyncLineEventHandle::new(
        chip.get_line(24)
            .unwrap()
            .events(
                LineRequestFlags::INPUT | LineRequestFlags::ACTIVE_LOW,
                EventRequestFlags::BOTH_EDGES,
                "read-input",
            )
            .unwrap(),
    )
    .unwrap();

    let mut button_tracker = ButtonTracker::new(timers);
    loop {
        tokio::select! {
            event = button_tracker.get_timeout() => {
                button_tracker.timeout_update(event);
            }
            Some(Ok(left_button)) = left_button.next() => {
                button_tracker.update(
                    ButtonSide::Left,
                    left_button.event_type() == gpio_cdev::EventType::RisingEdge,
                );
            },
            Some(Ok(right_button)) = right_button.next() => {
                button_tracker.update(
                    ButtonSide::Right,
                    right_button.event_type() == gpio_cdev::EventType::RisingEdge,
                );
            },
        }
    }
}
