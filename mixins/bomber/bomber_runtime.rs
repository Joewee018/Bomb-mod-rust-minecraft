//! Bomber Mod Core Runtime
//!
//! Core data structures, simulation managers, and global state for the bomber mod.

use nalgebra::{Vector3, Vector2, UnitQuaternion};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

// ============================================================================
// Global State
// ============================================================================

static BOMB_SIMULATION: OnceLock<Arc<Mutex<BombSimulation>>> = OnceLock::new();
static PLANE_SIMULATION: OnceLock<Arc<Mutex<PlaneSimulation>>> = OnceLock::new();
static FLIGHT_SESSION: OnceLock<Arc<Mutex<Option<FlightSession>>>> = OnceLock::new();
static SETTINGS: OnceLock<Arc<Mutex<ModSettings>>> = OnceLock::new();

pub static SIMULATION_ENABLED: AtomicBool = AtomicBool::new(true);
pub static DEBUG_PHYSICS: AtomicBool = AtomicBool::new(false);
pub static NUKE_EFFECTS_ENABLED: AtomicBool = AtomicBool::new(true);

/// Initialize global simulations
pub fn init_simulations() {
    BOMB_SIMULATION.set(Arc::new(Mutex::new(BombSimulation::new()))).ok();
    PLANE_SIMULATION.set(Arc::new(Mutex::new(PlaneSimulation::new()))).ok();
    FLIGHT_SESSION.set(Arc::new(Mutex::new(None))).ok();
    SETTINGS.set(Arc::new(Mutex::new(ModSettings::default()))).ok();
}

/// Access bomb simulation
pub fn with_bomb_simulation<F, R>(f: F) -> R
where
    F: FnOnce(&mut BombSimulation) -> R,
{
    let sim = BOMB_SIMULATION.get().expect("Bomb simulation not initialized");
    let mut guard = sim.lock().unwrap();
    f(&mut guard)
}

/// Access plane simulation
pub fn with_plane_simulation<F, R>(f: F) -> R
where
    F: FnOnce(&mut PlaneSimulation) -> R,
{
    let sim = PLANE_SIMULATION.get().expect("Plane simulation not initialized");
    let mut guard = sim.lock().unwrap();
    f(&mut guard)
}

/// Access flight session
pub fn with_flight_session<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut FlightSession) -> R,
{
    let session = FLIGHT_SESSION.get().expect("Flight session not initialized");
    let mut guard = session.lock().unwrap();
    guard.as_mut().map(f)
}

/// Access settings
pub fn with_settings<F, R>(f: F) -> R
where
    F: FnOnce(&mut ModSettings) -> R,
{
    let settings = SETTINGS.get().expect("Settings not initialized");
    let mut guard = settings.lock().unwrap();
    f(&mut guard)
}

// ============================================================================
// Enums & Types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BombType {
    Mk81,
    Mk82,
    Mk83,
    Mk84,
    BLU109,
    GBU31,
    GBU28,
    GBU43,
    GrandSlam,
    LittleBoy,
    FatMan,
    TsarBomba,
}

