use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::{fs, thread};

use log::{error, info, trace};
use notify::event::AccessKind::Close;
use notify::EventKind::Access;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};

use crate::binding::Binding;
use crate::event::MouseButton;
use crate::points_to_angles::points_to_angles;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub shape_button: MouseButton,
    pub bindings: Vec<Binding>,
}

pub fn load(file_path: &str) -> Config {
    let json_config = fs::read_to_string(file_path).unwrap();
    load_from_str(&json_config)
}

pub fn load_from_str(json_config: &str) -> Config {
    let mut config: Config = serde_json::from_str(&json_config).unwrap();
    // xy → angles
    for mut binding in &mut config.bindings {
        binding.event.shapes_angles = binding
            .event
            .shapes_xy
            .iter()
            .map(|shape_xy| points_to_angles(&shape_xy))
            .collect();
    }
    config
}

pub fn get_config_path(config_path_from_args: &Option<String>) -> PathBuf {
    if let Some(config_path) = config_path_from_args {
        PathBuf::from_str(config_path).unwrap()
    } else {
        [
            dirs_sys::home_dir().unwrap().to_str().unwrap(),
            ".config",
            "mouse-actions.json",
        ]
        .iter()
        .collect()
    }
}

pub fn get_config(config_path: &Path) -> Config {
    load(config_path.to_str().unwrap())
}

pub fn watch_config(config: Arc<Mutex<Config>>, config_path: PathBuf) {
    thread::Builder::new()
        .name("watch_config".to_string())
        .spawn(move || {
            let (tx, rx) = channel();
            let mut watcher = RecommendedWatcher::new(tx, notify::Config::default()).unwrap();
            watcher
                .watch(config_path.as_path(), RecursiveMode::NonRecursive)
                .unwrap();

            loop {
                match rx.recv() {
                    Ok(Ok(notify::Event {
                        kind: Access(Close(notify::event::AccessMode::Write)),
                        ..
                    })) => {
                        info!("Reload the config !");
                        *config.lock().unwrap() = get_config(&config_path);
                    }
                    Ok(event) => trace!("watcher: broken event: {:?}", event),
                    Err(e) => {
                        error!("watcher: watch error: {:?}", e);
                        thread::sleep(std::time::Duration::from_secs(1));
                    }
                }
            }
        })
        .unwrap();
}

pub fn init_config_file_if_not_exists(config_path: &Path) {
    if !config_path.exists() {
        let empty_config = Config {
            shape_button: MouseButton::Right,
            bindings: vec![],
        };
        let serialized = serde_json::to_string_pretty(&empty_config).unwrap();

        let mut config_file = match File::create(&config_path) {
            Err(err) => panic!(
                "couldn't create config file {}: {}",
                config_path.display(),
                err
            ),
            Ok(file) => file,
        };

        match config_file.write_all(serialized.as_bytes()) {
            Err(err) => panic!("couldn't write to {}: {}", config_path.display(), err),
            Ok(_) => println!("successfully wrote to {}", config_path.display()),
        }
    }
}

pub fn save_config(config: &Config, config_path_from_args: &Option<String>) {
    let serialized = serde_json::to_string_pretty(&config).unwrap();
    let config_path = get_config_path(config_path_from_args);
    let config_path_bak = config_path.parent().unwrap().join(format!(
        "{}.bak",
        config_path.file_name().unwrap().to_str().unwrap()
    ));
    let _ = fs::remove_file(&config_path_bak);
    fs::rename(&config_path, &config_path_bak)
        .expect("Error while backup the previous config file");
    let mut config_file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(&config_path)
        .unwrap();

    match config_file.write_all(serialized.as_bytes()) {
        Err(err) => panic!("couldn't write to {}: {}", config_path.display(), err),
        Ok(_) => println!("successfully save to {}", config_path.display()),
    }
}

pub fn open_config(config_path: PathBuf) {
    println!("Open config file with xdg-open : {:?}", config_path);
    Command::new("xdg-open")
        .args(config_path.to_str())
        .status()
        .map_err(|err| error!("Command err: {:?}", err))
        .ok();
}

#[cfg(test)]
mod tests {
    use crate::event::{ClickEvent, Edge, KeyboardModifier, MouseButton, Point, PressState};

    use super::*;

    #[test]
    fn test_json_serialize() {
        let config = Config {
            shape_button: MouseButton::Right,
            bindings: vec![Binding {
                event: ClickEvent {
                    button: MouseButton::Left,
                    edges: vec![Edge::Top, Edge::Left],
                    modifiers: vec![KeyboardModifier::ControlLeft],
                    event_type: PressState::Press,
                    shapes_angles: vec![vec![0.0, 1.0, 2.0]],
                    shapes_xy: vec![],
                },
                cmd: vec![String::from("xlogo")],
                comment: String::new(),
            }],
        };

        let serialized = serde_json::to_string_pretty(&config).unwrap();
        let expected = r#"{
  "shape_button": "Right",
  "bindings": [
    {
      "comment": "",
      "event": {
        "button": "Left",
        "edges": [
          "Top",
          "Left"
        ],
        "modifiers": [
          "ControlLeft"
        ],
        "event_type": "Press"
      },
      "cmd": [
        "xlogo"
      ]
    }
  ]
}"#;
        println!("serialized = {}", serialized);
        assert_eq!(
            serialized
                .split('\n')
                .map(|s| s.trim())
                .collect::<Vec<&str>>()
                .join("\n"),
            expected
                .split('\n')
                .map(|s| s.trim())
                .collect::<Vec<&str>>()
                .join("\n"),
        );
    }

    #[test]
    fn test_json_deserialize() {
        let serialized = r#"{
  "shape_button": "Right",
  "bindings": [
    {
      "event": {
        "button": "Left",
        "edges": [
          "Top",
          "Left"
        ],
        "modifiers": [
          "ControlLeft"
        ],
        "event_type": "Press",
        "shapes_xy": [[
          0,
          1,
          2,
          3
        ]]
      },
      "cmd": [
        "xlogo"
      ]
    }
  ]
}"#;
        let config: Config = serde_json::from_str(serialized).unwrap();
        println!("config = {:?}", config);
        assert_eq!(config.shape_button, MouseButton::Right);
        let binding = &config.bindings[0];
        assert_eq!(binding.cmd[0], "xlogo");
        assert_eq!(binding.event.button, MouseButton::Left);
        assert_eq!(binding.event.edges[0], Edge::Top);
        assert_eq!(binding.event.edges[1], Edge::Left);
        assert_eq!(binding.event.modifiers[0], KeyboardModifier::ControlLeft);
        assert_eq!(binding.event.event_type, PressState::Press);
        assert_eq!(
            binding.event.shapes_xy.first().unwrap().to_vec(),
            vec![Point { x: 0, y: 1 }, Point { x: 2, y: 3 }]
        );
    }
}
