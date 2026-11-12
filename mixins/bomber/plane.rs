//! Bomber Mod Plane System
//!
//! Plane entity management, spawning, flight physics, and player interaction.

use crate::bomber_runtime::{
    BombType, PlaneType, PlanePhysics, Vec3, UnitQuaternion,
    with_plane_simulation, next_plane_id, get_plane_model_name,
    get_plane_max_fuel, get_plane_max_thrust, get_plane_wing_area,
    get_plane_drag_coeff, lift_coefficient,
};
use crate::physics::{air_density_at_altitude, dynamic_pressure, GRAVITY, SEA_LEVEL_AIR_DENSITY};
use nalgebra::{Vector3, Vector2, UnitQuaternion as NalgebraUnitQuaternion};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static PLAYER_PLANE_ID: AtomicU32 = AtomicU32::new(0);
static FLIGHT_MODE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Called when a player joins
pub fn native_player_join(player_id: u32) {
    // Could spawn a default plane for the player
}

/// Called when a player leaves
pub fn native_player_leave(player_id: u32) {
    // Clean up player's plane if they had one
    let plane_id = PLAYER_PLANE_ID.load(Ordering::Relaxed);
    if plane_id != 0 {
        with_plane_simulation(|sim| {
            sim.remove_plane(plane_id);
        });
        PLAYER_PLANE_ID.store(0, Ordering::Relaxed);
    }
}

/// Spawn a plane for a player
pub fn spawn_player_plane(player_id: u32, plane_type: PlaneType, position: Vec3) -> u32 {
    let plane_id = next_plane_id();
    with_plane_simulation(|sim| {
        let mut plane = PlanePhysics::new(plane_id, plane_type, position, Some(player_id));

        // Initialize with some default loadout
        initialize_default_loadout(&mut plane);

        sim.planes.insert(plane_id, plane);
    });

    PLAYER_PLANE_ID.store(plane_id, Ordering::Relaxed);
    plane_id
}

/// Initialize default bomb loadout for a plane
fn initialize_default_loadout(plane: &mut PlanePhysics) {
    let compatible = plane.plane_type.compatible_bombs();
    if compatible.is_empty() {
        return;
    }

    // Load a mix of bombs appropriate for the plane's era/capability
    let loadout: Vec<BombType> = match plane.plane_type {
        PlaneType::B17G | PlaneType::B24 => vec![
            BombType::Mk84, BombType::Mk84, BombType::Mk83, BombType::Mk83,
            BombType::Mk82, BombType::Mk82, BombType::Mk81, BombType::Mk81,
        ],
        PlaneType::B29 => vec![
            BombType::Mk84, BombType::Mk84, BombType::Mk84, BombType::Mk84,
            BombType::Mk83, BombType::Mk83, BombType::Mk82, BombType::Mk82,
            BombType::LittleBoy, BombType::FatMan,
        ],
        PlaneType::B52H => vec![
            BombType::GBU31, BombType::GBU31, BombType::GBU31, BombType::GBU31,
            BombType::GBU31, BombType::GBU31, BombType::GBU31, BombType::GBU31,
            BombType::Mk84, BombType::Mk84, BombType::Mk84, BombType::Mk84,
            BombType::BLU109, BombType::BLU109,
            BombType::GBU43, // One MOAB
        ],
        PlaneType::B1B => vec![
            BombType::GBU31, BombType::GBU31, BombType::GBU31, BombType::GBU31,
            BombType::GBU31, BombType::GBU31,
            BombType::Mk84, BombType::Mk84, BombType::Mk84, BombType::Mk84,
            BombType::BLU109, BombType::GBU28,
        ],
        PlaneType::B2 => vec![
            BombType::GBU31, BombType::GBU31, BombType::GBU31, BombType::GBU31,
            BombType::GBU31, BombType::GBU31, BombType::GBU31, BombType::GBU31,
            BombType::GBU28, BombType::GBU28,
            BombType::BLU109, BombType::BLU109,
            BombType::GBU43, // One MOAB
        ],
    };

    for bomb_type in loadout {
        if plane.bombs.len() >= plane.plane_type.bomb_capacity() {
            break;
        }
        if plane.plane_type.compatible_bombs().contains(&bomb_type) {
            plane.load_bomb(bomb_type);
        }
    }

    plane.select_next_bomb();
}