impl BombType {
    pub fn all() -> &'static [BombType] {
        &[
            BombType::Mk81,
            BombType::Mk82,
            BombType::Mk83,
            BombType::Mk84,
            BombType::BLU109,
            BombType::GBU31,
            BombType::GBU28,
            BombType::GBU43,
            BombType::GrandSlam,
            BombType::LittleBoy,
            BombType::FatMan,
            BombType::TsarBomba,
        ]
    }

    pub fn tier(&self) -> BombTier {
        match self {
            BombType::Mk81 | BombType::Mk82 | BombType::Mk83 => BombTier::Light,
            BombType::Mk84 | BombType::BLU109 | BombType::GBU31 => BombTier::Medium,
            BombType::GBU28 | BombType::GBU43 | BombType::GrandSlam => BombTier::Heavy,
            BombType::LittleBoy | BombType::FatMan | BombType::TsarBomba => BombTier::Nuclear,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            BombType::Mk81 => "Mk 81 (250 lb)",
            BombType::Mk82 => "Mk 82 (500 lb)",
            BombType::Mk83 => "Mk 83 (1,000 lb)",
            BombType::Mk84 => "Mk 84 (2,000 lb)",
            BombType::BLU109 => "BLU-109 Penetrator",
            BombType::GBU31 => "GBU-31 JDAM",
            BombType::GBU28 => "GBU-28 Bunker Buster",
            BombType::GBU43 => "GBU-43/B MOAB",
            BombType::GrandSlam => "Grand Slam",
            BombType::LittleBoy => "Little Boy (15 kt)",
            BombType::FatMan => "Fat Man (21 kt)",
            BombType::TsarBomba => "Tsar Bomba (50 Mt)",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            BombType::Mk81 => "Light general-purpose bomb",
            BombType::Mk82 => "Standard general-purpose bomb, Snakeye retarder option",
            BombType::Mk83 => "Medium general-purpose bomb",
            BombType::Mk84 => "Heavy general-purpose bomb, Paveway laser-guidance option",
            BombType::BLU109 => "Hardened penetration bomb for bunkers",
            BombType::GBU31 => "GPS-guided JDAM, precision strike",
            BombType::GBU28 => "Laser-guided deep penetrator",
            BombType::GBU43 => "Massive Ordnance Air Blast, largest conventional",
            BombType::GrandSlam => "WWII earthquake bomb, seismic effect",
            BombType::LittleBoy => "Gun-type fission weapon, Hiroshima",
            BombType::FatMan => "Implosion fission weapon, Nagasaki",
            BombType::TsarBomba => "Three-stage thermonuclear, largest ever tested",
        }
    }

    pub fn weight_kg(&self) -> f32 {
        crate::physics::get_bomb_mass(*self)
    }

    pub fn is_nuclear(&self) -> bool {
        matches!(self, BombType::LittleBoy | BombType::FatMan | BombType::TsarBomba)
    }

    pub fn is_guided(&self) -> bool {
        matches!(self, BombType::GBU31 | BombType::GBU28 | BombType::GBU43)
    }

    pub fn has_parachute(&self) -> bool {
        matches!(self, BombType::GBU43 | BombType::TsarBomba)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BombTier {
    Light,
    Medium,
    Heavy,
    Nuclear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlaneType {
    B17G,
    B24,
    B29,
    B52H,
    B1B,
    B2,
}

impl PlaneType {
    pub fn all() -> &'static [PlaneType] {
        &[
            PlaneType::B17G,
            PlaneType::B24,
            PlaneType::B29,
            PlaneType::B52H,
            PlaneType::B1B,
            PlaneType::B2,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            PlaneType::B17G => "B-17G Flying Fortress",
            PlaneType::B24 => "B-24 Liberator",
            PlaneType::B29 => "B-29 Superfortress",
            PlaneType::B52H => "B-52H Stratofortress",
            PlaneType::B1B => "B-1B Lancer",
            PlaneType::B2 => "B-2 Spirit",
        }
    }

    pub fn era(&self) -> &'static str {
        match self {
            PlaneType::B17G | PlaneType::B24 | PlaneType::B29 => "WWII",
            PlaneType::B52H => "Cold War",
            PlaneType::B1B | PlaneType::B2 => "Modern",
        }
    }

    pub fn max_speed_mps(&self) -> f32 {
        match self {
            PlaneType::B17G => 128.0,
            PlaneType::B24 => 134.0,
            PlaneType::B29 => 160.0,
            PlaneType::B52H => 290.0,
            PlaneType::B1B => 402.0, // Mach 1.2
            PlaneType::B2 => 281.0,
        }
    }

    pub fn service_ceiling(&self) -> f32 {
        match self {
            PlaneType::B17G => 10800.0,
            PlaneType::B24 => 8500.0,
            PlaneType::B29 => 9700.0,
            PlaneType::B52H => 15200.0,
            PlaneType::B1B => 18000.0,
            PlaneType::B2 => 15200.0,
        }
    }

    pub fn bomb_capacity(&self) -> usize {
        match self {
            PlaneType::B17G => 8,
            PlaneType::B24 => 8,
            PlaneType::B29 => 20,
            PlaneType::B52H => 70,
            PlaneType::B1B => 24,
            PlaneType::B2 => 16,
        }
    }

    pub fn compatible_bombs(&self) -> &'static [BombType] {
        match self {
            PlaneType::B17G => &[BombType::Mk81, BombType::Mk82, BombType::Mk83, BombType::Mk84],
            PlaneType::B24 => &[BombType::Mk81, BombType::Mk82, BombType::Mk83, BombType::Mk84],
            PlaneType::B29 => &[
                BombType::Mk81, BombType::Mk82, BombType::Mk83, BombType::Mk84,
                BombType::LittleBoy, BombType::FatMan,
            ],
            PlaneType::B52H => &[
                BombType::Mk82, BombType::Mk83, BombType::Mk84, BombType::BLU109,
                BombType::GBU31, BombType::GBU28, BombType::GBU43,
            ],
            PlaneType::B1B => &[
                BombType::Mk82, BombType::Mk83, BombType::Mk84, BombType::BLU109,
                BombType::GBU31, BombType::GBU28, BombType::GBU43,
            ],
            PlaneType::B2 => &[
                BombType::Mk84, BombType::BLU109, BombType::GBU31, BombType::GBU28,
                BombType::GBU43,
            ],
        }
    }
}

