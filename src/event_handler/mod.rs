mod tone_generator;

use crate::types::*;
use tone_generator::ToneGenerator;

use interception as ic;
use vigem::*;

use serde::{Deserialize, Serialize};

use std::collections::{HashMap, VecDeque};
use std::hint::spin_loop;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Hash, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bind {
    Keyboard(ic::ScanCode),
    Mouse(MouseButton),
}

#[derive(Serialize, Deserialize, Hash, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DodgeAction {
    Jump,
    Forwards,
    Backwards,
    Left,
    Right,
}

#[derive(Serialize, Deserialize, Hash, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitAction {
    Reset,
    Toggle,
    Increment,
    Decrement,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum ControllerAction {
    Button(ControllerButton),
    Analog(f64, f64),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    sensitivity: f64,

    sample_window: Duration,
    dodge_lock_duration: Duration,

    spin_period: Duration,

    oversteer_alert_enabled: bool,
    oversteer_alert_threshold: f64,
    oversteer_alert: tone_generator::Config,

    analog_mask: (bool, bool),
    analog_circularize: bool,
    mouse_button_fix: bool,

    binds: HashMap<Bind, ControllerAction>,
    dodge_binds: HashMap<DodgeAction, Bind>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            sensitivity: 1.0,

            sample_window: Duration::from_millis(20),
            dodge_lock_duration: Duration::from_millis(50),

            spin_period: Duration::from_millis(2),

            oversteer_alert_enabled: false,
            oversteer_alert_threshold: 1.5,
            oversteer_alert: tone_generator::Config::default(),

            analog_mask: (true, true),
            analog_circularize: true,
            mouse_button_fix: false,

            binds: HashMap::new(),
            dodge_binds: HashMap::new(),
        }
    }
}

pub struct EventHandler {
    config: Config,

    rx: mpsc::Receiver<Event>,

    vigem: Vigem,
    target: Target,
    report: XUSBReport,

    tone_generator: Option<ToneGenerator>,

    mouse_samples: VecDeque<(i32, i32, Instant)>,
    mouse_button_states: (KeyState, KeyState),

    analog_locked: bool,
    analog_lock_end: Instant,

    analog_lock_x: f64,
    analog_lock_y: f64,

    iteration_count: i32,
    iteration_total: Duration,
    iteration_window_start: Instant,
}

impl EventHandler {
    const ANALOG_MAX: f64 = -(i16::MIN as f64);

    pub fn new(rx: mpsc::Receiver<Event>, config: Config) -> Result<Self, anyhow::Error> {
        let mut vigem = Vigem::new();
        vigem.connect()?;

        let mut target = Target::new(TargetType::Xbox360);
        vigem.target_add(&mut target)?;

        info!("ViGEm connected, controller index: {}", target.index());

        info!(
            "sensitivity: {}, sample_window: {:#?}, dodge_lock_duration: {:#?}",
            config.sensitivity, config.sample_window, config.dodge_lock_duration
        );

        let tone_generator = match config.oversteer_alert_enabled {
            true => Some(ToneGenerator::new(config.oversteer_alert)?),
            false => None,
        };

        Ok(EventHandler {
            config: config,

            rx: rx,

            vigem: vigem,
            target: target,
            report: XUSBReport::default(),

            tone_generator: tone_generator,

            mouse_samples: VecDeque::new(),
            mouse_button_states: (KeyState::Up, KeyState::Up),

            analog_locked: false,
            analog_lock_end: Instant::now(),

            analog_lock_x: 0.0,
            analog_lock_y: 0.0,

            iteration_count: 0,
            iteration_total: Duration::from_secs(0),
            iteration_window_start: Instant::now(),
        })
    }

