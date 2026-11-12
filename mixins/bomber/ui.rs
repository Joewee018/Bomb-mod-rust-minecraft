//! Bomber Mod UI System
//!
//! Bomb picker radial menu (Battlefield 5 style), HUD elements, and settings UI.

use crate::bomber_runtime::{
    BombType, PlaneType, Vec3, ModSettings, PhysicsRealism,
    with_settings, with_plane_simulation, get_player_plane_id,
};
use crate::flight::{get_bomb_picker_items, BombPickerItem};
use nalgebra::Vector3;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

static BOMB_PICKER_OPEN: AtomicBool = AtomicBool::new(false);
static SETTINGS_OPEN: AtomicBool = AtomicBool::new(false);
static RADIAL_MENU_SELECTION: AtomicU8 = AtomicU8::new(255);

/// Toggle bomb picker radial menu
pub fn toggle_bomb_picker() {
    let open = BOMB_PICKER_OPEN.fetch_xor(true, Ordering::Relaxed);
    if !open {
        RADIAL_MENU_SELECTION.store(255, Ordering::Relaxed);
    }
}

/// Check if bomb picker is open
pub fn is_bomb_picker_open() -> bool {
    BOMB_PICKER_OPEN.load(Ordering::Relaxed)
}

/// Get current radial menu selection
pub fn get_radial_selection() -> Option<u8> {
    let sel = RADIAL_MENU_SELECTION.load(Ordering::Relaxed);
    if sel == 255 { None } else { Some(sel) }
}

/// Update radial menu selection from mouse/analog input
pub fn update_radial_selection(angle: f32) {
    // Convert angle (0-360 degrees, 0 = up) to segment index
    // 12 segments for 12 bomb types
    let segments = 12;
    let angle = (angle + 360.0) % 360.0;
    let segment = ((angle + 360.0 / segments / 2.0) / (360.0 / segments)).floor() as u8 % segments as u8;
    RADIAL_MENU_SELECTION.store(segment, Ordering::Relaxed);
}

/// Confirm radial menu selection
pub fn confirm_radial_selection() -> Option<BombType> {
    let selection = get_radial_selection()?;
    let items = get_bomb_picker_items();
    if selection as usize < items.len() {
        BOMB_PICKER_OPEN.store(false, Ordering::Relaxed);
        RADIAL_MENU_SELECTION.store(255, Ordering::Relaxed);
        Some(items[selection as usize].bomb_type)
    } else {
        None
    }
}

/// Cancel radial menu
pub fn cancel_radial_menu() {
    BOMB_PICKER_OPEN.store(false, Ordering::Relaxed);
    RADIAL_MENU_SELECTION.store(255, Ordering::Relaxed);
}

/// Get radial menu data for rendering
pub fn get_radial_menu_data() -> Option<RadialMenuData> {
    if !is_bomb_picker_open() {
        return None;
    }

    let items = get_bomb_picker_items();
    if items.is_empty() {
        return None;
    }

    Some(RadialMenuData {
        items: items.into_iter().enumerate().map(|(i, item)| RadialMenuItem {
            index: i as u8,
            bomb_type: item.bomb_type,
            name: item.name,
            count: item.count,
            tier: item.tier,
            is_nuclear: item.is_nuclear,
            is_guided: item.is_guided,
            model: item.model,
            angle: (i as f32 * 360.0 / 12.0) - 90.0, // Start at top
        }).collect(),
        selected: get_radial_selection(),
        center_x: 0.5, // Normalized screen coordinates
        center_y: 0.5,
        radius: 0.3,
    })
}

/// Radial menu data for rendering
#[derive(Debug, Clone)]
pub struct RadialMenuData {
    pub items: Vec<RadialMenuItem>,
    pub selected: Option<u8>,
    pub center_x: f32,
    pub center_y: f32,
    pub radius: f32,
}

#[derive(Debug, Clone)]
pub struct RadialMenuItem {
    pub index: u8,
    pub bomb_type: BombType,
    pub name: &'static str,
    pub count: usize,
    pub tier: String,
    pub is_nuclear: bool,
    pub is_guided: bool,
    pub model: &'static str,
    pub angle: f32,
}

/// Get tier color for UI
pub fn get_tier_color(tier: &str) -> [f32; 4] {
    match tier {
        "Light" => [0.2, 0.8, 0.2, 1.0],      // Green
        "Medium" => [0.2, 0.6, 1.0, 1.0],     // Blue
        "Heavy" => [1.0, 0.6, 0.2, 1.0],      // Orange
        "Nuclear" => [1.0, 0.2, 0.2, 1.0],    // Red
        _ => [0.8, 0.8, 0.8, 1.0],            // Gray
    }
}

/// Settings UI
pub fn toggle_settings() {
    SETTINGS_OPEN.store(!SETTINGS_OPEN.load(Ordering::Relaxed), Ordering::Relaxed);
}

pub fn is_settings_open() -> bool {
    SETTINGS_OPEN.load(Ordering::Relaxed)
}