/// Enter first-person flight mode
pub fn enter_flight_mode(player_id: u32) -> bool {
    let plane_id = PLAYER_PLANE_ID.load(Ordering::Relaxed);
    if plane_id == 0 {
        return false;
    }

    with_plane_simulation(|sim| {
        if let Some(plane) = sim.get_plane_mut(plane_id) {
            if plane.player_id == Some(player_id) {
                plane.autopilot.engaged = false;
                FLIGHT_MODE_ACTIVE.store(true, Ordering::Relaxed);
                // Flight session creation handled in flight.rs
                return true;
            }
        }
    });
    false
}

/// Exit flight mode
pub fn exit_flight_mode() {
    FLIGHT_MODE_ACTIVE.store(false, Ordering::Relaxed);
    // Flight session cleanup handled in flight.rs
}

/// Check if flight mode is active
pub fn is_flight_mode_active() -> bool {
    FLIGHT_MODE_ACTIVE.load(Ordering::Relaxed)
}

/// Get player's current plane ID
pub fn get_player_plane_id() -> u32 {
    PLAYER_PLANE_ID.load(Ordering::Relaxed)
}

/// Update plane physics (called from native_tick)
pub fn update_plane_physics(plane: &mut PlanePhysics, dt: f32) {
    if plane.player_id.is_some() && is_flight_mode_active() {
        // Player controlled - physics handled in flight.rs with input
        return;
    }

    // AI controlled physics
    update_ai_physics(plane, dt);
}

/// AI physics for non-player planes
fn update_ai_physics(plane: &mut PlanePhysics, dt: f32) {
    use crate::physics::{get_plane_max_thrust, get_plane_wing_area, get_plane_drag_coeff};

    let mass = crate::physics::get_plane_mass(plane.plane_type);
    let max_thrust = get_plane_max_thrust(plane.plane_type);
    let wing_area = get_plane_wing_area(plane.plane_type);
    let drag_coeff = get_plane_drag_coeff(plane.plane_type);

    let forward = plane.forward();
    let up = plane.up();
    let right = plane.right();

    let velocity = plane.velocity;
    let speed = velocity.norm();
    let altitude = plane.altitude();

    // Thrust
    let thrust_force = forward * max_thrust * plane.throttle;

    // Drag
    let density = air_density_at_altitude(altitude);
    let q = dynamic_pressure(density, speed);
    let drag_force = -velocity.normalize() * q * wing_area * drag_coeff * (1.0 + plane.flap_setting * 0.5);

    // Lift
    let aoa = angle_of_attack(plane, velocity);
    let cl = lift_coefficient(aoa.to_degrees(), speed / 340.0);
    let lift_force = up * q * wing_area * cl;

    // Gravity
    let gravity_force = Vector3::new(0.0, -GRAVITY * mass, 0.0);

    // Net force
    let net_force = thrust_force + drag_force + lift_force + gravity_force;

    // Acceleration
    let acceleration = net_force / mass;

    // Update velocity
    plane.velocity += acceleration * dt;

    // Update angular velocity based on control inputs (AI)
    if plane.autopilot.engaged {
        update_autopilot(plane, dt);
    } else {
        // Simple stability
        plane.angular_velocity *= 0.95;
    }

    // Update orientation
    let angular_vel = plane.angular_velocity;
    if angular_vel.norm() > 0.001 {
        let delta_rot = NalgebraUnitQuaternion::from_scaled_axis(angular_vel * dt);
        plane.rotation = delta_rot * plane.rotation;
    }

    // Update position
    plane.position += plane.velocity * dt;

    // Fuel consumption
    let fuel_burn = max_thrust * plane.throttle * 0.000001; // kg per newton-second
    plane.fuel = (plane.fuel - fuel_burn * dt).max(0.0);

    // Keep above ground
    if plane.position.y < 50.0 && plane.velocity.y < 0.0 {
        plane.position.y = 50.0;
        plane.velocity.y = 0.0;
    }
}