    pub fn run(&mut self) -> Result<(), anyhow::Error> {
        let mut w = false;
        let mut a = false;
        let mut s = false;
        let mut d = false;
        loop {
            let iteration_start = Instant::now();

            let mut event = self.rx.try_recv();
            while event.is_err() && iteration_start.elapsed() < self.config.spin_period {
                spin_loop();
                event = self.rx.try_recv();
            }

            if let Ok(event) = event {
                match event {
                    Event::MouseMove(x, y) => self.handle_mouse_move(x, y),

                    Event::MouseButton(button, state) => {
                        if button == MouseButton::Left {
                            self.mouse_button_states.0 = state;
                        }

                        if button == MouseButton::Right {
                            self.mouse_button_states.1 = state;
                        }

                        self.handle_bind(Bind::Mouse(button), state);

                        if self.config.mouse_button_fix && state == KeyState::Up {
                            if self.mouse_button_states.0 == KeyState::Down {
                                self.handle_bind(Bind::Mouse(MouseButton::Left), KeyState::Down)
                            }

                            if self.mouse_button_states.1 == KeyState::Down {
                                self.handle_bind(Bind::Mouse(MouseButton::Right), KeyState::Down)
                            }
                        }
                    }

                    Event::Keyboard(scancode, state) => {
                        self.handle_bind(Bind::Keyboard(scancode), state);
                        if state == KeyState::Up && scancode == ic::ScanCode::W {
                            w = false;
                        }
                        if state == KeyState::Up && scancode == ic::ScanCode::A {
                            a = false;
                        }
                        if state == KeyState::Up && scancode == ic::ScanCode::S {
                            s = false;
                        }
                        if state == KeyState::Up && scancode == ic::ScanCode::D {
                            d = false;
                        }
                        if state == KeyState::Down && scancode == ic::ScanCode::W {
                            w = true;
                        }
                        if state == KeyState::Down && scancode == ic::ScanCode::A {
                            a = true;
                        }
                        if state == KeyState::Down && scancode == ic::ScanCode::S {
                            s = true;
                        }
                        if state == KeyState::Down && scancode == ic::ScanCode::D {
                            d = true;
                        }
                        if w == s {
                            self.report.s_thumb_ly = 0;
                        } else {
                            self.report.s_thumb_ly = if w { i16::MAX } else { i16::MIN };
                        }
                        if a == d {
                            self.report.s_thumb_lx = 0;
                        } else {
                            self.report.s_thumb_lx = if d { i16::MAX } else { i16::MIN };
                        }
                    }

                    Event::Reset => {
                        self.mouse_button_states = (KeyState::Up, KeyState::Up);
                        self.report = XUSBReport::default();
                    }
                }
            }

            self.update_analog();
            self.vigem.update(&self.target, &self.report)?;

            if log_enabled!(log::Level::Info) {
                self.iteration_count += 1;
                self.iteration_total += iteration_start.elapsed();

                if self.iteration_window_start.elapsed() > Duration::from_secs(2) {
                    debug!(
                        "{} loops, {} per sec, avg = {:#?}",
                        self.iteration_count,
                        self.iteration_count as f64 / 2.0,
                        self.iteration_total.div_f64(self.iteration_count.into())
                    );

                    self.iteration_count = 0;
                    self.iteration_total = Duration::from_secs(0);
                    self.iteration_window_start = Instant::now();
                }
            }
        }
    }

    fn handle_bind(&mut self, bind: Bind, state: KeyState) {
        let controller_button = match self.config.binds.get(&bind) {
            Some(ControllerAction::Button(controller_button)) => controller_button,
            Some(ControllerAction::Analog(_x, _y)) => {
                return;
            }
            None => return,
        };

        match *controller_button {
            ControllerButton::LeftTrigger => match state {
                KeyState::Down => self.report.b_left_trigger = u8::MAX,
                KeyState::Up => self.report.b_left_trigger = 0,
            },

            ControllerButton::RightTrigger => match state {
                KeyState::Down => self.report.b_right_trigger = u8::MAX,
                KeyState::Up => self.report.b_right_trigger = 0,
            },

            button => {
                let button_flag = XButton::from_bits(button as u16).unwrap();

                match state {
                    KeyState::Down => self.report.w_buttons |= button_flag,
                    KeyState::Up => self.report.w_buttons &= !button_flag,
                }
            }
        }

        if state == KeyState::Up {
            return;
        }

        if let Some(jump_bind) = self.config.dodge_binds.get(&DodgeAction::Jump) {
            if *jump_bind == bind {
                self.handle_jump();
            }
        }
    }

