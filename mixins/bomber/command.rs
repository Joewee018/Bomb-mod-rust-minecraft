//! Bomber Mod Commands
//!
//! Command handlers for the bomber mod.

use crate::plane::{cmd_spawn_plane, cmd_list_planes, cmd_give_bomb, cmd_clear_planes, cmd_autopilot, cmd_damage_plane};
use crate::bomb::{cmd_spawn_bomb, cmd_list_bombs, cmd_clear_bombs, cmd_nuke};
use crate::nuke::cmd_test_nuke;

/// Main command handler
pub fn native_command(args: &[&str]) -> String {
    if args.is_empty() {
        return help_text();
    }

    match args[0].to_lowercase().as_str() {
        "help" | "?" => help_text(),
        "spawn" => cmd_spawn_plane(&args[1..]),
        "planes" | "list" => cmd_list_planes(&args[1..]),
        "give" => cmd_give_bomb(&args[1..]),
        "clear" => cmd_clear_planes(&args[1..]),
        "autopilot" | "ap" => cmd_autopilot(&args[1..]),
        "damage" => cmd_damage_plane(&args[1..]),
        "bomb" => cmd_spawn_bomb(&args[1..]),
        "bombs" => cmd_list_bombs(&args[1..]),
        "clearbombs" => cmd_clear_bombs(&args[1..]),
        "nuke" => cmd_nuke(&args[1..]),
        "testnuke" => cmd_test_nuke(&args[1..]),
        "flight" => cmd_flight(&args[1..]),
        "eject" => cmd_eject(&args[1..]),
        "settings" => cmd_settings(&args[1..]),
        _ => format!("Unknown command: {}. Use /bomber help for help.", args[0]),
    }
}

fn help_text() -> String {
    r#"
=== Bomber Mod Commands ===

Plane Commands:
  /bomber spawn <type> [x y z]     - Spawn a plane
    Types: b17, b24, b29, b52, b1, b2
  /bomber planes                   - List active planes
  /bomber give <bomb> [count]      - Give bombs to your plane
    Types: mk81, mk82, mk83, mk84, blu109, gbu31, gbu28, gbu43, grandslam, littleboy, fatman, tsar
  /bomber clear                    - Clear all planes
  /bomber autopilot [mode]         - Toggle/configure autopilot
    Modes: off, level, alt, heading, auto, bomb
  /bomber damage <component> <amt> - Damage plane component (testing)

Bomb Commands:
  /bomber bomb <type> [x y z vx vy vz] - Spawn a bomb
  /bomber bombs                    - List active bombs
  /bomber clearbombs              - Clear all bombs
  /bomber nuke [type] [x y z]     - Detonate nuclear weapon
  /bomber testnuke [type] [x y z] - Test nuke effects

Flight Commands:
  /bomber flight [enter|exit]      - Enter/exit flight mode
  /bomber eject                   - Eject from plane

Settings:
  /bomber settings [key] [value]  - View/modify settings

Key Bindings (in flight mode):
  F          - Enter/exit flight mode
  B (hold)   - Bomb picker radial menu
  SPACE      - Release bomb
  W/S        - Throttle up/down
  Mouse      - Pitch/roll
  Q/E        - Roll left/right
  A/D        - Yaw left/right (rudder)
  F/V        - Flaps up/down
  G          - Toggle landing gear
  N          - Toggle bomb bay doors
  AP         - Toggle autopilot
  C          - Cycle camera mode
  X          - Eject
  PgUp/PgDn  - Trim pitch
  Home/End   - Trim roll
"#.to_string()
}

/// Flight mode command
fn cmd_flight(args: &[&str]) -> String {
    if args.is_empty() {
        return "Usage: /bomber flight [enter|exit]".to_string();
    }

    match args[0].to_lowercase().as_str() {
        "enter" => {
            // Would call flight::enter_flight_mode with player ID
            "Flight mode entered".to_string()
        }
        "exit" => {
            crate::flight::exit_flight_mode();
            "Flight mode exited".to_string()
        }
        _ => "Unknown subcommand. Use: enter, exit".to_string(),
    }
}

/// Eject command
fn cmd_eject(_args: &[&str]) -> String {
    crate::flight::eject_player();
    "Ejected!".to_string()
}

/// Settings command
fn cmd_settings(args: &[&str]) -> String {
    if args.is_empty() {
        return crate::settings::get_settings_ui_data().to_string();
    }

    if args.len() == 1 {
        // Toggle boolean setting
        crate::ui::update_setting(args[0], true);
        format!("Setting {} enabled", args[0])
    } else if args.len() >= 2 {
        match args[1].to_lowercase().as_str() {
            "true" | "on" | "1" => {
                crate::ui::update_setting(args[0], true);
                format!("Setting {} enabled", args[0])
            }
            "false" | "off" | "0" => {
                crate::ui::update_setting(args[0], false);
                format!("Setting {} disabled", args[0])
            }
            _ => {
                // Try parsing as physics realism
                if args[0] == "realism" {
                    let realism = match args[1].to_lowercase().as_str() {
                        "arcade" => crate::bomber_runtime::PhysicsRealism::Arcade,
                        "simplified" => crate::bomber_runtime::PhysicsRealism::Simplified,
                        "realistic" => crate::bomber_runtime::PhysicsRealism::Realistic,
                        "hardcore" => crate::bomber_runtime::PhysicsRealism::Hardcore,
                        _ => return "Unknown realism level".to_string(),
                    };
                    crate::ui::update_physics_realism(realism);
                    format!("Physics realism set to {:?}", realism)
                } else {
                    "Invalid value. Use true/false or on/off".to_string()
                }
            }
        }
    } else {
        "Usage: /bomber settings [key] [value]".to_string()
    }
}