/// Calculate angle of attack
fn angle_of_attack(plane: &PlanePhysics, velocity: Vector3<f32>) -> f32 {
    if velocity.norm() < 1.0 {
        return 0.0;
    }

    let forward = plane.forward();
    let vel_normalized = velocity.normalize();

    // Angle between forward vector and velocity vector in vertical plane
    let forward_horizontal = Vector3::new(forward.x, 0.0, forward.z).normalize();
    let vel_horizontal = Vector3::new(velocity.x, 0.0, velocity.z).normalize();

    let dot = forward.dot(&vel_normalized).clamp(-1.0, 1.0);
    dot.acos().copysign(forward.y - vel_normalized.y)
}

/// Update autopilot
fn update_autopilot(plane: &mut PlanePhysics, dt: f32) {
    match plane.autopilot.mode {
        crate::bomber_runtime::AutopilotMode::Level => {
            // Level wings
            let roll = plane.roll();
            plane.angular_velocity.x = -roll * 2.0;

            // Level pitch
            let pitch = plane.pitch();
            plane.angular_velocity.y = -pitch * 1.5;
        }
        crate::bomber_runtime::AutopilotMode::AltitudeHold => {
            let alt_error = plane.autopilot.target_altitude - plane.altitude();
            let pitch = plane.pitch();

            // PID-like control
            let target_pitch = (alt_error * 0.01).clamp(-0.3, 0.3);
            plane.angular_velocity.y = (target_pitch - pitch) * 3.0;

            // Level roll
            let roll = plane.roll();
            plane.angular_velocity.x = -roll * 2.0;
        }
        crate::bomber_runtime::AutopilotMode::HeadingHold => {
            let heading_error = angle_diff(plane.yaw(), plane.autopilot.target_heading);
            plane.angular_velocity.z = -heading_error * 1.0;

            // Level pitch and roll
            plane.angular_velocity.x = -plane.roll() * 2.0;
            plane.angular_velocity.y = -plane.pitch() * 1.5;
        }
        crate::bomber_runtime::AutopilotMode::AltitudeHeading => {
            // Combine altitude and heading hold
            let alt_error = plane.autopilot.target_altitude - plane.altitude();
            let pitch = plane.pitch();
            let target_pitch = (alt_error * 0.01).clamp(-0.3, 0.3);
            plane.angular_velocity.y = (target_pitch - pitch) * 3.0;

            let heading_error = angle_diff(plane.yaw(), plane.autopilot.target_heading);
            plane.angular_velocity.z = -heading_error * 1.0;

            let roll = plane.roll();
            plane.angular_velocity.x = -roll * 2.0;
        }
        crate::bomber_runtime::AutopilotMode::AutoBomb => {
            // Auto bomb run - maintain level flight at target altitude/speed
            let alt_error = plane.autopilot.target_altitude - plane.altitude();
            let pitch = plane.pitch();
            let target_pitch = (alt_error * 0.01).clamp(-0.2, 0.2);
            plane.angular_velocity.y = (target_pitch - pitch) * 3.0;

            let speed_error = plane.autopilot.target_speed - plane.speed();
            plane.throttle = (plane.throttle + speed_error * 0.001).clamp(0.3, 0.9);

            let heading_error = angle_diff(plane.yaw(), plane.autopilot.target_heading);
            plane.angular_velocity.z = -heading_error * 1.0;

            let roll = plane.roll();
            plane.angular_velocity.x = -roll * 2.0;

            // Check if over target and release bomb
            if alt_error.abs() < 10.0 && speed_error.abs() < 5.0 {
                // Would trigger bomb release here
            }
        }
        crate::bomber_runtime::AutopilotMode::Off => {}
    }
}

/// Angle difference normalized to [-π, π]
fn angle_diff(a: f32, b: f32) -> f32 {
    let diff = b - a;
    if diff > std::f32::consts::PI {
        diff - 2.0 * std::f32::consts::PI
    } else if diff < -std::f32::consts::PI {
        diff + 2.0 * std::f32::consts::PI
    } else {
        diff
    }
}