// ============================================================================
// Vector Types
// ============================================================================

pub type Vec3 = Vector3<f32>;
pub type Vec2 = Vector2<f32>;

pub trait Vec3Ext {
    fn horizontal(&self) -> Vec2;
    fn set_y(&self, y: f32) -> Self;
    fn distance_horizontal(&self, other: &Self) -> f32;
}

impl Vec3Ext for Vec3 {
    fn horizontal(&self) -> Vec2 {
        Vec2::new(self.x, self.z)
    }

    fn set_y(&self, y: f32) -> Self {
        Vec3::new(self.x, y, self.z)
    }

    fn distance_horizontal(&self, other: &Self) -> f32 {
        (self - other).horizontal().norm()
    }
}

// ============================================================================
// Bomb Physics State
// ============================================================================

#[derive(Debug, Clone)]
pub struct BombPhysics {
    pub id: u32,
    pub bomb_type: BombType,
    pub position: Vec3,
    pub velocity: Vec3,
    pub angular_velocity: Vec3,
    pub mass: f32,
    pub drag_coeff: f32,
    pub lift_coeff: f32,
    pub cross_section: f32,
    pub spin: f32,
    pub armed: bool,
    pub fuze_type: FuzeType,
    pub release_time: u64,
    pub guid_target: Option<Vec3>, // For guided bombs
}

impl BombPhysics {
    pub fn new(id: u32, bomb_type: BombType, position: Vec3, velocity: Vec3) -> Self {
        use crate::physics::*;
        Self {
            id,
            bomb_type,
            position,
            velocity,
            angular_velocity: Vec3::zeros(),
            mass: get_bomb_mass(bomb_type),
            drag_coeff: get_bomb_drag_coeff(bomb_type),
            lift_coeff: get_bomb_lift_coeff(bomb_type),
            cross_section: get_bomb_cross_section(bomb_type),
            spin: get_bomb_spin(bomb_type),
            armed: true,
            fuze_type: FuzeType::Impact,
            release_time: 0,
            guid_target: None,
        }
    }

    pub fn explode(&mut self) {
        self.armed = false;
        // Explosion handled by simulation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FuzeType {
    Impact,
    Delay,
    Proximity,
    Airburst,
    Contact,
}

// ============================================================================
// Bomb Simulation Manager
// ============================================================================

pub struct BombSimulation {
    bombs: HashMap<u32, BombPhysics>,
    next_id: u32,
    explosions: Vec<ExplosionEvent>,
}

impl BombSimulation {
    pub fn new() -> Self {
        Self {
            bombs: HashMap::new(),
            next_id: 1,
            explosions: Vec::new(),
        }
    }

    pub fn spawn_bomb(&mut self, bomb_type: BombType, position: Vec3, velocity: Vec3) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let mut bomb = BombPhysics::new(id, bomb_type, position, velocity);
        bomb.release_time = crate::physics::SIMULATION_TICK.load(Ordering::Relaxed);

