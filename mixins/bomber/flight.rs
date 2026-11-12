//! Bomber Mod First-Person Flight Mode
//!
//! Immersive cockpit experience with realistic instruments, HUD, and controls.

use crate::bomber_runtime::{
    BombType, PlaneType, PlanePhysics, Vec3, UnitQuaternion,
    FlightSession, FlightCamera, FlightHUD, FlightControls, CameraMode, HUDMode,
    with_plane_simulation, with_flight_session, with_bomb_simulation,
    is_flight_mode_active, get_player_plane_id, next_bomb_id,
};
use crate::physics::{predict_impact, get_wind_at_position};
use crate::bomb::{spawn_bomb_entity, get_bomb_model};
use nalgebra::{Vector3, Vector2, UnitQuaternion as NalgebraUnitQuaternion};
use std::sync::atomic::{AtomicBool, Ordering};

static FLIGHT_SESSION_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Initialize flight mode for a player
pub fn init_flight_session(player_id: u32) -> bool {
    let plane_id = get_player_plane_id();
    if plane_id == 0 {
        return false;
    }

    let plane_type = with_plane_simulation(|sim| {
        sim.get_plane_mut(plane_id).map(|p| p.plane_type)
    });

    if let Some(plane_type) = plane_type {
        let session = FlightSession::new(plane_id, plane_type);
        // Store session (would use FLIGHT_SESSION from bomber_runtime)
        FLIGHT_SESSION_ACTIVE.store(true, Ordering::Relaxed);
        true
    } else {
        false
    }
}

/// Exit flight mode
pub fn exit_flight_mode() {
    FLIGHT_SESSION_ACTIVE.store(false, Ordering::Relaxed);
    // Cleanup handled elsewhere
}

/// Check if flight mode is active
pub fn is_in_flight_mode() -> bool {
    FLIGHT_SESSION_ACTIVE.load(Ordering::Relaxed)
}

/// Update flight session (called every tick when in flight mode)
pub fn update_flight_session(dt: f32, input: &FlightInput) {
    let plane_id = get_player_plane_id();
    if plane_id == 0 {
        return;
    }

    with_plane_simulation(|sim| {
        if let Some(plane) = sim.get_plane_mut(plane_id) {
            // Apply player input to plane physics
            apply_flight_input(plane, input, dt);

            // Update plane physics with player control
            update_player_plane_physics(plane, dt);

            // Update camera
            update_camera(plane, dt);

            // Update HUD
            update_hud(plane, dt);

            // Handle bomb release
            if input.bomb_release_pressed {
                release_bomb(plane);
            }

            // Handle bomb picker
            if input.bomb_picker_toggled {
                toggle_bomb_picker();
            }

            // Handle bomb selection in picker
            if input.bomb_selected.is_some() {
                select_bomb_type(input.bomb_selected.unwrap());
            }
        }
    });
}

/// Player input state
#[derive(Debug, Clone, Default)]
pub struct FlightInput {
    pub pitch: f32,        // -1.0 to 1.0 (mouse Y / joystick)
    pub roll: f32,         // -1.0 to 1.0 (mouse X / joystick)
    pub yaw: f32,          // -1.0 to 1.0 (rudder pedals / keys)
    pub throttle: f32,     // 0.0 to 1.0
    pub throttle_delta: f32, // For keyboard +/- control
    pub flaps: f32,        // 0.0 to 1.0
    pub flaps_delta: f32,
    pub gear_toggle: bool,
    pub bomb_bay_toggle: bool,
    pub bomb_release_pressed: bool,
    pub bomb_picker_toggled: bool,
    pub bomb_selected: Option<usize>, // Index in bomb picker
    pub camera_mode_cycle: bool,
    pub autopilot_toggle: bool,
    pub autopilot_mode_cycle: bool,
    pub eject_pressed: bool,
    pub trim_pitch: f32,
    pub trim_roll: f32,
    pub trim_yaw: f32,
}

/// Apply player input to plane controls
fn apply_flight_input(plane: &mut PlanePhysics, input: &FlightInput, dt: f32) {
    // Throttle control
    plane.throttle = (plane.throttle + input.throttle_delta * dt).clamp(0.0, 1.0);

    // Flaps
    plane.flap_setting = (plane.flap_setting + input.flaps_delta * dt).clamp(0.0, 1.0);

    // Gear
    if input.gear_toggle {
        plane.gear_down = !plane.gear_down;
    }

    // Bomb bay
    if input.bomb_bay_toggle {
        if plane.bomb_bay_open {
            plane.close_bomb_bay();
        } else {
            plane.open_bomb_bay();
        }
    }

    // Store control inputs for physics
    plane.controls = Some(FlightControls {
        pitch: input.pitch,
        roll: input.roll,
        yaw: input.yaw,
        throttle: plane.throttle,
        flaps: plane.flap_setting,
        gear: plane.gear_down,
        bomb_bay: plane.bomb_bay_open,
        brake: false,
        trim_pitch: plane.trim_pitch + input.trim_pitch * dt,
        trim_roll: plane.trim_roll + input.trim_roll * dt,
        trim_yaw: plane.trim_yaw + input.trim_yaw * dt,
    });
}