/// Command to spawn a plane
pub fn cmd_spawn_plane(args: &[&str]) -> String {
    if args.is_empty() {
        return "Usage: /bomber spawn <plane_type> [x] [y] [z]".to_string();
    }

    let plane_type = match args[0].to_lowercase().as_str() {
        "b17" | "b17g" => PlaneType::B17G,
        "b24" | "liberator" => PlaneType::B24,
        "b29" | "superfortress" => PlaneType::B29,
        "b52" | "b52h" | "stratofortress" => PlaneType::B52H,
        "b1" | "b1b" | "lancer" => PlaneType::B1B,
        "b2" | "spirit" => PlaneType::B2,
        _ => return format!("Unknown plane type: {}", args[0]),
    };

    let pos = if args.len() >= 4 {
        Vec3::new(
            args[1].parse().unwrap_or(0.0),
            args[2].parse().unwrap_or(1000.0),
            args[3].parse().unwrap_or(0.0),
        )
    } else {
        Vec3::new(0.0, 1000.0, 0.0)
    };

    let plane_id = next_plane_id();
    with_plane_simulation(|sim| {
        let mut plane = PlanePhysics::new(plane_id, plane_type, pos, None);
        initialize_default_loadout(&mut plane);
        plane.autopilot.engaged = true;
        plane.autopilot.mode = crate::bomber_runtime::AutopilotMode::AltitudeHeading;
        plane.autopilot.target_altitude = 1000.0;
        plane.autopilot.target_heading = 0.0;
        plane.autopilot.target_speed = plane.plane_type.max_speed_mps() * 0.7;
        plane.throttle = 0.7;
        sim.planes.insert(plane_id, plane);
    });

    format!("Spawned {} at ({:.0}, {:.0}, {:.0})", plane_type.name(), pos.x, pos.y, pos.z)
}

/// Command to list planes
pub fn cmd_list_planes(_args: &[&str]) -> String {
    with_plane_simulation(|sim| {
        let planes = sim.get_active_planes();
        if planes.is_empty() {
            return "No active planes".to_string();
        }

        let mut result = String::from("Active planes:\n");
        for plane in planes {
            result.push_str(&format!(
                "  #{:04} {} | Pos: ({:.0}, {:.0}, {:.0}) | Speed: {:.1} m/s | Alt: {:.0}m | Bombs: {}/{}\n",
                plane.id,
                plane.plane_type.name(),
                plane.position.x, plane.position.y, plane.position.z,
                plane.speed(),
                plane.altitude(),
                plane.bombs.len(),
                plane.plane_type.bomb_capacity()
            ));
        }
        result
    })
}

/// Command to give bombs to player's plane
pub fn cmd_give_bomb(args: &[&str]) -> String {
    if args.is_empty() {
        return "Usage: /bomber give <bomb_type> [count]".to_string();
    }

    let bomb_type = match args[0].to_lowercase().as_str() {
        "mk81" => BombType::Mk81,
        "mk82" => BombType::Mk82,
        "mk83" => BombType::Mk83,
        "mk84" => BombType::Mk84,
        "blu109" => BombType::BLU109,
        "gbu31" | "jdam" => BombType::GBU31,
        "gbu28" => BombType::GBU28,
        "gbu43" | "moab" => BombType::GBU43,
        "grandslam" => BombType::GrandSlam,
        "littleboy" => BombType::LittleBoy,
        "fatman" => BombType::FatMan,
        "tsar" | "tsarbomba" => BombType::TsarBomba,
        _ => return format!("Unknown bomb type: {}", args[0]),
    };

    let count = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1).min(64);

    let plane_id = PLAYER_PLANE_ID.load(Ordering::Relaxed);
    if plane_id == 0 {
        return "No plane spawned for player. Use /bomber spawn first.".to_string();
    }

    let mut added = 0;
    with_plane_simulation(|sim| {
        if let Some(plane) = sim.get_plane_mut(plane_id) {
            if !plane.plane_type.compatible_bombs().contains(&bomb_type) {
                return format!("{} cannot carry {}", plane.plane_type.name(), bomb_type.name());
            }

            for _ in 0..count {
                if plane.bombs.len() >= plane.plane_type.bomb_capacity() {
                    break;
                }
                if plane.load_bomb(bomb_type) {
                    added += 1;
                }
            }
        }
    });

    if added > 0 {
        format!("Added {}x {} to plane", added, bomb_type.name())
    } else {
        "Bomb bay full or incompatible".to_string()
    }
}