        // Set fuze type based on bomb
        bomb.fuze_type = match bomb_type {
            BombType::GBU43 | BombType::TsarBomba => FuzeType::Airburst,
            BombType::LittleBoy | BombType::FatMan => FuzeType::Proximity,
            BombType::BLU109 | BombType::GBU28 | BombType::GrandSlam => FuzeType::Delay,
            _ => FuzeType::Impact,
        };

        self.bombs.insert(id, bomb);
        id
    }

    pub fn get_active_bombs(&self) -> Vec<&BombPhysics> {
        self.bombs.values().collect()
    }

    pub fn get_bomb_mut(&mut self, id: u32) -> Option<&mut BombPhysics> {
        self.bombs.get_mut(&id)
    }

    pub fn remove_bomb(&mut self, id: u32) -> Option<BombPhysics> {
        self.bombs.remove(&id)
    }

    pub fn add_explosion(&mut self, explosion: ExplosionEvent) {
        self.explosions.push(explosion);
    }

    pub fn take_explosions(&mut self) -> Vec<ExplosionEvent> {
        std::mem::take(&mut self.explosions)
    }

    pub fn clear(&mut self) {
        self.bombs.clear();
        self.explosions.clear();
    }
}

// ============================================================================
// Explosion Events
// ============================================================================

#[derive(Debug, Clone)]
pub struct ExplosionEvent {
    pub position: Vec3,
    pub bomb_type: BombType,
    pub yield_kt: f32,
    pub timestamp: u64,
}

impl ExplosionEvent {
    pub fn new(position: Vec3, bomb_type: BombType) -> Self {
        let yield_kt = match bomb_type {
            BombType::LittleBoy => 15.0,
            BombType::FatMan => 21.0,
            BombType::TsarBomba => 50000.0,
            BombType::GBU43 => 0.011, // 11 tons TNT equivalent
            BombType::GrandSlam => 0.006, // 6 tons TNT equivalent
            _ => 0.0,
        };

        Self {
            position,
            bomb_type,
            yield_kt,
            timestamp: crate::physics::SIMULATION_TICK.load(Ordering::Relaxed),
        }
    }

    pub fn fireball_radius(&self) -> f32 {
        // Nuclear fireball radius: R = 100 * Y^0.4 (meters, Y in kt)
        if self.yield_kt > 0.0 {
            100.0 * self.yield_kt.powf(0.4)
        } else {
            0.0
        }
    }

    pub fn blast_radius_1psi(&self) -> f32 {
        // 1 psi overpressure radius (light damage)
        if self.yield_kt > 0.0 {
            1000.0 * self.yield_kt.powf(0.33)
        } else {
            50.0 // Conventional blast
        }
    }

    pub fn blast_radius_5psi(&self) -> f32 {
        // 5 psi overpressure radius (moderate damage)
        if self.yield_kt > 0.0 {
            400.0 * self.yield_kt.powf(0.33)
        } else {
            20.0
        }
    }

    pub fn blast_radius_20psi(&self) -> f32 {
        // 20 psi overpressure radius (severe damage)
        if self.yield_kt > 0.0 {
            200.0 * self.yield_kt.powf(0.33)
        } else {
            10.0
        }
    }

    pub fn thermal_radius(&self) -> f32 {
        // Thermal radiation radius (3rd degree burns)
        if self.yield_kt > 0.0 {
            2000.0 * self.yield_kt.powf(0.4)
        } else {
            0.0
        }
    }

    pub fn radiation_radius(&self) -> f32 {
        // Initial radiation (500 rem)
        if self.yield_kt > 0.0 {
            1500.0 * self.yield_kt.powf(0.33)
        } else {
            0.0
        }
    }

    pub fn crater_radius(&self) -> f32 {
        // Crater radius for ground bursts
        if self.yield_kt > 0.0 {
            50.0 * self.yield_kt.powf(0.33)
        } else {
            5.0
        }
    }

    pub fn crater_depth(&self) -> f32 {
        self.crater_radius() * 0.2
    }
}

// ============================================================================
// Plane Physics State
// ============================================================================