    fn handle_jump(&mut self) {
        self.analog_locked = true;
        self.analog_lock_end = Instant::now() + self.config.dodge_lock_duration;

        let mut analog = [0.0, 0.0];

        let actions = [
            (DodgeAction::Forwards, 1, 1.0),
            (DodgeAction::Backwards, 1, -1.0),
            (DodgeAction::Left, 0, -1.0),
            (DodgeAction::Right, 0, 1.0),
        ];

        for (action, idx, diff) in actions.iter() {
            if self.dodge_action_pressed(*action) {
                analog[*idx] += *diff;
            }
        }

        self.analog_lock_x = analog[0];
        self.analog_lock_y = analog[1];

        self.set_analog(self.analog_lock_x, self.analog_lock_y);
    }

    fn dodge_action_pressed(&self, action: DodgeAction) -> bool {
        let bind = match self.config.dodge_binds.get(&action) {
            Some(bind) => bind,
            None => return false,
        };

        let button = match self.config.binds.get(&bind) {
            Some(ControllerAction::Button(button)) => button,
            _ => return false,
        };

        match *button {
            ControllerButton::LeftTrigger => return self.report.b_left_trigger > 0,
            ControllerButton::RightTrigger => return self.report.b_right_trigger > 0,
            button => {
                let button_flag = XButton::from_bits(button as u16).unwrap();
                return self.report.w_buttons.contains(button_flag);
            }
        }
    }

    fn handle_mouse_move(&mut self, x: i32, y: i32) {
        let now = Instant::now();
        self.mouse_samples.push_back((x, y, now));
    }

    fn update_analog(&mut self) {
        let now = Instant::now();

        loop {
            let sample = match self.mouse_samples.front() {
                Some(sample) => sample,
                None => break,
            };

            if now - sample.2 > self.config.sample_window {
                self.mouse_samples.pop_front();
            } else {
                break;
            }
        }

        // let window = self.config.sample_window.as_secs_f64();
        let mut mouse_vel = (0.0, 0.0);

        /*
        let dt_offset = if self.mouse_samples.len() > 0 {
            let sample = self.mouse_samples[0];
            if (now - sample.2).as_secs_f64() * 1000.0 < 1.0 {
                (now - sample.2).as_secs_f64()
            } else {
                0.0005
            }
        } else {
            0.0
        };
        */

        for &(x, y, _) in self.mouse_samples.iter() {
            // let dt = ((now - t).as_secs_f64() - dt_offset) / window;

            mouse_vel.0 += x as f64;
            mouse_vel.1 += y as f64;
        }

        // TODO: proper analog binds
        if !self.config.analog_mask.0 {
            mouse_vel.0 = 0.0;
        }

        if !self.config.analog_mask.1 {
            mouse_vel.1 = 0.0;
        }

        let multiplier =
            self.config.sensitivity / (1e4 * self.config.sample_window.as_secs_f64());

        self.set_analog_linear(
            mouse_vel.0 as f64 * multiplier,
            -mouse_vel.1 as f64 * multiplier,
        );
    }

    fn set_analog(&mut self, x: f64, y: f64) {
        let alert = x.abs().max(y.abs()) >= self.config.oversteer_alert_threshold;
        self.tone_generator.as_mut().map(|tg| tg.enable(alert));

        if self.config.analog_circularize {
            self.set_analog_circularized(x, y);
        } else {
            self.set_analog_linear(x, y);
        }
    }

    fn set_analog_circularized(&mut self, x: f64, y: f64) {
        let angle = y.atan2(x);
        let radius = (x.powi(2) + y.powi(2)).sqrt();

        self.report.s_thumb_lx = (angle.cos() * radius * Self::ANALOG_MAX) as i16;
        self.report.s_thumb_ly = (angle.sin() * radius * Self::ANALOG_MAX) as i16;
    }

    fn set_analog_linear(&mut self, x: f64, y: f64) {
        if x.abs() <= 1.0 && y.abs() <= 1.0 {
            self.report.s_thumb_rx = (x * Self::ANALOG_MAX) as i16;
            self.report.s_thumb_ry = (y * Self::ANALOG_MAX) as i16;

            return;
        }

        let overshoot = x.abs().max(y.abs());

        let angle = y.atan2(x);
        let radius = (x.powi(2) + y.powi(2)).sqrt();

        let new_radius = radius / overshoot;

        self.report.s_thumb_rx = (angle.cos() * new_radius * Self::ANALOG_MAX) as i16;
        self.report.s_thumb_ry = (angle.sin() * new_radius * Self::ANALOG_MAX) as i16;
    }
}