/// Command to clear all planes
pub fn cmd_clear_planes(_args: &[&str]) -> String {
    with_plane_simulation(|sim| {
        let count = sim.planes.len();
        sim.clear();
        format!("Cleared {} planes", count)
    })
}

/// Command to toggle autopilot
pub fn cmd_autopilot(args: &[&str]) -> String {
    let plane_id = PLAYER_PLANE_ID.load(Ordering::Relaxed);
    if plane_id == 0 {
        return "No player plane".to_string();
    }

    let mode = if args.is_empty() {
        "toggle".to_string()
    } else {
        args[0].to_lowercase()
    };

    with_plane_simulation(|sim| {
        if let Some(plane) = sim.get_plane_mut(plane_id) {
            match mode.as_str() {
                "off" => {
                    plane.autopilot.engaged = false;
                    plane.autopilot.mode = crate::bomber_runtime::AutopilotMode::Off;
                    "Autopilot OFF".to_string()
                }
                "level" => {
                    plane.autopilot.engaged = true;
                    plane.autopilot.mode = crate::bomber_runtime::AutopilotMode::Level;
                    "Autopilot: Level flight".to_string()
                }
                "althold" | "alt" => {
                    plane.autopilot.engaged = true;
                    plane.autopilot.mode = crate::bomber_runtime::AutopilotMode::AltitudeHold;
                    plane.autopilot.target_altitude = plane.altitude();
                    "Autopilot: Altitude hold".to_string()
                }
                "heading" | "hdg" => {
                    plane.autopilot.engaged = true;
                    plane.autopilot.mode = crate::bomber_runtime::AutopilotMode::HeadingHold;
                    plane.autopilot.target_heading = plane.yaw();
                    "Autopilot: Heading hold".to_string()
                }
                "auto" | "nav" => {
                    plane.autopilot.engaged = true;
                    plane.autopilot.mode = crate::bomber_runtime::AutopilotMode::AltitudeHeading;
                    plane.autopilot.target_altitude = plane.altitude();
                    plane.autopilot.target_heading = plane.yaw();
                    "Autopilot: Altitude + Heading".to_string()
                }
                "bomb" | "autobomb" => {
                    plane.autopilot.engaged = true;
                    plane.autopilot.mode = crate::bomber_runtime::AutopilotMode::AutoBomb;
                    plane.autopilot.target_altitude = plane.altitude();
                    plane.autopilot.target_heading = plane.yaw();
                    plane.autopilot.target_speed = plane.plane_type.max_speed_mps() * 0.7;
                    "Autopilot: Auto bomb run".to_string()
                }
                "toggle" => {
                    plane.autopilot.engaged = !plane.autopilot.engaged;
                    if plane.autopilot.engaged {
                        plane.autopilot.mode = crate::bomber_runtime::AutopilotMode::AltitudeHeading;
                        plane.autopilot.target_altitude = plane.altitude();
                        plane.autopilot.target_heading = plane.yaw();
                        "Autopilot ON (Altitude + Heading)".to_string()
                    } else {
                        plane.autopilot.mode = crate::bomber_runtime::AutopilotMode::Off;
                        "Autopilot OFF".to_string()
                    }
                }
                _ => "Unknown mode. Use: off, level, alt, heading, auto, bomb, toggle".to_string()
            }
        } else {
            "Player plane not found".to_string()
        }
    })
}

/// Get plane info for HUD
pub fn get_plane_hud_info(plane_id: u32) -> Option<String> {
    with_plane_simulation(|sim| {
        sim.get_plane_mut(plane_id).map(|plane| {
            let ap_mode = if plane.autopilot.engaged {
                format!(" | AP: {:?}", plane.autopilot.mode)
            } else {
                String::new()
            };

            let bomb_info = if let Some(idx) = plane.selected_bomb {
                if idx < plane.bombs.len() {
                    format!(" | Bomb: {}", plane.bombs[idx].0.name())
                } else {
                    String::new()
                }
            } else {
                " | No bomb selected".to_string()
            };

            format!(
                "{} | Pos: ({:.0}, {:.0}, {:.0}) | Vel: {:.1} m/s | Alt: {:.0}m | Fuel: {:.0}%{}{}",
                plane.plane_type.name(),
                plane.position.x, plane.position.y, plane.position.z,
                plane.speed(),
                plane.altitude(),
                plane.fuel / plane.max_fuel * 100.0,
                ap_mode,
                bomb_info
            )
        })
    })
}

