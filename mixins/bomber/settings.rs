//! Bomber Mod Settings UI
//!
//! Settings page for Options > Mods > Bomber Mod

use crate::bomber_runtime::{ModSettings, PhysicsRealism, with_settings};

/// Called when settings page opens
pub fn native_settings(settings_ui: &mut dyn SettingsUi) {
    let data = get_settings_ui_data();

    settings_ui.add_section("Physics", |ui| {
        ui.add_dropdown("physics_realism", "Physics Realism", &[
            ("arcade", "Arcade"),
            ("simplified", "Simplified"),
            ("realistic", "Realistic (Default)"),
            ("hardcore", "Hardcore"),
        ], data.physics_realism as u8);

        ui.add_checkbox("damage_enabled", "Enable Damage", data.damage_enabled);
        ui.add_checkbox("nuke_effects", "Nuclear Effects", data.nuke_effects);
        ui.add_checkbox("fallout_enabled", "Fallout & Radiation", data.fallout_enabled);
    });

    settings_ui.add_section("Visual Effects", |ui| {
        ui.add_checkbox("camera_shake", "Camera Shake", data.camera_shake);
        ui.add_checkbox("g_force_effects", "G-Force Effects", data.g_force_effects);
        ui.add_checkbox("bomb_trajectory_preview", "Bomb Trajectory Preview", data.bomb_trajectory_preview);
    });

    settings_ui.add_section("Gameplay", |ui| {
        ui.add_checkbox("auto_level_assist", "Auto-Level Assist", data.auto_level_assist);
        ui.add_checkbox("friendly_fire", "Friendly Fire", data.friendly_fire);
        ui.add_checkbox("unlimited_ammo", "Unlimited Ammo", data.unlimited_ammo);
    });

    settings_ui.add_section("Key Bindings", |ui| {
        ui.add_text("Flight Mode: F");
        ui.add_text("Bomb Picker (hold): B");
        ui.add_text("Release Bomb: SPACE");
        ui.add_text("Throttle Up/Down: W / S");
        ui.add_text("Pitch/Roll: Mouse");
        ui.add_text("Yaw (Rudder): A / D");
        ui.add_text("Flaps Up/Down: F / V");
        ui.add_text("Gear Toggle: G");
        ui.add_text("Bomb Bay Toggle: N");
        ui.add_text("Autopilot Toggle: AP");
        ui.add_text("Camera Cycle: C");
        ui.add_text("Eject: X");
    });

    settings_ui.add_section("About", |ui| {
        ui.add_text("Bomber Mod v0.1.0");
        ui.add_text("6 Historical Bombers | 12 Bomb Types");
        ui.add_text("Custom Physics Engine | Nuclear Effects");
        ui.add_text("By: Your Name");
    });
}

/// Get settings data for UI
pub fn get_settings_ui_data() -> SettingsUIData {
    with_settings(|s| SettingsUIData {
        physics_realism: s.physics_realism,
        damage_enabled: s.damage_enabled,
        nuke_effects: s.nuke_effects,
        fallout_enabled: s.fallout_enabled,
        camera_shake: s.camera_shake,
        g_force_effects: s.g_force_effects,
        auto_level_assist: s.auto_level_assist,
        bomb_trajectory_preview: s.bomb_trajectory_preview,
        friendly_fire: s.friendly_fire,
        unlimited_ammo: s.unlimited_ammo,
    })
}

#[derive(Debug, Clone)]
pub struct SettingsUIData {
    pub physics_realism: PhysicsRealism,
    pub damage_enabled: bool,
    pub nuke_effects: bool,
    pub fallout_enabled: bool,
    pub camera_shake: bool,
    pub g_force_effects: bool,
    pub auto_level_assist: bool,
    pub bomb_trajectory_preview: bool,
    pub friendly_fire: bool,
    pub unlimited_ammo: bool,
}

/// Settings UI trait (Horizon provides implementation)
trait SettingsUi {
    fn add_section<F>(&mut self, name: &str, f: F) where F: FnOnce(&mut SectionUi);
}

trait SectionUi {
    fn add_dropdown(&mut self, id: &str, label: &str, options: &[(&str, &str)], selected: u8);
    fn add_checkbox(&mut self, id: &str, label: &str, checked: bool);
    fn add_text(&mut self, text: &str);
}