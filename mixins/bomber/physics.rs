//! Bomber Mod Physics Engine
//!
//! Custom ballistics simulation with realistic aerodynamics for bombs and aircraft.
//! Uses RK4 integration for accurate trajectory prediction.

use crate::bomber_runtime::{BombPhysics, BombType, PlaneType, Vec3, with_bomb_simulation, with_plane_simulation};
use nalgebra::{Vector3, Vector2};
use rand::Rng;
use std::sync::atomic::{AtomicU64, Ordering};

/// Global simulation tick counter
pub static SIMULATION_TICK: AtomicU64 = AtomicU64::new(0);

/// Gravity constant (m/s²)
pub const GRAVITY: f32 = 9.81;

/// Sea level air density (kg/m³)
pub const SEA_LEVEL_AIR_DENSITY: f32 = 1.225;

/// Gas constant for dry air (J/(kg·K))
pub const GAS_CONSTANT: f32 = 287.05;

/// Standard temperature lapse rate (K/m)
pub const LAPSE_RATE: f32 = 0.0065;

/// Sea level temperature (K)
pub const SEA_LEVEL_TEMP: f32 = 288.15;

/// Bomb drag coefficients by type
pub fn get_bomb_drag_coeff(bomb_type: BombType) -> f32 {
    match bomb_type {
        BombType::Mk81 => 0.35,
        BombType::Mk82 => 0.32,
        BombType::Mk83 => 0.30,
        BombType::Mk84 => 0.28,
        BombType::BLU109 => 0.25,
        BombType::GBU31 => 0.22,  // JDAM with strakes
        BombType::GBU28 => 0.20,  // Streamlined penetrator
        BombType::GBU43 => 0.35,  // MOAB - blunt
        BombType::GrandSlam => 0.22, // Aerodynamic earthquake bomb
        BombType::LittleBoy => 0.30,
        BombType::FatMan => 0.32,
        BombType::TsarBomba => 0.38, // Massive with parachute
    }
}

/// Bomb lift coefficients by type
pub fn get_bomb_lift_coeff(bomb_type: BombType) -> f32 {
    match bomb_type {
        BombType::GBU31 => 0.15,  // Strakes provide lift
        BombType::GBU28 => 0.05,
        BombType::GBU43 => 0.0,
        BombType::GrandSlam => 0.08,
        BombType::LittleBoy => 0.02,
        BombType::FatMan => 0.02,
        BombType::TsarBomba => 0.0,
        _ => 0.0,
    }
}

/// Bomb cross-sectional area (m²)
pub fn get_bomb_cross_section(bomb_type: BombType) -> f32 {
    match bomb_type {
        BombType::Mk81 => 0.0113,   // 120mm diameter
        BombType::Mk82 => 0.0201,   // 160mm diameter
        BombType::Mk83 => 0.0314,   // 200mm diameter
        BombType::Mk84 => 0.0507,   // 254mm diameter
        BombType::BLU109 => 0.0507, // Same as Mk84
        BombType::GBU31 => 0.0707,  // With strakes
        BombType::GBU28 => 0.0491,  // 250mm diameter
        BombType::GBU43 => 0.3848,  // 700mm diameter
        BombType::GrandSlam => 0.2376, // 550mm diameter
        BombType::LittleBoy => 0.1590, // 450mm diameter
        BombType::FatMan => 0.2027, // 508mm diameter
        BombType::TsarBomba => 1.7671, // 1500mm diameter
    }
}

/// Bomb mass (kg)
pub fn get_bomb_mass(bomb_type: BombType) -> f32 {
    match bomb_type {
        BombType::Mk81 => 113.0,
        BombType::Mk82 => 227.0,
        BombType::Mk83 => 454.0,
        BombType::Mk84 => 907.0,
        BombType::BLU109 => 907.0,
        BombType::GBU31 => 984.0,   // Mk84 + guidance kit
        BombType::GBU28 => 2268.0,
        BombType::GBU43 => 9797.0,
        BombType::GrandSlam => 9979.0,
        BombType::LittleBoy => 4400.0,
        BombType::FatMan => 4670.0,
        BombType::TsarBomba => 27000.0,
    }
}

/// Bomb spin rate (rad/s) for stability
pub fn get_bomb_spin(bomb_type: BombType) -> f32 {
    match bomb_type {
        BombType::GBU31 => 0.0,     // GPS guided, no spin
        BombType::GBU28 => 0.0,     // Laser guided
        BombType::GBU43 => 0.0,     // Parachute retarded
        BombType::TsarBomba => 0.0, // Parachute retarded
        BombType::GrandSlam => 10.0, // Spun for stability
        _ => 15.0,                  // Standard fin-stabilized spin
    }
}