/// Update plane physics with player input
fn update_player_plane_physics(plane: &mut PlanePhysics, dt: f32) {
    use crate::physics::{
        get_plane_max_thrust, get_plane_wing_area, get_plane_drag_coeff,
        air_density_at_altitude, dynamic_pressure, GRAVITY,
    };
    use crate::plane::lift_coefficient;

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

    // Get control inputs
    let (pitch_input, roll_input, yaw_input) = if let Some(controls) = &plane.controls {
        (controls.pitch, controls.roll, controls.yaw)
    } else {
        (0.0, 0.0, 0.0)
    };

    // Apply control surface effectiveness based on speed
    let control_effectiveness = (speed / 50.0).min(1.0);
    let max_pitch_rate = 1.5 * control_effectiveness;
    let max_roll_rate = 2.0 * control_effectiveness;
    let max_yaw_rate = 1.0 * control_effectiveness;

    // Target angular velocity from inputs
    let target_pitch_rate = pitch_input * max_pitch_rate;
    let target_roll_rate = roll_input * max_roll_rate;
    let target_yaw_rate = yaw_input * max_yaw_rate;

    // Smooth angular velocity toward target (simulates control response)
    let response_rate = 5.0 * control_effectiveness;
    plane.angular_velocity.x += (target_roll_rate - plane.angular_velocity.x) * response_rate * dt;
    plane.angular_velocity.y += (target_pitch_rate - plane.angular_velocity.y) * response_rate * dt;
    plane.angular_velocity.z += (target_yaw_rate - plane.angular_velocity.z) * response_rate * dt;

    // Apply trim
    plane.angular_velocity.x += plane.trim_roll * dt;
    plane.angular_velocity.y += plane.trim_pitch * dt;
    plane.angular_velocity.z += plane.trim_yaw * dt;

    // Natural stability (damping)
    let damping = 0.5 * control_effectiveness;
    plane.angular_velocity *= 1.0 - damping * dt;

    // Thrust
    let thrust_force = forward * max_thrust * plane.throttle;

    // Drag
    let density = air_density_at_altitude(altitude);
    let q = dynamic_pressure(density, speed);
    let flap_drag = 1.0 + plane.flap_setting * 0.8;
    let gear_drag = if plane.gear_down { 1.15 } else { 1.0 };
    let bomb_bay_drag = if plane.bomb_bay_open { 1.08 } else { 1.0 };
    let drag_force = -velocity.normalize() * q * wing_area * drag_coeff * flap_drag * gear_drag * bomb_bay_drag;

    // Lift
    let aoa = angle_of_attack(plane, velocity);
    let cl = lift_coefficient(aoa.to_degrees(), speed / 340.0);
    let flap_lift = 1.0 + plane.flap_setting * 0.5;
    let lift_force = up * q * wing_area * cl * flap_lift;

    // Gravity
    let gravity_force = Vector3::new(0.0, -GRAVITY * mass, 0.0);

    // Engine torque effect (P-factor)
    let torque = right * max_thrust * plane.throttle * 0.001;

    // Net force
    let net_force = thrust_force + drag_force + lift_force + gravity_force + torque;

    // Acceleration
    let acceleration = net_force / mass;

    // Update velocity
    plane.velocity += acceleration * dt;

    // Update orientation from angular velocity
    let angular_vel = plane.angular_velocity;
    if angular_vel.norm() > 0.001 {
        let delta_rot = NalgebraUnitQuaternion::from_scaled_axis(angular_vel * dt);
        plane.rotation = delta_rot * plane.rotation;
    }

    // Update position
    plane.position += plane.velocity * dt;

    // Fuel consumption
    let fuel_burn = max_thrust * plane.throttle * 0.000001;
    plane.fuel = (plane.fuel - fuel_burn * dt).max(0.0);

    // Stall warning
    if speed < 40.0 && aoa.abs() > 15.0 {
        // Trigger stall warning in HUD
    }

    // Ground collision
    if plane.position.y < 10.0 && plane.velocity.y < 0.0 {
        plane.position.y = 10.0;
        plane.velocity.y = plane.velocity.y.max(0.0);
        // Damage on hard landing
        if plane.velocity.norm() > 10.0 {
            crate::plane::damage_plane(plane.id, crate::plane::PlaneComponent::Fuselage, 0.1);
        }
    }

    // B-1B wing sweep automation
    if plane.plane_type == PlaneType::B1B {
        let target_sweep = if speed > 300.0 { 1.0 } else { 0.0 };
        plane.wing_sweep += (target_sweep - plane.wing_sweep) * dt * 0.5;
    }
}

