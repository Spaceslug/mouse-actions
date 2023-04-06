use std::sync::{Arc, Mutex};

use log::Level::Trace;
use log::{debug, log_enabled, trace};
use rdev::{grab, Event, EventType, GrabError, Key};

use crate::args::Args;
use crate::config::Config;
use crate::event::{
    ButtonState, ClickEvent, Edge, KeyboardModifier, KeyboardState, MouseButton, Point,
    PointHistory, PointHistoryArcMutex, PressState,
};
use crate::{listen, points_to_angles, trace_svg};

pub struct GrabContext {
    pub point_history: PointHistoryArcMutex,
    pub button_state: Arc<Mutex<ButtonState>>,
    pub keyboard_state: Arc<Mutex<KeyboardState>>,
    pub config: Arc<Mutex<Config>>,
    pub last_point: Arc<Mutex<Point>>,
    pub args: Arc<Args>,
}

pub fn start_grab_binding(
    args: Arc<Args>,
    config: Arc<Mutex<Config>>,
    process_event_fn: fn(Arc<Mutex<Config>>, ClickEvent, Arc<Args>) -> bool,
) -> Result<(), GrabError> {
    let point_history: PointHistoryArcMutex = Arc::new(Mutex::new(PointHistory::new()));
    let button_state: Arc<Mutex<ButtonState>> = Arc::new(Mutex::new(ButtonState::None));
    let keyboard_state: Arc<Mutex<KeyboardState>> = Arc::new(Mutex::new(KeyboardState::default()));
    let last_point: Arc<Mutex<Point>> = Arc::new(Mutex::new(Point { x: 0, y: 0 }));
    if !args.no_listen {
        listen::start_listen(last_point.clone());
    }

    debug!("Start grab");
    grab(move |event: Event| {
        let context = GrabContext {
            point_history: point_history.clone(),
            button_state: button_state.clone(),
            keyboard_state: keyboard_state.clone(),
            config: config.clone(),
            last_point: last_point.clone(),
            args: args.clone(),
        };
        grab_event_fn(event, context, process_event_fn)
    })
}