/// Damage a plane
pub fn damage_plane(plane_id: u32, component: PlaneComponent, amount: f32) {
    with_plane_simulation(|sim| {
        if let Some(plane) = sim.get_plane_mut(plane_id) {
            let damage = &mut plane.damage;
            match component {
                PlaneComponent::Engine1 => damage.engine_1 = (damage.engine_1 + amount).min(1.0),
                PlaneComponent::Engine2 => damage.engine_2 = (damage.engine_2 + amount).min(1.0),
                PlaneComponent::Engine3 => damage.engine_3 = (damage.engine_3 + amount).min(1.0),
                PlaneComponent::Engine4 => damage.engine_4 = (damage.engine_4 + amount).min(1.0),
                PlaneComponent::LeftWing => damage.left_wing = (damage.left_wing + amount).min(1.0),
                PlaneComponent::RightWing => damage.right_wing = (damage.right_wing + amount).min(1.0),
                PlaneComponent::Tail => damage.tail = (damage.tail + amount).min(1.0),
                PlaneComponent::Fuselage => damage.fuselage = (damage.fuselage + amount).min(1.0),
                PlaneComponent::Hydraulics => damage.hydraulics = (damage.hydraulics + amount).min(1.0),
                PlaneComponent::FuelTank => damage.fuel_leak = (damage.fuel_leak + amount).min(1.0),
                PlaneComponent::Fire => damage.fire = (damage.fire + amount).min(1.0),
            }

            // Apply damage effects
            apply_damage_effects(plane);
        }
    })
}

/// Apply damage effects to flight physics
fn apply_damage_effects(plane: &mut PlanePhysics) {
    let damage = &plane.damage;

    // Engine damage reduces thrust
    let engine_damage = (damage.engine_1 + damage.engine_2 + damage.engine_3 + damage.engine_4) / 4.0;
    // Would affect max_thrust calculation

    // Wing damage reduces lift and increases drag
    let wing_damage = (damage.left_wing + damage.right_wing) / 2.0;
    // Would affect wing_area and drag_coeff

    // Tail damage reduces control authority
    let tail_damage = damage.tail;
    // Would affect angular_velocity response

    // Fire spreads
    if damage.fire > 0.0 {
        // Fire damages nearby components over time
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneComponent {
    Engine1,
    Engine2,
    Engine3,
    Engine4,
    LeftWing,
    RightWing,
    Tail,
    Fuselage,
    Hydraulics,
    FuelTank,
    Fire,
}

/// Command to damage plane (for testing)
pub fn cmd_damage_plane(args: &[&str]) -> String {
    if args.len() < 2 {
        return "Usage: /bomber damage <component> <amount>".to_string();
    }

    let plane_id = PLAYER_PLANE_ID.load(Ordering::Relaxed);
    if plane_id == 0 {
        return "No player plane".to_string();
    }

    let component = match args[0].to_lowercase().as_str() {
        "engine1" => PlaneComponent::Engine1,
        "engine2" => PlaneComponent::Engine2,
        "engine3" => PlaneComponent::Engine3,
        "engine4" => PlaneComponent::Engine4,
        "lwing" | "leftwing" => PlaneComponent::LeftWing,
        "rwing" | "rightwing" => PlaneComponent::RightWing,
        "tail" => PlaneComponent::Tail,
        "fuselage" => PlaneComponent::Fuselage,
        "hydraulics" => PlaneComponent::Hydraulics,
        "fuel" => PlaneComponent::FuelTank,
        "fire" => PlaneComponent::Fire,
        _ => return "Unknown component".to_string(),
    };

    let amount = args[1].parse::<f32>().unwrap_or(0.1).clamp(0.0, 1.0);
    damage_plane(plane_id, component, amount);

    format!("Applied {:.0}% damage to {:?}", amount * 100.0, component)
}

/// Get model name for rendering
pub fn get_plane_model(plane_type: PlaneType) -> &'static str {
    get_plane_model_name(plane_type)
}