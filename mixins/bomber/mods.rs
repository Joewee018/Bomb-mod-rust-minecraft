//! Bomber Mod Registration
//!
//! Registers the mod in Options > Mods menu.

use crate::bomber_runtime::SIMULATION_ENABLED;
use crate::flight::is_in_flight_mode;
use std::sync::atomic::Ordering;

/// Called when Options > Mods list opens
pub fn native_mods(mods: &mut dyn ModList) {
    mods.add_mod(
        "bomber",
        "Bomber Command",
        "Hyper-realistic bomber simulation with 6 historical aircraft, 12 bomb types, custom physics engine, first-person flight mode, and nuclear explosions. Features B-17G, B-24, B-29, B-52H, B-1B, B-2 Spirit with authentic flight models and Battlefield 5-style bomb picker.",
        vec![
            ("flight_mode", "Enter Flight Mode (F)"),
            ("bomb_picker", "Bomb Picker (Hold B)"),
            ("bomb_release", "Release Bomb (Space)"),
            ("autopilot", "Toggle Autopilot (AP)"),
            ("spawn_b17", "Spawn B-17G"),
            ("spawn_b29", "Spawn B-29"),
            ("spawn_b52", "Spawn B-52H"),
            ("spawn_b1b", "Spawn B-1B"),
            ("spawn_b2", "Spawn B-2"),
            ("nuke_tsar", "Detonate Tsar Bomba"),
            ("eject", "Eject (X)"),
        ],
    );
}

trait ModList {
    fn add_mod(&mut self, id: &str, title: &str, description: &str, keybinds: Vec<(&str, &str)>);
}