pub fn grab_event_fn(
    event: Event,
    GrabContext {
        point_history,
        button_state,
        keyboard_state,
        config,
        last_point,
        args,
    }: GrabContext,
    process_event_fn: fn(Arc<Mutex<Config>>, ClickEvent, Arc<Args>) -> bool,
) -> Option<Event> {
    match event.event_type {
        EventType::MouseMove { x, y } => {
            if args.no_listen {
                last_point.lock().unwrap().set(x as i32, y as i32);
            }
            if let ButtonState::Pressed(pressed_btn) = *button_state.lock().unwrap() {
                if config.lock().unwrap().shape_button.to_rdev_event() == pressed_btn {
                    let mut histo = point_history.lock().unwrap();
                    if !histo.is_full() {
                        histo.push(*last_point.lock().unwrap());
                    } else {
                        trace!("point_history is full !")
                    }
                }
            }
            Some(event)
        }
        EventType::ButtonPress(pressed_btn) => {
            *button_state.lock().unwrap() = ButtonState::Pressed(pressed_btn);
            let last_point_clone = *last_point.lock().unwrap();

            let click_event = ClickEvent {
                button: MouseButton::from_rdev_event(pressed_btn),
                edges: Edge::edges_from_pos(last_point_clone.x, last_point_clone.y),
                modifiers: KeyboardModifier::from_keyboard_state(*keyboard_state.lock().unwrap()),
                event_type: PressState::Press,
                shape_angles: vec![],
                shape_xy: PointHistory::new(),
            };
            if config.lock().unwrap().shape_button.to_rdev_event() == pressed_btn {
                let mut histo = point_history.lock().unwrap();
                if !histo.is_full() {
                    histo.push(last_point_clone);
                } else {
                    trace!("point_history is full !")
                }
                if histo.len() < 10 {
                    process_event_fn(config, click_event, args);
                }
                return None;
            }
            if process_event_fn(config, click_event, args) {
                Some(event)
            } else {
                None
            }
        }
        EventType::ButtonRelease(btn) => {
            let angles = points_to_angles::points_to_angles(&point_history.lock().unwrap());

            if log_enabled!(Trace) {
                let normalized_points = normalize_points(&point_history.lock().unwrap(), false);
                trace!("normalized_points = {normalized_points:?}");
                trace_svg::trace_svg(&point_history.lock().unwrap(), &angles);
            }
            let last_point_clone = *last_point.lock().unwrap();
            let click_event = ClickEvent {
                button: MouseButton::from_rdev_event(btn),
                edges: Edge::edges_from_pos(last_point_clone.x, last_point_clone.y),
                modifiers: KeyboardModifier::from_keyboard_state(*keyboard_state.lock().unwrap()),
                event_type: PressState::Release,
                shape_angles: angles,
                shape_xy: point_history.lock().unwrap().clone(),
            };
            point_history.lock().unwrap().clear();
            *button_state.lock().unwrap() = ButtonState::None;

            if process_event_fn(config, click_event, args) {
                Some(event)
            } else {
                None
            }
        }
        EventType::Wheel { delta_y, .. } => {
            let last_point_clone = *last_point.lock().unwrap();
            let click_event = ClickEvent {
                button: MouseButton::from_rdev_wheel(delta_y),
                edges: Edge::edges_from_pos(last_point_clone.x, last_point_clone.y),
                modifiers: KeyboardModifier::from_keyboard_state(*keyboard_state.lock().unwrap()),
                event_type: PressState::Release,
                shape_angles: vec![],
                shape_xy: PointHistory::new(),
            };
            if process_event_fn(config, click_event, args) {
                Some(event)
            } else {
                None
            }
        }
        EventType::KeyPress(key) => {
            match key {
                Key::ShiftLeft => keyboard_state.lock().unwrap().shift_left = true,
                Key::ShiftRight => keyboard_state.lock().unwrap().shift_right = true,
                Key::ControlLeft => keyboard_state.lock().unwrap().control_left = true,
                Key::ControlRight => keyboard_state.lock().unwrap().control_right = true,
                Key::MetaLeft => keyboard_state.lock().unwrap().meta_left = true,
                Key::Alt => keyboard_state.lock().unwrap().alt = true,
                Key::AltGr => keyboard_state.lock().unwrap().alt_gr = true,
                _ => {}
            }
            Some(event)
        }
        EventType::KeyRelease(key) => {
            match key {
                Key::ShiftLeft => keyboard_state.lock().unwrap().shift_left = false,
                Key::ShiftRight => keyboard_state.lock().unwrap().shift_right = false,
                Key::ControlLeft => keyboard_state.lock().unwrap().control_left = false,
                Key::ControlRight => keyboard_state.lock().unwrap().control_right = false,
                Key::MetaLeft => keyboard_state.lock().unwrap().meta_left = false,
                Key::Alt => keyboard_state.lock().unwrap().alt = false,
                Key::AltGr => keyboard_state.lock().unwrap().alt_gr = false,
                _ => {}
            }
            Some(event)
        }
    }
}

pub fn normalize_points(input_points: &PointHistory, use_avg: bool) -> PointHistory {
    let mut out = PointHistory::new();
    if !input_points.is_empty() {
        let min_x = input_points.iter().map(|p| p.x).min().unwrap();
        let max_x = input_points.iter().map(|p| p.x).max().unwrap();
        let width = max_x - min_x;

        let min_y = input_points.iter().map(|p| p.y).min().unwrap();
        let max_y = input_points.iter().map(|p| p.y).max().unwrap();
        let height = max_y - min_y;

        if width > 0 && height > 0 {
            if use_avg {
                let avg_x: i32 =
                    input_points.iter().map(|p| p.x).sum::<i32>() / (input_points.len() as i32);
                let avg_y: i32 =
                    input_points.iter().map(|p| p.y).sum::<i32>() / (input_points.len() as i32);
                for p in input_points.iter() {
                    out.push(Point {
                        x: 100 * (p.x - avg_x) / width,
                        y: 100 * (p.y - avg_y) / height,
                    });
                }
            } else {
                for p in input_points.iter() {
                    out.push(Point {
                        x: 100 * (p.x - min_x) / width,
                        y: 100 * (p.y - min_y) / height,
                    });
                }
            }
        }
    }
    out
}
