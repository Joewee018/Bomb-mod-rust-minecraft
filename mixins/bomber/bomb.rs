//! Bomber Mod Bomb System
//!
//! 12 bomb types with custom physics, guidance, and explosion effects.

use crate::bomber_runtime::{
    BombType, BombPhysics, FuzeType, ExplosionEvent, Vec3,
    with_bomb_simulation, next_bomb_id, get_bomb_model_name,
};
use crate::physics::{predict_impact, generate_trajectory_points, integrate_bomb_rk4, get_wind_at_position};
use nalgebra::Vector3;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::atomic::{AtomicU64, Ordering};

static BOMB_TICK: AtomicU64 = AtomicU64::new(0);

/// Bomb guidance types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuidanceType {
    None,           // Free-fall
    GPS,            // GBU-31 JDAM
    Laser,          // GBU-28, Paveway
    Radar,          // Future
    Infrared,       // Future
    Parachute,      // GBU-43, Tsar Bomba
}

/// Bomb fuze/arming state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BombState {
    Armed,
    Fuzed,
    Detonated,
    Dud,
}

/// Extended bomb data for simulation
#[derive(Debug, Clone)]
pub struct BombEntity {
    pub physics: BombPhysics,
    pub state: BombState,
    pub guidance: GuidanceType,
    pub target: Option<Vec3>,
    pub parachute_deployed: bool,
    pub parachute_altitude: f32,
    pub arming_delay: f32, // Seconds after release before armed
    pub time_since_release: f32,
    pub trail_points: Vec<Vec3>, // For visual trail
}

impl BombEntity {
    pub fn new(physics: BombPhysics) -> Self {
        let guidance = match physics.bomb_type {
            crate::bomber_runtime::BombType::GBU31 => GuidanceType::GPS,
            crate::bomber_runtime::BombType::GBU28 => GuidanceType::Laser,
            crate::bomber_runtime::BombType::GBU43 => GuidanceType::Parachute,
            crate::bomber_runtime::BombType::TsarBomba => GuidanceType::Parachute,
            crate::bomber_runtime::BombType::Mk84 => GuidanceType::None, // Can be Paveway
            _ => GuidanceType::None,
        };

        let parachute_altitude = match physics.bomb_type {
            crate::bomber_runtime::BombType::GBU43 => 3000.0,
            crate::bomber_runtime::BombType::TsarBomba => 5000.0,
            _ => 0.0,
        };

        let arming_delay = match physics.fuze_type {
            FuzeType::Delay => 0.5,
            FuzeType::Proximity => 1.0,
            FuzeType::Airburst => 2.0,
            _ => 0.0,
        };

        Self {
            physics,
            state: BombState::Armed,
            guidance,
            target: None,
            parachute_deployed: false,
            parachute_altitude,
            arming_delay,
            time_since_release: 0.0,
            trail_points: Vec::new(),
        }
    }
}

/// Spawn a bomb with full entity data
pub fn spawn_bomb_entity(
    bomb_type: crate::bomber_runtime::BombType,
    position: Vec3,
    velocity: Vec3,
) -> u32 {
    let bomb_id = next_bomb_id();
    let mut physics = BombPhysics::new(bomb_id, bomb_type, position, velocity);
    physics.release_time = crate::physics::SIMULATION_TICK.load(Ordering::Relaxed);

    // Set fuze type
    physics.fuze_type = match bomb_type {
        crate::bomber_runtime::BombType::GBU43 => FuzeType::Airburst,
        crate::bomber_runtime::BombType::TsarBomba => FuzeType::Airburst,
        crate::bomber_runtime::BombType::LittleBoy => FuzeType::Proximity,
        crate::bomber_runtime::BombType::FatMan => FuzeType::Proximity,
        crate::bomber_runtime::BombType::BLU109 => FuzeType::Delay,
        crate::bomber_runtime::BombType::GBU28 => FuzeType::Delay,
        crate::bomber_runtime::BombType::GrandSlam => FuzeType::Delay,
        _ => FuzeType::Impact,
    };

    let entity = BombEntity::new(physics);

    with_bomb_simulation(|sim| {
        sim.entities.insert(bomb_id, entity);
    });

    bomb_id
}

