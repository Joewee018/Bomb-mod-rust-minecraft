//! Bomber Mod Key Bindings
//!
//! Handles custom key bindings for flight mode, bomb picker, and controls.

use crate::flight::{init_flight_session, exit_flight_mode, is_in_flight_mode, FlightInput};
use crate::ui::{toggle_bomb_picker, cancel_radial_menu, update_radial_selection, confirm_radial_selection};
use crate::plane::get_player_plane_id;
use crate::bomber_runtime::PlaneType;

/// Called when a key/mouse button goes down or up
pub fn native_key(key: &str, pressed: bool) {
    // Global keys (work outside flight mode)
    if !pressed {
        return;
    }

    match key {
        "F" => {
            // Toggle flight mode
            if is_in_flight_mode() {
                exit_flight_mode();
            } else {
                // Would need player_id from context
                // init_flight_session(player_id);
            }
        }
        "B" => {
            // Bomb picker is handled via key hold/release
            // This is just the toggle for non-flight mode
            toggle_bomb_picker();
        }
        _ => {}
    }
}

/// Called for key hold/release (for bomb picker radial menu)
pub fn native_key_hold(key: &str, held: bool) {
    if key == "B" {
        if held {
            toggle_bomb_picker();
        } else {
            // Confirm selection on release
            if let Some(bomb_type) = confirm_radial_selection() {
                // Set selected bomb on player's plane
                select_bomb_on_plane(bomb_type);
            } else {
                cancel_radial_menu();
            }
        }
    }
}

/// Called for mouse movement (for radial menu selection)
pub fn native_mouse_move(dx: f32, dy: f32) {
    if crate::ui::is_bomb_picker_open() {
        // Convert mouse position to angle for radial menu
        let angle = dy.atan2(dx).to_degrees();
        update_radial_selection(angle);
    }
}

/// Called for mouse scroll (throttle control in flight mode)
pub fn native_mouse_scroll(delta: f32) {
    if is_in_flight_mode() {
        // Would send throttle_delta to flight input
    }
}

/// Called for gamepad/joystick input
pub fn native_gamepad_input(
    left_stick_x: f32,
    left_stick_y: f32,
    right_stick_x: f32,
    right_stick_y: f32,
    left_trigger: f32,
    right_trigger: f32,
    buttons: u32,
) {
    if !is_in_flight_mode() {
        return;
    }

    let mut input = FlightInput::default();

    // Left stick: pitch/roll
    input.pitch = -left_stick_y;
    input.roll = left_stick_x;

    // Right stick: yaw/camera
    // Would map to yaw or camera control

    // Triggers: throttle
    input.throttle_delta = right_trigger - left_trigger;

    // Buttons
    const BTN_A: u32 = 1 << 0;      // Bomb release
    const BTN_B: u32 = 1 << 1;      // Bomb picker
    const BTN_X: u32 = 1 << 2;      // Eject
    const BTN_Y: u32 = 1 << 3;      // Autopilot
    const BTN_LB: u32 = 1 << 4;     // Flaps down
    const BTN_RB: u32 = 1 << 5;     // Flaps up
    const BTN_LT: u32 = 1 << 6;     // Gear
    const BTN_RT: u32 = 1 << 7;     // Bomb bay
    const BTN_BACK: u32 = 1 << 8;   // Camera cycle
    const BTN_START: u32 = 1 << 9;  // Pause/menu

    if buttons & BTN_A != 0 {
        input.bomb_release_pressed = true;
    }
    if buttons & BTN_B != 0 {
        input.bomb_picker_toggled = true;
    }
    if buttons & BTN_X != 0 {
        input.eject_pressed = true;
    }
    if buttons & BTN_Y != 0 {
        input.autopilot_toggle = true;
    }
    if buttons & BTN_LB != 0 {
        input.flaps_delta = -1.0;
    }
    if buttons & BTN_RB != 0 {
        input.flaps_delta = 1.0;
    }
    if buttons & BTN_LT != 0 {
        input.gear_toggle = true;
    }
    if buttons & BTN_RT != 0 {
        input.bomb_bay_toggle = true;
    }
    if buttons & BTN_BACK != 0 {
        input.camera_mode_cycle = true;
    }

    // Apply input to flight session
    crate::flight::update_flight_session(0.05, &input);
}

