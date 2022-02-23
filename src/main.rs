use nannou_osc::Type;
use serde::Deserialize;
use std::fs::File;
use std::path::Path;
use std::{thread, time};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BulbService {
    HomeAssistant,
}

#[derive(Debug, Deserialize)]
struct HomeAssistantConfig {
    entity_id: String,
    server_ip: String,
    server_port: i32,
    bearer_token: String,
}

#[derive(Debug, Deserialize)]
struct Config {
    vrchat_ip: String,
    vrchat_port: i32,
    max_updates_per_second: i32,
    bulb_service: BulbService,
    home_assistant: HomeAssistantConfig,
}

#[derive(Debug, PartialEq, Clone, Copy)]
struct BulbState {
    on: bool,
    hue: f32,
    brightness: f32,
}

fn translate(value: f32, prev_start: f32, prev_end: f32, new_start: f32, new_end: f32) -> f32 {
    let prev_span = prev_end - prev_start;
    let new_span = new_end - new_start;
    let scaled_value = (value - prev_start) / prev_span;
    return new_start + (scaled_value * new_span);
}

fn get_config(file: &str) -> Config {
    let config_file_path = Path::new(file);
    let display = config_file_path.display();
    let file = match File::open(&config_file_path) {
        Err(why) => panic!("Couldn't open {}: {}", display, why),
        Ok(file) => file,
    };

    return serde_yaml::from_reader(file).expect("Error while parsing settings file.");
}

fn get_home_assistant_state(config: &HomeAssistantConfig) -> Result<BulbState, reqwest::Error> {
    let url: String = "http://".to_owned()
        + &config.server_ip
        + ":"
        + &config.server_port.to_string()
        + "/api/states/"
        + &config.entity_id;
    let client = reqwest::blocking::Client::new();
    let res = client
        .get(url)
        .header("Authorization", "Bearer ".to_owned() + &config.bearer_token)
        .send()?;
    let json: serde_json::Value = serde_json::from_str(&res.text()?)
        .expect("JSON from Home Assistant endpoint contained errors.");

    let on = json["state"] == "on";
    let hue: f32 = match &json["attributes"]["hs_color"][0] {
        serde_json::Value::Number(val) => val
            .as_f64()
            .expect("Hue value in home_assistant was there but wasn't a number.")
            as f32,
        _ => 0.0,
    };
    let brightness: f32 = match &json["attributes"]["brightness"] {
        serde_json::Value::Number(val) => val
            .as_f64()
            .expect("Brightness value in home_assistant was there but wasn't a number.")
            as f32,
        _ => 0.0,
    };
    return Ok(BulbState {
        on: on,
        hue: translate(hue, 0.0, 360.0, 0.0, 1.0),
        brightness: translate(brightness, 0.0, 255.0, 0.0, 1.0),
    });
}

fn get_bulb_state(config: &Config) -> BulbState {
    return match config.bulb_service {
        BulbService::HomeAssistant => match get_home_assistant_state(&config.home_assistant) {
            Ok(res) => res,
            Err(err) => panic!("Failed to get status from home assistant: {}", err),
        },
    };
}

fn update_vrchat(sender: &nannou_osc::Sender<nannou_osc::Connected>, state: &BulbState) -> () {
    sender
        .send(("/avatar/parameters/on", vec![Type::Bool(state.on)]))
        .ok();
    sender
        .send(("/avatar/parameters/Color", vec![Type::Float(state.hue)]))
        .ok();
    sender
        .send((
            "/avatar/parameters/brightness",
            vec![Type::Float(state.brightness)],
        ))
        .ok();
    println!("Sent updated state to VRChat");
}

fn main() {
    let config: Config = get_config("settings.yaml");

    // Start the OSC sender
    let vrc_addr = format!("{}:{}", config.vrchat_ip, config.vrchat_port);
    let sender = nannou_osc::sender().unwrap().connect(vrc_addr).unwrap();

    // Run loop
    let max_loop_speed = time::Duration::from_secs_f32(1.0 / config.max_updates_per_second as f32);
    let mut state = get_bulb_state(&config);
    let mut old_state = state.clone();
    update_vrchat(&sender, &state);
    loop {
        // Save the start
        let start = time::Instant::now();
        // Send the update to VRChat if the light status has changed
        if state != old_state {
            update_vrchat(&sender, &state);
        }
        // Wait if the max update time hasn't passed
        let elapsed = start.elapsed();
        if elapsed < max_loop_speed {
            thread::sleep(max_loop_speed - elapsed);
        }
        println!("{:?}", start.elapsed());
        // Get the new state from the light
        old_state = state;
        state = get_bulb_state(&config);
    }
}