/// Update all bomb entities (called from native_tick)
pub fn update_bomb_entities(dt: f32) {
    BOMB_TICK.fetch_add(1, Ordering::Relaxed);

    with_bomb_simulation(|sim| {
        let wind_cache = WindCache::new();
        let mut to_remove = Vec::new();
        let mut explosions = Vec::new();

        for (id, entity) in &mut sim.entities {
            entity.time_since_release += dt;

            // Update trail for visual effect
            if entity.trail_points.len() < 50 {
                entity.trail_points.push(entity.physics.position);
            } else {
                entity.trail_points.remove(0);
                entity.trail_points.push(entity.physics.position);
            }

            // Check arming delay
            if entity.state == BombState::Armed && entity.time_since_release < entity.arming_delay {
                // Not yet armed, just fall with drag
                let wind = wind_cache.get_wind(entity.physics.position);
                integrate_bomb_rk4(&mut entity.physics, dt, wind);
                continue;
            }

            if entity.state == BombState::Armed {
                entity.state = BombState::Fuzed;
            }

            // Guidance updates
            update_guidance(entity, dt);

            // Parachute deployment
            if entity.parachute_altitude > 0.0
                && !entity.parachute_deployed
                && entity.physics.position.y <= entity.parachute_altitude
            {
                deploy_parachute(entity);
            }

            // Physics integration
            let wind = wind_cache.get_wind(entity.physics.position);
            integrate_bomb_rk4(&mut entity.physics, dt, wind);

            // Check for impact/detonation
            if check_detonation(entity) {
                let explosion = ExplosionEvent::new(entity.physics.position, entity.physics.bomb_type);
                explosions.push((*id, explosion));
                to_remove.push(*id);
            }
        }

        // Remove detonated bombs
        for id in to_remove {
            sim.entities.remove(&id);
        }

        // Add explosions
        for (id, explosion) in explosions {
            sim.explosions.push(explosion);
        }
    });
}

/// Wind cache for performance
struct WindCache {
    cache: std::collections::HashMap<(i32, i32, i32), Vec3>,
}

impl WindCache {
    fn new() -> Self {
        Self {
            cache: std::collections::HashMap::new(),
        }
    }

    fn get_wind(&mut self, pos: Vec3) -> Vec3 {
        let key = (
            (pos.x / 1000.0).floor() as i32,
            (pos.y / 1000.0).floor() as i32,
            (pos.z / 1000.0).floor() as i32,
        );

        *self.cache.entry(key).or_insert_with(|| get_wind_at_position(pos))
    }
}

/// Update guidance for guided bombs
fn update_guidance(entity: &mut BombEntity, dt: f32) {
    match entity.guidance {
        GuidanceType::GPS => {
            // GBU-31 JDAM - GPS guidance to target coordinates
            if let Some(target) = entity.target {
                guide_to_target(entity, target, dt, 15.0); // Max 15 m/s correction
            }
        }
        GuidanceType::Laser => {
            // GBU-28, Paveway - Laser guidance
            if let Some(target) = entity.target {
                guide_to_target(entity, target, dt, 25.0); // More responsive
            }
        }
        GuidanceType::Parachute => {
            // Parachute retardation - already handled in physics via drag
        }
        _ => {}
    }
}

/// Proportional navigation guidance
fn guide_to_target(entity: &mut BombEntity, target: Vec3, dt: f32, max_accel: f32) {
    let pos = entity.physics.position;
    let vel = entity.physics.velocity;
    let speed = vel.norm();

    if speed < 10.0 {
        return; // Too slow for effective guidance
    }

    let to_target = target - pos;
    let distance = to_target.norm();

    if distance < 5.0 {
        return; // Close enough
    }

    // Calculate required velocity change (proportional navigation)
    let los = to_target / distance;
    let vel_dir = vel / speed;

    // Cross product for turn rate
    let los_rate = los.cross(&vel_dir);
    let turn_rate = los_rate.norm();

    if turn_rate < 0.001 {
        return;
    }

    // Proportional navigation: N * Vc * λ̇
    // N = 3-5 (navigation constant), Vc = closing velocity
    let vc = -vel.dot(&los).max(0.0);
    let nav_constant = 3.0;
    let commanded_accel = nav_constant * vc * turn_rate;

    // Limit acceleration
    let accel_magnitude = commanded_accel.min(max_accel);

    // Apply acceleration perpendicular to velocity
    let accel_dir = los_rate.normalize();
    entity.physics.velocity += accel_dir * accel_magnitude * dt;
}

/// Deploy parachute (massively increases drag)
fn deploy_parachute(entity: &mut BombEntity) {
    entity.parachute_deployed = true;

    // Increase drag coefficient dramatically
    entity.physics.drag_coeff *= 20.0;
    entity.physics.cross_section *= 50.0; // Parachute area
    entity.physics.lift_coeff = 0.5; // Some lift from parachute

    // Add slight upward velocity component (parachute opening shock)
    entity.physics.velocity.y = entity.physics.velocity.y.max(-5.0);
}