/// Calculate air density at altitude (kg/m³)
pub fn air_density_at_altitude(altitude: f32) -> f32 {
    if altitude <= 11000.0 {
        // Troposphere
        let temp = SEA_LEVEL_TEMP - LAPSE_RATE * altitude;
        let pressure_ratio = (temp / SEA_LEVEL_TEMP).powf(GRAVITY / (GAS_CONSTANT * LAPSE_RATE));
        SEA_LEVEL_AIR_DENSITY * pressure_ratio * (SEA_LEVEL_TEMP / temp)
    } else if altitude <= 20000.0 {
        // Lower stratosphere (isothermal)
        let temp = SEA_LEVEL_TEMP - LAPSE_RATE * 11000.0;
        let pressure_11km = (temp / SEA_LEVEL_TEMP).powf(GRAVITY / (GAS_CONSTANT * LAPSE_RATE));
        let pressure_ratio = pressure_11km * (-GRAVITY * (altitude - 11000.0) / (GAS_CONSTANT * temp)).exp();
        SEA_LEVEL_AIR_DENSITY * pressure_ratio * (SEA_LEVEL_TEMP / temp)
    } else {
        // Upper stratosphere - very low density
        0.088 * (- (altitude - 20000.0) / 7000.0).exp()
    }
}

/// Calculate dynamic pressure (Pa)
pub fn dynamic_pressure(density: f32, velocity: f32) -> f32 {
    0.5 * density * velocity * velocity
}

/// Calculate Mach number
pub fn mach_number(velocity: f32, altitude: f32) -> f32 {
    // Speed of sound varies with temperature
    let temp = if altitude <= 11000.0 {
        SEA_LEVEL_TEMP - LAPSE_RATE * altitude
    } else {
        SEA_LEVEL_TEMP - LAPSE_RATE * 11000.0
    };
    let speed_of_sound = (1.4 * GAS_CONSTANT * temp).sqrt();
    velocity / speed_of_sound
}

/// Compressibility correction for drag coefficient
pub fn compressibility_correction(mach: f32, base_cd: f32) -> f32 {
    if mach < 0.8 {
        base_cd
    } else if mach < 1.2 {
        // Transonic rise
        base_cd * (1.0 + 0.5 * (mach - 0.8) / 0.4)
    } else {
        // Supersonic
        base_cd * (1.0 + 0.5) / mach.sqrt()
    }
}

/// Magnus force for spinning projectiles
pub fn magnus_force(spin: f32, velocity: Vector3<f32>, density: f32, radius: f32) -> Vector3<f32> {
    if spin == 0.0 {
        return Vector3::zeros();
    }

    let speed = velocity.norm();
    if speed < 1.0 {
        return Vector3::zeros();
    }

    // Magnus force: F = 0.5 * ρ * v² * A * Cl * (ω × v) / (|ω| * |v|)
    // Simplified: perpendicular to velocity and spin axis
    let spin_axis = Vector3::new(0.0, 0.0, spin); // Spin around longitudinal axis
    let magnus_dir = spin_axis.cross(&velocity).normalize();
    let cl_magnus = 0.01 * (spin * radius / speed).min(1.0);
    let area = std::f32::consts::PI * radius * radius;

    magnus_dir * 0.5 * density * speed * speed * area * cl_magnus
}

/// RK4 integration step for bomb physics
pub fn integrate_bomb_rk4(
    bomb: &mut BombPhysics,
    dt: f32,
    wind: Vector3<f32>,
) {
    let state = BombState {
        position: bomb.position,
        velocity: bomb.velocity,
    };

    let k1 = acceleration(&state, bomb, wind);
    let k2 = acceleration(&BombState {
        position: state.position + state.velocity * (dt * 0.5),
        velocity: state.velocity + k1 * (dt * 0.5),
    }, bomb, wind);
    let k3 = acceleration(&BombState {
        position: state.position + state.velocity * (dt * 0.5),
        velocity: state.velocity + k2 * (dt * 0.5),
    }, bomb, wind);
    let k4 = acceleration(&BombState {
        position: state.position + state.velocity * dt,
        velocity: state.velocity + k3 * dt,
    }, bomb, wind);

    bomb.position += state.velocity * dt + (k1 + k2 * 2.0 + k3 * 2.0 + k4) * (dt * dt / 6.0);
    bomb.velocity += (k1 + k2 * 2.0 + k3 * 2.0 + k4) * (dt / 6.0);

    // Update angular velocity (simplified - no torque modeling)
    bomb.angular_velocity *= 0.999; // Very slow decay
}