/// Calculate angle of attack
fn angle_of_attack(plane: &PlanePhysics, velocity: Vector3<f32>) -> f32 {
    if velocity.norm() < 1.0 {
        return 0.0;
    }

    let forward = plane.forward();
    let vel_normalized = velocity.normalize();

    let dot = forward.dot(&vel_normalized).clamp(-1.0, 1.0);
    let angle = dot.acos();

    // Determine sign from vertical component difference
    if forward.y < vel_normalized.y {
        -angle
    } else {
        angle
    }
}

/// Update camera based on plane state
fn update_camera(plane: &PlanePhysics, dt: f32) {
    // Camera logic would update FlightCamera in the session
    // For now, just calculate the ideal camera position
}

/// Update HUD elements
fn update_hud(plane: &PlanePhysics, dt: f32) {
    // HUD logic would update FlightHUD in the session
}

/// Release selected bomb
fn release_bomb(plane: &mut PlanePhysics) {
    if !plane.bomb_bay_open {
        // Auto-open bomb bay
        plane.open_bomb_bay();
        // Bomb will release next tick when bay is open
        return;
    }

    if let Some(bomb_id) = plane.drop_bomb(&mut with_bomb_simulation(|s| s))) {
        // Set guidance target if guided bomb
        if let Some(idx) = plane.selected_bomb {
            if idx < plane.bombs.len() {
                let bomb_type = plane.bombs[idx].0;
                if bomb_type.is_guided() {
                    // Set target to current crosshair position or designated target
                    let target = plane.position + plane.forward() * 5000.0; // Placeholder
                    crate::bomb::set_bomb_target(bomb_id, target);
                }
            }
        }
    }
}

/// Toggle bomb picker UI
fn toggle_bomb_picker() {
    with_flight_session(|session| {
        session.bomb_picker_open = !session.bomb_picker_open;
    });
}

/// Select bomb type from picker
fn select_bomb_type(index: usize) {
    with_plane_simulation(|sim| {
        let plane_id = get_player_plane_id();
        if let Some(plane) = sim.get_plane_mut(plane_id) {
            // Get compatible bombs for this plane
            let compatible: Vec<BombType> = plane.bombs.iter()
                .map(|(t, _)| *t)
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            if index < compatible.len() {
                plane.selected_bomb = Some(index);
                with_flight_session(|session| {
                    session.selected_bomb_type = Some(compatible[index]);
                    session.bomb_picker_open = false;
                });
            }
        }
    });
}

/// Eject sequence
pub fn eject_player() {
    let plane_id = get_player_plane_id();
    if plane_id == 0 {
        return;
    }

    with_plane_simulation(|sim| {
        if let Some(plane) = sim.get_plane_mut(plane_id) {
            // Jettison canopy
            // Fire ejection seat
            // Deploy parachute
            // Remove player from plane
            plane.player_id = None;
        }
    });

    exit_flight_mode();
    // Spawn player as parachutist entity
}

/// Get flight HUD data for rendering
pub fn get_flight_hud_data() -> Option<FlightHUDData> {
    let plane_id = get_player_plane_id();
    if plane_id == 0 {
        return None;
    }

    with_plane_simulation(|sim| {
        sim.get_plane_mut(plane_id).map(|plane| {
            let controls = plane.controls.as_ref();

            FlightHUDData {
                // Flight instruments
                airspeed: plane.speed(),
                groundspeed: plane.velocity.norm(),
                altitude: plane.altitude(),
                vertical_speed: plane.velocity.y,
                heading: plane.yaw().to_degrees(),
                pitch: plane.pitch().to_degrees(),
                roll: plane.roll().to_degrees(),
                aoa: angle_of_attack(plane, plane.velocity).to_degrees(),
                g_force: 1.0, // Would calculate from acceleration

                // Engine instruments
                throttle: plane.throttle,
                fuel_percent: plane.fuel / plane.max_fuel * 100.0,
                engines_ok: plane.damage.engines_operational() == 4,

                // Navigation
                position: plane.position,
                target_waypoint: None, // Would come from mission system

                // Weapon status
                bomb_bay_open: plane.bomb_bay_open,
                selected_bomb: plane.selected_bomb.and_then(|i| {
                    plane.bombs.get(i).map(|(t, _)| t.name())
                }),
                bombs_remaining: plane.bombs.len(),
                bomb_trajectory: None, // Calculated separately

                // Autopilot
                autopilot_mode: if plane.autopilot.engaged {
                    Some(format!("{:?}", plane.autopilot.mode))
                } else {
                    None
                },

                // Damage
                damage_percent: plane.damage.total() * 100.0,
                fire_warning: plane.damage.fire > 0.5,
                fuel_leak: plane.damage.fuel_leak > 0.3,

                // Camera
                camera_mode: CameraMode::Cockpit,
            }
        })
    })
}