/// Check if bomb should detonate
fn check_detonation(entity: &mut BombEntity) -> bool {
    if entity.state == BombState::Detonated || entity.state == BombState::Dud {
        return true;
    }

    let pos = entity.physics.position;
    let vel = entity.physics.velocity;
    let bomb_type = entity.physics.bomb_type;

    match entity.physics.fuze_type {
        FuzeType::Impact => {
            // Detonate on ground impact
            if pos.y <= 2.0 && vel.y < 0.0 {
                entity.state = BombState::Detonated;
                return true;
            }
        }
        FuzeType::Delay => {
            // Penetrate then detonate
            if pos.y <= 2.0 && vel.y < 0.0 {
                // In reality would penetrate ground first
                // For game, detonate slightly below surface
                entity.state = BombState::Detonated;
                return true;
            }
        }
        FuzeType::Proximity => {
            // Detonate at set altitude above ground
            let detonation_altitude = match bomb_type {
                crate::bomber_runtime::BombType::LittleBoy => 580.0, // ~1900 ft
                crate::bomber_runtime::BombType::FatMan => 500.0,
                _ => 10.0,
            };

            if pos.y <= detonation_altitude {
                entity.state = BombState::Detonated;
                return true;
            }
        }
        FuzeType::Airburst => {
            // Detonate at optimal altitude for blast effect
            let burst_altitude = match bomb_type {
                crate::bomber_runtime::BombType::GBU43 => 100.0,
                crate::bomber_runtime::BombType::TsarBomba => 4000.0, // Airburst for max effect
                _ => 50.0,
            };

            if pos.y <= burst_altitude && vel.y < 0.0 {
                entity.state = BombState::Detonated;
                return true;
            }
        }
        FuzeType::Contact => {
            // Same as impact but more sensitive
            if pos.y <= 1.0 {
                entity.state = BombState::Detonated;
                return true;
            }
        }
    }

    // Safety: dud if falling too long
    if entity.time_since_release > 300.0 {
        entity.state = BombState::Dud;
        return true;
    }

    false
}

/// Set target for guided bomb
pub fn set_bomb_target(bomb_id: u32, target: Vec3) {
    with_bomb_simulation(|sim| {
        if let Some(entity) = sim.entities.get_mut(&bomb_id) {
            entity.target = Some(target);
        }
    })
}

/// Get bomb trajectory preview points
pub fn get_bomb_trajectory(
    release_pos: Vec3,
    release_vel: Vec3,
    bomb_type: crate::bomber_runtime::BombType,
) -> Vec<Vec3> {
    let wind = get_wind_at_position(release_pos);
    generate_trajectory_points(release_pos, release_vel, bomb_type, wind, 100)
}

/// Predict impact point
pub fn predict_bomb_impact(
    release_pos: Vec3,
    release_vel: Vec3,
    bomb_type: crate::bomber_runtime::BombType,
) -> Option<Vec3> {
    let wind = get_wind_at_position(release_pos);
    predict_impact(release_pos, release_vel, bomb_type, wind, 120.0)
}