#[derive(Debug, Clone)]
pub struct PlanePhysics {
    pub id: u32,
    pub plane_type: PlaneType,
    pub position: Vec3,
    pub velocity: Vec3,
    pub rotation: UnitQuaternion<f32>, // Orientation
    pub angular_velocity: Vec3,
    pub throttle: f32, // 0.0 - 1.0
    pub flap_setting: f32, // 0.0 - 1.0
    pub gear_down: bool,
    pub bomb_bay_open: bool,
    pub wing_sweep: f32, // For B-1B (0.0 - 1.0)
    pub bombs: Vec<(BombType, u32)>, // (type, station_id)
    pub selected_bomb: Option<usize>,
    pub fuel: f32, // kg
    pub max_fuel: f32,
    pub damage: PlaneDamage,
    pub autopilot: AutopilotState,
    pub player_id: Option<u32>, // If player-controlled
}

impl PlanePhysics {
    pub fn new(id: u32, plane_type: PlaneType, position: Vec3, player_id: Option<u32>) -> Self {
        use crate::physics::*;
        Self {
            id,
            plane_type,
            position,
            velocity: Vec3::zeros(),
            rotation: UnitQuaternion::identity(),
            angular_velocity: Vec3::zeros(),
            throttle: 0.0,
            flap_setting: 0.0,
            gear_down: true,
            bomb_bay_open: false,
            wing_sweep: 0.0,
            bombs: Vec::new(),
            selected_bomb: None,
            fuel: get_plane_max_fuel(plane_type),
            max_fuel: get_plane_max_fuel(plane_type),
            damage: PlaneDamage::default(),
            autopilot: AutopilotState::default(),
            player_id,
        }
    }

    pub fn forward(&self) -> Vec3 {
        self.rotation * Vec3::new(0.0, 0.0, -1.0)
    }

    pub fn up(&self) -> Vec3 {
        self.rotation * Vec3::new(0.0, 1.0, 0.0)
    }

    pub fn right(&self) -> Vec3 {
        self.rotation * Vec3::new(1.0, 0.0, 0.0)
    }

    pub fn pitch(&self) -> f32 {
        // Extract pitch from quaternion
        let (roll, pitch, yaw) = self.rotation.euler_angles();
        pitch
    }

    pub fn roll(&self) -> f32 {
        let (roll, _, _) = self.rotation.euler_angles();
        roll
    }

    pub fn yaw(&self) -> f32 {
        let (_, _, yaw) = self.rotation.euler_angles();
        yaw
    }

    pub fn speed(&self) -> f32 {
        self.velocity.norm()
    }

    pub fn altitude(&self) -> f32 {
        self.position.y
    }

    pub fn load_bomb(&mut self, bomb_type: BombType) -> bool {
        if self.bombs.len() >= self.plane_type.bomb_capacity() {
            return false;
        }
        if !self.plane_type.compatible_bombs().contains(&bomb_type) {
            return false;
        }
        let station = self.bombs.len() as u32;
        self.bombs.push((bomb_type, station));
        true
    }

    pub fn select_next_bomb(&mut self) {
        if self.bombs.is_empty() {
            self.selected_bomb = None;
        } else {
            let next = match self.selected_bomb {
                Some(i) if i + 1 < self.bombs.len() => i + 1,
                _ => 0,
            };
            self.selected_bomb = Some(next);
        }
    }

    pub fn drop_bomb(&mut self, bomb_sim: &mut BombSimulation) -> Option<u32> {
        let idx = self.selected_bomb?;
        if idx >= self.bombs.len() {
            return None;
        }

        let (bomb_type, _) = self.bombs.remove(idx);
        if self.selected_bomb.is_some() && self.selected_bomb.unwrap() >= self.bombs.len() {
            self.selected_bomb = if self.bombs.is_empty() { None } else { Some(self.bombs.len() - 1) };
        }

        // Calculate release velocity (plane velocity + small ejection velocity)
        let release_pos = self.position + self.up() * -3.0; // Below bomb bay
        let release_vel = self.velocity + self.forward() * 5.0; // Slight forward ejection

        Some(bomb_sim.spawn_bomb(bomb_type, release_pos, release_vel))
    }