#[derive(Clone, Copy)]
struct BombState {
    position: Vector3<f32>,
    velocity: Vector3<f32>,
}

/// Calculate acceleration vector for a bomb
fn acceleration(state: &BombState, bomb: &BombPhysics, wind: Vector3<f32>) -> Vector3<f32> {
    let rel_velocity = state.velocity - wind;
    let speed = rel_velocity.norm();

    if speed < 0.01 {
        return Vector3::new(0.0, -GRAVITY, 0.0);
    }

    let altitude = state.position.y.max(0.0);
    let density = air_density_at_altitude(altitude);
    let mach = mach_number(speed, altitude);

    let base_cd = bomb.drag_coeff;
    let cd = compressibility_correction(mach, base_cd);
    let cl = bomb.lift_coeff;
    let area = bomb.cross_section;
    let mass = bomb.mass;

    // Drag force (opposite to relative velocity)
    let drag_dir = -rel_velocity / speed;
    let drag_force = 0.5 * density * speed * speed * area * cd;
    let drag_accel = drag_dir * (drag_force / mass);

    // Lift force (perpendicular to velocity)
    let lift_accel = if cl > 0.0 && speed > 10.0 {
        // Lift perpendicular to velocity in vertical plane
        let lift_dir = Vector3::new(-rel_velocity.z, 0.0, rel_velocity.x).normalize();
        let lift_force = 0.5 * density * speed * speed * area * cl;
        lift_dir * (lift_force / mass)
    } else {
        Vector3::zeros()
    };

    // Magnus force
    let magnus_accel = magnus_force(bomb.spin, rel_velocity, density, area.sqrt() / 2.0) / mass;

    // Gravity
    let gravity = Vector3::new(0.0, -GRAVITY, 0.0);

    drag_accel + lift_accel + magnus_accel + gravity
}

/// Predict bomb impact point from release conditions
pub fn predict_impact(
    release_pos: Vector3<f32>,
    release_vel: Vector3<f32>,
    bomb_type: BombType,
    wind: Vector3<f32>,
    max_time: f32,
) -> Option<Vector3<f32>> {
    let mut bomb = BombPhysics {
        position: release_pos,
        velocity: release_vel,
        angular_velocity: Vector3::new(0.0, 0.0, get_bomb_spin(bomb_type)),
        mass: get_bomb_mass(bomb_type),
        drag_coeff: get_bomb_drag_coeff(bomb_type),
        lift_coeff: get_bomb_lift_coeff(bomb_type),
        cross_section: get_bomb_cross_section(bomb_type),
        spin: get_bomb_spin(bomb_type),
    };

    let dt = 0.05; // 50ms integration steps
    let steps = (max_time / dt) as usize;

    for _ in 0..steps {
        if bomb.position.y <= 1.0 {
            // Hit ground
            return Some(Vector3::new(bomb.position.x, 0.0, bomb.position.z));
        }

        integrate_bomb_rk4(&mut bomb, dt, wind);
    }

    None // Didn't hit ground within max_time
}

/// Generate bomb trajectory points for visual preview
pub fn generate_trajectory_points(
    release_pos: Vector3<f32>,
    release_vel: Vector3<f32>,
    bomb_type: BombType,
    wind: Vector3<f32>,
    max_points: usize,
) -> Vec<Vector3<f32>> {
    let mut bomb = BombPhysics {
        position: release_pos,
        velocity: release_vel,
        angular_velocity: Vector3::new(0.0, 0.0, get_bomb_spin(bomb_type)),
        mass: get_bomb_mass(bomb_type),
        drag_coeff: get_bomb_drag_coeff(bomb_type),
        lift_coeff: get_bomb_lift_coeff(bomb_type),
        cross_section: get_bomb_cross_section(bomb_type),
        spin: get_bomb_spin(bomb_type),
    };

    let mut points = Vec::with_capacity(max_points);
    points.push(bomb.position);

    let dt = 0.1; // 100ms steps for trajectory preview
    let skip = 2; // Only store every 2nd point

    for i in 0..max_points * skip {
        if bomb.position.y <= 1.0 {
            break;
        }

        integrate_bomb_rk4(&mut bomb, dt, wind);

        if i % skip == 0 {
            points.push(bomb.position);
        }
    }

    points
}