/// Command to spawn a bomb (for testing)
pub fn cmd_spawn_bomb(args: &[&str]) -> String {
    if args.is_empty() {
        return "Usage: /bomber bomb <type> [x] [y] [z] [vx] [vy] [vz]".to_string();
    }

    let bomb_type = match args[0].to_lowercase().as_str() {
        "mk81" => crate::bomber_runtime::BombType::Mk81,
        "mk82" => crate::bomber_runtime::BombType::Mk82,
        "mk83" => crate::bomber_runtime::BombType::Mk83,
        "mk84" => crate::bomber_runtime::BombType::Mk84,
        "blu109" => crate::bomber_runtime::BombType::BLU109,
        "gbu31" | "jdam" => crate::bomber_runtime::BombType::GBU31,
        "gbu28" => crate::bomber_runtime::BombType::GBU28,
        "gbu43" | "moab" => crate::bomber_runtime::BombType::GBU43,
        "grandslam" => crate::bomber_runtime::BombType::GrandSlam,
        "littleboy" => crate::bomber_runtime::BombType::LittleBoy,
        "fatman" => crate::bomber_runtime::BombType::FatMan,
        "tsar" | "tsarbomba" => crate::bomber_runtime::BombType::TsarBomba,
        _ => return format!("Unknown bomb type: {}", args[0]),
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

    let vel = if args.len() >= 7 {
        Vec3::new(
            args[4].parse().unwrap_or(0.0),
            args[5].parse().unwrap_or(0.0),
            args[6].parse().unwrap_or(0.0),
        )
    } else {
        Vec3::new(100.0, 0.0, 0.0) // Default forward velocity
    };

    let bomb_id = spawn_bomb_entity(bomb_type, pos, vel);

    format!("Spawned {} (ID: {}) at ({:.0}, {:.0}, {:.0})", bomb_type.name(), bomb_id, pos.x, pos.y, pos.z)
}

/// Command to list active bombs
pub fn cmd_list_bombs(_args: &[&str]) -> String {
    with_bomb_simulation(|sim| {
        let bombs = sim.entities.values().collect::<Vec<_>>();
        if bombs.is_empty() {
            return "No active bombs".to_string();
        }

        let mut result = String::from("Active bombs:\n");
        for entity in bombs {
            let p = &entity.physics;
            result.push_str(&format!(
                "  #{:04} {} | Pos: ({:.0}, {:.0}, {:.0}) | Vel: {:.1} m/s | State: {:?} | Guidance: {:?}\n",
                p.id, p.bomb_type.name(), p.position.x, p.position.y, p.position.z, p.velocity.norm(), entity.state, entity.guidance
            ));
        }
        result
    })
}

/// Command to clear all bombs
pub fn cmd_clear_bombs(_args: &[&str]) -> String {
    with_bomb_simulation(|sim| {
        let count = sim.entities.len();
        sim.entities.clear();
        format!("Cleared {} bombs", count)
    })
}

/// Command to trigger nuke at position
pub fn cmd_nuke(args: &[&str]) -> String {
    let bomb_type = if args.is_empty() {
        crate::bomber_runtime::BombType::TsarBomba
    } else {
        match args[0].to_lowercase().as_str() {
            "littleboy" => crate::bomber_runtime::BombType::LittleBoy,
            "fatman" => crate::bomber_runtime::BombType::FatMan,
            "tsar" | "tsarbomba" => crate::bomber_runtime::BombType::TsarBomba,
            _ => return "Usage: /bomber nuke [littleboy|fatman|tsar]".to_string(),
        }
    };

    let pos = if args.len() >= 4 {
        Vec3::new(
            args[1].parse().unwrap_or(0.0),
            args[2].parse().unwrap_or(100.0),
            args[3].parse().unwrap_or(0.0),
        )
    } else {
        Vec3::new(0.0, 100.0, 0.0)
    };

    let bomb_id = spawn_bomb_entity(bomb_type, pos, Vec3::new(0.0, -50.0, 0.0));

    // Force immediate detonation for testing
    with_bomb_simulation(|sim| {
        if let Some(entity) = sim.entities.get_mut(&bomb_id) {
            entity.state = BombState::Detonated;
        }
    });

    format!("Detonated {} at ({:.0}, {:0}, {:.0})", bomb_type.name(), pos.x, pos.y, pos.z)
}

/// Get bomb info for HUD/UI
pub fn get_bomb_info(bomb_id: u32) -> Option<String> {
    with_bomb_simulation(|sim| {
        sim.entities.get(&bomb_id).map(|entity| {
            let p = &entity.physics;
            format!(
                "{} | State: {:?} | Pos: ({:.0}, {:.0}, {:.0}) | Vel: {:.1} m/s | Target: {}",
                p.bomb_type.name(),
                entity.state,
                p.position.x, p.position.y, p.position.z,
                p.velocity.norm(),
                entity.target.map_or("None".to_string(), |t| format!("({:.0}, {:.0}, {:.0})", t.x, t.y, t.z))
            )
        })
    })
}

/// Get all active bomb entities for rendering
pub fn get_active_bomb_entities() -> Vec<BombEntity> {
    with_bomb_simulation(|sim| {
        sim.entities.values().cloned().collect()
    })
}

/// Get explosion events for rendering
pub fn take_explosions() -> Vec<ExplosionEvent> {
    with_bomb_simulation(|sim| {
        std::mem::take(&mut sim.explosions)
    })
}

/// Nuke yield in kilotons
pub fn get_nuke_yield_kt(bomb_type: crate::bomber_runtime::BombType) -> f32 {
    match bomb_type {
        crate::bomber_runtime::BombType::LittleBoy => 15.0,
        crate::bomber_runtime::BombType::FatMan => 21.0,
        crate::bomber_runtime::BombType::TsarBomba => 50000.0,
        crate::bomber_runtime::BombType::GBU43 => 0.011,
        crate::bomber_runtime::BombType::GrandSlam => 0.006,
        _ => 0.0,
    }
}

/// Check if bomb is nuclear
pub fn is_nuclear(bomb_type: crate::bomber_runtime::BombType) -> bool {
    matches!(bomb_type,
        crate::bomber_runtime::BombType::LittleBoy |
        crate::bomber_runtime::BombType::FatMan |
        crate::bomber_runtime::BombType::TsarBomba
    )
}

/// Get bomb model name for rendering
pub fn get_bomb_model(bomb_type: crate::bomber_runtime::BombType) -> &'static str {
    get_bomb_model_name(bomb_type)
}