    pub fn open_bomb_bay(&mut self) {
        self.bomb_bay_open = true;
    }

    pub fn close_bomb_bay(&mut self) {
        self.bomb_bay_open = false;
    }

    pub fn update_physics(&mut self, dt: f32) {
        // This will be called from native_tick with full physics
        // For now, simple integration
        if self.player_id.is_some() {
            // Player controlled - physics handled in flight.rs
        } else {
            // AI controlled - simple cruise
            let target_speed = self.plane_type.max_speed_mps() * self.throttle;
            let forward = self.forward();
            let speed = self.velocity.norm();

            if speed < target_speed {
                self.velocity += forward * (target_speed - speed) * dt * 0.1;
            }

            // Simple altitude hold
            if self.altitude() < 100.0 {
                self.velocity.y += 5.0 * dt;
            }
        }

        self.position += self.velocity * dt;

        // Fuel consumption
        self.fuel = (self.fuel - dt * self.plane_type.max_speed_mps() * 0.01).max(0.0);
    }
}

// ============================================================================
// Plane Damage System
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaneDamage {
    pub engine_1: f32, // 0.0 = healthy, 1.0 = destroyed
    pub engine_2: f32,
    pub engine_3: f32,
    pub engine_4: f32,
    pub left_wing: f32,
    pub right_wing: f32,
    pub tail: f32,
    pub fuselage: f32,
    pub hydraulics: f32,
    pub fuel_leak: f32,
    pub fire: f32,
}

impl PlaneDamage {
    pub fn total(&self) -> f32 {
        (self.engine_1 + self.engine_2 + self.engine_3 + self.engine_4
            + self.left_wing + self.right_wing + self.tail + self.fuselage) / 8.0
    }

    pub fn is_destroyed(&self) -> bool {
        self.total() > 0.8 || self.fire > 0.9
    }

    pub fn engines_operational(&self) -> usize {
        [self.engine_1, self.engine_2, self.engine_3, self.engine_4]
            .iter()
            .filter(|&&d| d < 0.5)
            .count()
    }
}