pub fn get_settings_ui_data() -> SettingsUIData {
    with_settings(|settings| {
        SettingsUIData {
            physics_realism: settings.physics_realism,
            damage_enabled: settings.damage_enabled,
            nuke_effects: settings.nuke_effects,
            fallout_enabled: settings.fallout_enabled,
            camera_shake: settings.camera_shake,
            g_force_effects: settings.g_force_effects,
            auto_level_assist: settings.auto_level_assist,
            bomb_trajectory_preview: settings.bomb_trajectory_preview,
            friendly_fire: settings.friendly_fire,
            unlimited_ammo: settings.unlimited_ammo,
        }
    })
}

pub fn update_setting(key: &str, value: bool) {
    with_settings(|settings| {
        match key {
            "damage_enabled" => settings.damage_enabled = value,
            "nuke_effects" => settings.nuke_effects = value,
            "fallout_enabled" => settings.fallout_enabled = value,
            "camera_shake" => settings.camera_shake = value,
            "g_force_effects" => settings.g_force_effects = value,
            "auto_level_assist" => settings.auto_level_assist = value,
            "bomb_trajectory_preview" => settings.bomb_trajectory_preview = value,
            "friendly_fire" => settings.friendly_fire = value,
            "unlimited_ammo" => settings.unlimited_ammo = value,
            _ => {}
        }
    });
}

pub fn update_physics_realism(value: PhysicsRealism) {
    with_settings(|settings| {
        settings.physics_realism = value;
    });
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

/// Key binding display names
pub fn get_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding { action: "flight_mode", key: "F", description: "Enter/Exit flight mode" },
        KeyBinding { action: "bomb_picker", key: "B", description: "Open bomb picker (hold)" },
        KeyBinding { action: "bomb_release", key: "SPACE", description: "Release selected bomb" },
        KeyBinding { action: "throttle_up", key: "W / UP", description: "Increase throttle" },
        KeyBinding { action: "throttle_down", key: "S / DOWN", description: "Decrease throttle" },
        KeyBinding { action: "pitch_up", key: "MOUSE DOWN", description: "Pitch up" },
        KeyBinding { action: "pitch_down", key: "MOUSE UP", description: "Pitch down" },
        KeyBinding { action: "roll_left", key: "MOUSE LEFT / Q", description: "Roll left" },
        KeyBinding { action: "roll_right", key: "MOUSE RIGHT / E", description: "Roll right" },
        KeyBinding { action: "yaw_left", key: "A", description: "Yaw left (rudder)" },
        KeyBinding { action: "yaw_right", key: "D", description: "Yaw right (rudder)" },
        KeyBinding { action: "flaps_up", key: "F", description: "Flaps up" },
        KeyBinding { action: "flaps_down", key: "V", description: "Flaps down" },
        KeyBinding { action: "gear_toggle", key: "G", description: "Toggle landing gear" },
        KeyBinding { action: "bomb_bay_toggle", key: "N", description: "Toggle bomb bay doors" },
        KeyBinding { action: "autopilot_toggle", key: "AP", description: "Toggle autopilot" },
        KeyBinding { action: "camera_cycle", key: "C", description: "Cycle camera mode" },
        KeyBinding { action: "eject", key: "X", description: "Eject" },
        KeyBinding { action: "trim_pitch_up", key: "PAGE UP", description: "Trim pitch up" },
        KeyBinding { action: "trim_pitch_down", key: "PAGE DOWN", description: "Trim pitch down" },
        KeyBinding { action: "trim_roll_left", key: "HOME", description: "Trim roll left" },
        KeyBinding { action: "trim_roll_right", key: "END", description: "Trim roll right" },
    ]
}

#[derive(Debug, Clone)]
pub struct KeyBinding {
    pub action: &'static str,
    pub key: &'static str,
    pub description: &'static str,
}

/// HUD element visibility toggles
pub fn get_hud_elements() -> Vec<HudElement> {
    vec![
        HudElement { id: "airspeed", name: "Airspeed Tape", visible: true, position: HudPosition::Left },
        HudElement { id: "altitude", name: "Altitude Tape", visible: true, position: HudPosition::Right },
        HudElement { id: "attitude", name: "Attitude Indicator", visible: true, position: HudPosition::Center },
        HudElement { id: "heading", name: "Heading Tape", visible: true, position: HudPosition::Bottom },
        HudElement { id: "vertical_speed", name: "Vertical Speed", visible: true, position: HudPosition::Right },
        HudElement { id: "throttle", name: "Throttle Indicator", visible: true, position: HudPosition::Left },
        HudElement { id: "fuel", name: "Fuel Gauge", visible: true, position: HudPosition::Left },
        HudElement { id: "engine", name: "Engine Instruments", visible: true, position: HudPosition::Left },
        HudElement { id: "bomb_bay", name: "Bomb Bay Status", visible: true, position: HudPosition::Bottom },
        HudElement { id: "selected_bomb", name: "Selected Bomb", visible: true, position: HudPosition::Bottom },
        HudElement { id: "trajectory", name: "Bomb Trajectory Preview", visible: true, position: HudPosition::Center },
        HudElement { id: "release_cue", name: "Release Cue", visible: true, position: HudPosition::Center },
        HudElement { id: "target", name: "Target Info", visible: true, position: HudPosition::Top },
        HudElement { id: "damage", name: "Damage Indicators", visible: true, position: HudPosition::Top },
        HudElement { id: "g_force", name: "G-Force Meter", visible: true, position: HudPosition::Right },
        HudElement { id: "aoa", name: "Angle of Attack", visible: true, position: HudPosition::Center },
        HudElement { id: "autopilot", name: "Autopilot Status", visible: true, position: HudPosition::Top },
    ]
}