/// HUD data structure for rendering
#[derive(Debug, Clone)]
pub struct FlightHUDData {
    pub airspeed: f32,
    pub groundspeed: f32,
    pub altitude: f32,
    pub vertical_speed: f32,
    pub heading: f32,
    pub pitch: f32,
    pub roll: f32,
    pub aoa: f32,
    pub g_force: f32,
    pub throttle: f32,
    pub fuel_percent: f32,
    pub engines_ok: bool,
    pub position: Vec3,
    pub target_waypoint: Option<Vec3>,
    pub bomb_bay_open: bool,
    pub selected_bomb: Option<String>,
    pub bombs_remaining: usize,
    pub bomb_trajectory: Option<Vec<Vec3>>,
    pub autopilot_mode: Option<String>,
    pub damage_percent: f32,
    pub fire_warning: bool,
    pub fuel_leak: bool,
    pub camera_mode: CameraMode,
}

/// Get bomb trajectory preview for HUD
pub fn get_bomb_trajectory_preview() -> Option<Vec<Vec3>> {
    let plane_id = get_player_plane_id();
    if plane_id == 0 {
        return None;
    }

    with_plane_simulation(|sim| {
        sim.get_plane_mut(plane_id).and_then(|plane| {
            plane.selected_bomb.and_then(|idx| {
                plane.bombs.get(idx).map(|(bomb_type, _)| {
                    let release_pos = plane.position + plane.up() * -3.0;
                    let release_vel = plane.velocity + plane.forward() * 5.0;
                    let wind = get_wind_at_position(release_pos);
                    crate::physics::generate_trajectory_points(release_pos, release_vel, *bomb_type, wind, 50)
                })
            })
        })
    })
}

/// Get bomb picker items for UI
pub fn get_bomb_picker_items() -> Vec<BombPickerItem> {
    let plane_id = get_player_plane_id();
    if plane_id == 0 {
        return Vec::new();
    }

    with_plane_simulation(|sim| {
        sim.get_plane_mut(plane_id).map(|plane| {
            let mut items = Vec::new();
            let mut seen = std::collections::HashSet::new();

            for (bomb_type, _) in &plane.bombs {
                if seen.insert(*bomb_type) {
                    items.push(BombPickerItem {
                        bomb_type: *bomb_type,
                        name: bomb_type.name(),
                        count: plane.bombs.iter().filter(|(t, _)| *t == *bomb_type).count(),
                        weight_kg: bomb_type.weight_kg(),
                        tier: format!("{:?}", bomb_type.tier()),
                        is_nuclear: bomb_type.is_nuclear(),
                        is_guided: bomb_type.is_guided(),
                        model: get_bomb_model(*bomb_type),
                    });
                }
            }

            items
        }).unwrap_or_default()
    })
}

/// Bomb picker item for UI
#[derive(Debug, Clone)]
pub struct BombPickerItem {
    pub bomb_type: BombType,
    pub name: &'static str,
    pub count: usize,
    pub weight_kg: f32,
    pub tier: String,
    pub is_nuclear: bool,
    pub is_guided: bool,
    pub model: &'static str,
}

/// HUD render callback (called from Horizon's render pipeline)
pub fn native_hud(batch: &mut dyn HudBatch, corners: &mut dyn HudCorners) {
    if !is_in_flight_mode() {
        return;
    }

    if let Some(hud_data) = get_flight_hud_data() {
        draw_flight_hud(batch, corners, &hud_data);
    }
}

/// Draw flight HUD
fn draw_flight_hud(batch: &mut dyn HudBatch, corners: &mut dyn HudCorners, data: &FlightHUDData) {
    // This would use Horizon's HUD drawing API
    // For now, placeholder

    let _ = (batch, corners, data);
}

/// Trait for HUD drawing (Horizon provides implementation)
trait HudBatch {
    fn draw_text(&mut self, text: &str, x: f32, y: f32, scale: f32, color: [f32; 4]);
    fn draw_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]);
    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, color: [f32; 4], width: f32);
    fn draw_circle(&mut self, x: f32, y: f32, r: f32, color: [f32; 4], filled: bool);
}

trait HudCorners {
    fn screen_size(&self) -> (f32, f32);
}