// ============================================================================
// Autopilot
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutopilotState {
    pub mode: AutopilotMode,
    pub target_altitude: f32,
    pub target_heading: f32,
    pub target_speed: f32,
    pub engaged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutopilotMode {
    Off,
    Level,
    AltitudeHold,
    HeadingHold,
    AltitudeHeading,
    AutoBomb, // Auto bomb run
}

impl Default for AutopilotMode {
    fn default() -> Self {
        AutopilotMode::Off
    }
}

// ============================================================================
// Plane Simulation Manager
// ============================================================================

pub struct PlaneSimulation {
    planes: HashMap<u32, PlanePhysics>,
    next_id: u32,
}

impl PlaneSimulation {
    pub fn new() -> Self {
        Self {
            planes: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn spawn_plane(&mut self, plane_type: PlaneType, position: Vec3, player_id: Option<u32>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let plane = PlanePhysics::new(id, plane_type, position, player_id);
        self.planes.insert(id, plane);
        id
    }

    pub fn get_active_planes(&self) -> Vec<&PlanePhysics> {
        self.planes.values().collect()
    }

    pub fn get_plane_mut(&mut self, id: u32) -> Option<&mut PlanePhysics> {
        self.planes.get_mut(&id)
    }

    pub fn remove_plane(&mut self, id: u32) -> Option<PlanePhysics> {
        self.planes.remove(&id)
    }

    pub fn clear(&mut self) {
        self.planes.clear();
    }
}

// ============================================================================
// Flight Session (First-person)
// ============================================================================

#[derive(Debug, Clone)]
pub struct FlightSession {
    pub plane_id: u32,
    pub plane_type: PlaneType,
    pub camera: FlightCamera,
    pub hud: FlightHUD,
    pub controls: FlightControls,
    pub bomb_picker_open: bool,
    pub selected_bomb_type: Option<BombType>,
}

impl FlightSession {
    pub fn new(plane_id: u32, plane_type: PlaneType) -> Self {
        Self {
            plane_id,
            plane_type,
            camera: FlightCamera::default(),
            hud: FlightHUD::default(),
            controls: FlightControls::default(),
            bomb_picker_open: false,
            selected_bomb_type: None,
        }
    }
}

// ============================================================================
// Flight Camera
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct FlightCamera {
    pub mode: CameraMode,
    pub position: Vec3,
    pub rotation: UnitQuaternion<f32>,
    pub fov: f32,
    pub shake: f32,
    pub g_force_effect: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CameraMode {
    #[default]
    Cockpit,
    Chase,
    Target,
    Free,
}

// ============================================================================
// Flight HUD
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct FlightHUD {
    pub show_instruments: bool,
    pub show_targeting: bool,
    pub show_trajectory: bool,
    pub mode: HUDMode,
    pub target_lock: Option<Vec3>,
    pub release_indicator: Option<f32>, // 0.0 - 1.0, when to release
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HUDMode {
    #[default]
    Normal,
    Bombing,
    Navigation,
    Combat,
}

// ============================================================================
// Flight Controls
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct FlightControls {
    pub pitch: f32,    // -1.0 to 1.0
    pub roll: f32,     // -1.0 to 1.0
    pub yaw: f32,      // -1.0 to 1.0
    pub throttle: f32, // 0.0 to 1.0
    pub flaps: f32,    // 0.0 to 1.0
    pub gear: bool,
    pub bomb_bay: bool,
    pub brake: bool,
    pub trim_pitch: f32,
    pub trim_roll: f32,
    pub trim_yaw: f32,
}

// ============================================================================
// Mod Settings
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModSettings {
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

impl Default for ModSettings {
    fn default() -> Self {
        Self {
            physics_realism: PhysicsRealism::Realistic,
            damage_enabled: true,
            nuke_effects: true,
            fallout_enabled: true,
            camera_shake: true,
            g_force_effects: true,
            auto_level_assist: false,
            bomb_trajectory_preview: true,
            friendly_fire: false,
            unlimited_ammo: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhysicsRealism {
    Arcade,
    Simplified,
    Realistic,
    Hardcore,
}

// ============================================================================
// Utility Functions
// ============================================================================

pub fn next_bomb_id() -> u32 {
    static COUNTER: AtomicU32 = AtomicU32::new(1000);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub fn next_plane_id() -> u32 {
    static COUNTER: AtomicU32 = AtomicU32::new(2000);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Calculate max fuel for plane type (kg)
pub fn get_plane_max_fuel(plane_type: PlaneType) -> f32 {
    match plane_type {
        PlaneType::B17G => 6600.0,
        PlaneType::B24 => 8200.0,
        PlaneType::B29 => 27000.0,
        PlaneType::B52H => 181000.0,
        PlaneType::B1B => 120000.0,
        PlaneType::B2 => 75000.0,
    }
}

/// Get bomb model name for rendering
pub fn get_bomb_model_name(bomb_type: BombType) -> &'static str {
    match bomb_type {
        BombType::Mk81 => "mk81_detailed",
        BombType::Mk82 => "mk82_detailed",
        BombType::Mk83 => "mk83_detailed",
        BombType::Mk84 => "mk84_detailed",
        BombType::BLU109 => "blu109_detailed",
        BombType::GBU31 => "gbu31_detailed",
        BombType::GBU28 => "gbu28_detailed",
        BombType::GBU43 => "gbu43_detailed",
        BombType::GrandSlam => "grandslam_detailed",
        BombType::LittleBoy => "littleboy_detailed",
        BombType::FatMan => "fatman_detailed",
        BombType::TsarBomba => "tsarbomba_detailed",
    }
}

/// Get plane model name for rendering
pub fn get_plane_model_name(plane_type: PlaneType) -> &'static str {
    match plane_type {
        PlaneType::B17G => "b17g_detailed",
        PlaneType::B24 => "b24_detailed",
        PlaneType::B29 => "b29_detailed",
        PlaneType::B52H => "b52h_detailed",
        PlaneType::B1B => "b1b_detailed",
        PlaneType::B2 => "b2_detailed",
    }
}