/// Select bomb on player's plane
fn select_bomb_on_plane(bomb_type: crate::bomber_runtime::BombType) {
    let plane_id = get_player_plane_id();
    if plane_id == 0 {
        return;
    }

    crate::plane::with_plane_simulation(|sim| {
        if let Some(plane) = sim.get_plane_mut(plane_id) {
            // Find bomb of this type
            for (i, (t, _)) in plane.bombs.iter().enumerate() {
                if *t == bomb_type {
                    plane.selected_bomb = Some(i);
                    break;
                }
            }
        }
    });
}

/// Get default key bindings for display
pub fn get_default_bindings() -> Vec<KeyBindingInfo> {
    vec![
        KeyBindingInfo { action: "flight_mode", key: "F", category: "Flight", description: "Enter/Exit first-person flight mode" },
        KeyBindingInfo { action: "bomb_picker", key: "B (hold)", category: "Weapons", description: "Open bomb picker radial menu" },
        KeyBindingInfo { action: "bomb_release", key: "SPACE", category: "Weapons", description: "Release selected bomb" },
        KeyBindingInfo { action: "throttle_up", key: "W / UP", category: "Flight", description: "Increase throttle" },
        KeyBindingInfo { action: "throttle_down", key: "S / DOWN", category: "Flight", description: "Decrease throttle" },
        KeyBindingInfo { action: "pitch_up", key: "Mouse Down", category: "Flight", description: "Pitch nose up" },
        KeyBindingInfo { action: "pitch_down", key: "Mouse Up", category: "Flight", description: "Pitch nose down" },
        KeyBindingInfo { action: "roll_left", key: "Mouse Left / Q", category: "Flight", description: "Roll left" },
        KeyBindingInfo { action: "roll_right", key: "Mouse Right / E", category: "Flight", description: "Roll right" },
        KeyBindingInfo { action: "yaw_left", key: "A", category: "Flight", description: "Yaw left (rudder)" },
        KeyBindingInfo { action: "yaw_right", key: "D", category: "Flight", description: "Yaw right (rudder)" },
        KeyBindingInfo { action: "flaps_up", key: "F", category: "Flight", description: "Retract flaps" },
        KeyBindingInfo { action: "flaps_down", key: "V", category: "Flight", description: "Extend flaps" },
        KeyBindingInfo { action: "gear_toggle", key: "G", category: "Flight", description: "Toggle landing gear" },
        KeyBindingInfo { action: "bomb_bay_toggle", key: "N", category: "Weapons", description: "Toggle bomb bay doors" },
        KeyBindingInfo { action: "autopilot_toggle", key: "AP", category: "Flight", description: "Toggle autopilot" },
        KeyBindingInfo { action: "camera_cycle", key: "C", category: "Camera", description: "Cycle camera mode" },
        KeyBindingInfo { action: "eject", key: "X", category: "Emergency", description: "Eject from aircraft" },
        KeyBindingInfo { action: "trim_pitch_up", key: "Page Up", category: "Trim", description: "Trim pitch up" },
        KeyBindingInfo { action: "trim_pitch_down", key: "Page Down", category: "Trim", description: "Trim pitch down" },
        KeyBindingInfo { action: "trim_roll_left", key: "Home", category: "Trim", description: "Trim roll left" },
        KeyBindingInfo { action: "trim_roll_right", key: "End", category: "Trim", description: "Trim roll right" },
    ]
}

#[derive(Debug, Clone)]
pub struct KeyBindingInfo {
    pub action: &'static str,
    pub key: &'static str,
    pub category: &'static str,
    pub description: &'static str,
}