#[derive(Debug, Clone)]
pub struct HudElement {
    pub id: &'static str,
    pub name: &'static str,
    pub visible: bool,
    pub position: HudPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudPosition {
    Top,
    Bottom,
    Left,
    Right,
    Center,
}

/// Radial menu render data for Horizon's UI system
pub fn render_radial_menu(ui: &mut dyn UiRenderer) {
    if let Some(data) = get_radial_menu_data() {
        let (screen_w, screen_h) = ui.screen_size();
        let cx = data.center_x * screen_w;
        let cy = data.center_y * screen_h;
        let radius = data.radius * screen_h.min(screen_w);

        // Draw background ring
        ui.draw_circle(cx, cy, radius + 20.0, [0.0, 0.0, 0.0, 0.7], true);
        ui.draw_circle(cx, cy, radius - 60.0, [0.0, 0.0, 0.0, 0.7], true);

        // Draw segments
        for item in &data.items {
            let angle_rad = item.angle.to_radians();
            let inner_r = radius - 60.0;
            let outer_r = radius + 20.0;

            let color = if Some(item.index) == data.selected {
                [1.0, 1.0, 1.0, 1.0] // Highlighted
            } else {
                get_tier_color(&item.tier)
            };

            // Draw segment
            let x1 = cx + inner_r * angle_rad.cos();
            let y1 = cy + inner_r * angle_rad.sin();
            let x2 = cx + outer_r * angle_rad.cos();
            let y2 = cy + outer_r * angle_rad.sin();

            let next_angle = ((item.index + 1) as f32 * 30.0 - 90.0).to_radians();
            let x3 = cx + outer_r * next_angle.cos();
            let y3 = cy + outer_r * next_angle.sin();
            let x4 = cx + inner_r * next_angle.cos();
            let y4 = cy + inner_r * next_angle.sin();

            ui.draw_polygon(&[(x1, y1), (x2, y2), (x3, y3), (x4, y4)], color);

            // Draw icon/model preview in segment center
            let mid_angle = (item.angle + 15.0).to_radians();
            let mid_r = (inner_r + outer_r) / 2.0;
            let ix = cx + mid_r * mid_angle.cos();
            let iy = cy + mid_r * mid_angle.sin();

            // Would draw bomb model/icon here
            ui.draw_text(&item.name, ix - 50.0, iy - 10.0, 1.0, [1.0, 1.0, 1.0, 1.0]);
            ui.draw_text(&format!("x{}", item.count), ix - 20.0, iy + 10.0, 0.8, [0.8, 0.8, 0.8, 1.0]);
        }

        // Draw center info
        if let Some(sel) = data.selected {
            if let Some(item) = data.items.iter().find(|i| i.index == sel) {
                ui.draw_text(&format!("SELECTED: {}", item.name), cx - 100.0, cy - 80.0, 1.2, [1.0, 1.0, 0.0, 1.0]);
                ui.draw_text(&format!("Tier: {} | Weight: {:.0} kg", item.tier, item.bomb_type.weight_kg()), cx - 120.0, cy - 55.0, 1.0, [0.8, 0.8, 0.8, 1.0]);
                if item.is_nuclear {
                    ui.draw_text("⚠ NUCLEAR WEAPON ⚠", cx - 100.0, cy - 30.0, 1.0, [1.0, 0.0, 0.0, 1.0]);
                }
                if item.is_guided {
                    ui.draw_text("GUIDED", cx - 40.0, cy - 30.0, 1.0, [0.0, 1.0, 1.0, 1.0]);
                }
                ui.draw_text("RELEASE TO CONFIRM", cx - 100.0, cy + 40.0, 1.0, [0.5, 1.0, 0.5, 1.0]);
            }
        }
    }
}

/// UI renderer trait (Horizon provides implementation)
trait UiRenderer {
    fn screen_size(&self) -> (f32, f32);
    fn draw_circle(&mut self, x: f32, y: f32, r: f32, color: [f32; 4], filled: bool);
    fn draw_polygon(&mut self, points: &[(f32, f32)], color: [f32; 4]);
    fn draw_text(&mut self, text: &str, x: f32, y: f32, scale: f32, color: [f32; 4]);
    fn draw_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]);
}