/// Called every tick (20 Hz) to update all bomb physics
pub fn native_tick() {
    let tick = SIMULATION_TICK.fetch_add(1, Ordering::Relaxed);
    let dt = 0.05; // 20 Hz = 50ms

    // Update all active bombs
    with_bomb_simulation(|sim| {
        let bombs = sim.get_active_bombs();
        for bomb in bombs {
            // Get wind at bomb position
            let wind = get_wind_at_position(bomb.position);
            integrate_bomb_rk4(bomb, dt, wind);

            // Check for impact
            if bomb.position.y <= 1.0 {
                bomb.explode();
                sim.remove_bomb(bomb.id);
            }
        }
    });

    // Update all active planes
    with_plane_simulation(|sim| {
        let planes = sim.get_active_planes();
        for plane in planes {
            plane.update_physics(dt);
        }
    });
}

/// Get wind vector at a position (simplified - in reality would sample from weather system)
fn get_wind_at_position(pos: Vector3<f32>) -> Vector3<f32> {
    // TODO: Integrate with Horizon's weather system
    // For now, return a simple altitude-based wind
    let altitude = pos.y.max(0.0);
    let wind_speed = (altitude / 1000.0).min(50.0); // Increases with altitude, caps at 50 m/s
    let wind_dir = (tick as f32 * 0.001).sin(); // Slowly varying direction

    Vector3::new(
        wind_dir * wind_speed,
        0.0,
        wind_dir.cos() * wind_speed,
    )
}

/// Plane physics constants by type
pub fn get_plane_mass(plane_type: PlaneType) -> f32 {
    match plane_type {
        PlaneType::B17G => 29700.0,
        PlaneType::B24 => 25200.0,
        PlaneType::B29 => 63500.0,
        PlaneType::B52H => 181000.0,
        PlaneType::B1B => 86000.0,
        PlaneType::B2 => 71700.0,
    }
}

pub fn get_plane_wing_area(plane_type: PlaneType) -> f32 {
    match plane_type {
        PlaneType::B17G => 131.9,
        PlaneType::B24 => 100.6,
        PlaneType::B29 => 162.0,
        PlaneType::B52H => 370.0,
        PlaneType::B1B => 181.0, // Swept forward
        PlaneType::B2 => 478.0,
    }
}

pub fn get_plane_max_thrust(plane_type: PlaneType) -> f32 {
    match plane_type {
        PlaneType::B17G => 4 * 1200.0 * 4.448, // 4x 1200 hp
        PlaneType::B24 => 4 * 1200.0 * 4.448,
        PlaneType::B29 => 4 * 2200.0 * 4.448, // 4x 2200 hp
        PlaneType::B52H => 8 * 17000.0 * 4.448, // 8x TF33 turbofans
        PlaneType::B1B => 4 * 30000.0 * 4.448, // 4x F101 turbofans
        PlaneType::B2 => 4 * 17300.0 * 4.448, // 4x F118 turbofans
    }
}

pub fn get_plane_drag_coeff(plane_type: PlaneType) -> f32 {
    match plane_type {
        PlaneType::B17G => 0.035,
        PlaneType::B24 => 0.032,
        PlaneType::B29 => 0.028,
        PlaneType::B52H => 0.025,
        PlaneType::B1B => 0.020, // Variable sweep
        PlaneType::B2 => 0.018, // Flying wing
    }
}

/// Calculate lift coefficient based on angle of attack
pub fn lift_coefficient(aoa_deg: f32, mach: f32) -> f32 {
    let aoa_rad = aoa_deg.to_radians();
    let cl_alpha = if mach < 0.8 {
        2.0 * std::f32::consts::PI // Subsonic
    } else if mach < 1.2 {
        2.0 * std::f32::consts::PI / (1.0 - mach * mach).sqrt() // Transonic
    } else {
        4.0 / (mach * mach - 1.0).sqrt() // Supersonic
    };

    (cl_alpha * aoa_rad).clamp(-2.0, 2.0)
}

/// Public API for other modules
pub use self::{
    GRAVITY,
    SEA_LEVEL_AIR_DENSITY,
    air_density_at_altitude,
    dynamic_pressure,
    mach_number,
    predict_impact,
    generate_trajectory_points,
    integrate_bomb_rk4,
    get_bomb_drag_coeff,
    get_bomb_lift_coeff,
    get_bomb_cross_section,
    get_bomb_mass,
    get_bomb_spin,
    get_plane_mass,
    get_plane_wing_area,
    get_plane_max_thrust,
    get_plane_drag_coeff,
    lift_coefficient,
};