//! Bomber Mod Nuclear Explosion System
//!
//! Hyper-realistic nuclear explosions with block destruction, crater generation,
//! thermal radiation, blast waves, fallout, and persistent radiation zones.

use crate::bomber_runtime::{BombType, ExplosionEvent, Vec3};
use crate::bomb::get_nuke_yield_kt;
use nalgebra::{Vector3, Vector2};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

static FALLOUT_TICK: AtomicU64 = AtomicU64::new(0);

/// Nuclear explosion parameters
#[derive(Debug, Clone)]
pub struct NukeExplosion {
    pub event: ExplosionEvent,
    pub fireball_radius: f32,
    pub blast_1psi: f32,
    pub blast_5psi: f32,
    pub blast_20psi: f32,
    pub thermal_radius: f32,
    pub radiation_radius: f32,
    pub crater_radius: f32,
    pub crater_depth: f32,
    pub blocks_destroyed: Vec<DestroyedBlock>,
    pub fallout_zones: Vec<FalloutZone>,
    pub radiation_sources: Vec<RadiationSource>,
    pub shockwave_particles: Vec<ShockwaveParticle>,
    pub mushroom_cloud: MushroomCloud,
    pub timestamp: u64,
}

/// Destroyed block record
#[derive(Debug, Clone)]
pub struct DestroyedBlock {
    pub position: Vec3,
    pub block_id: String,
    pub original_state: BlockState,
    pub destruction_type: DestructionType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestructionType {
    Vaporized,      // Inside fireball
    Shattered,      // Blast wave 20+ psi
    Broken,         // Blast wave 5-20 psi
    Displaced,      // Blast wave 1-5 psi
    Burned,         // Thermal radiation
    Irradiated,     // Radiation
}

/// Fallout zone with radiation levels
#[derive(Debug, Clone)]
pub struct FalloutZone {
    pub center: Vec3,
    pub radius: f32,
    pub intensity: f32,      // Rads per tick at center
    pub half_life_ticks: u64, // Decay rate
    pub created_tick: u64,
    pub particles: Vec<FalloutParticle>,
}

/// Fallout particle for visual effect
#[derive(Debug, Clone)]
pub struct FalloutParticle {
    pub position: Vec3,
    pub velocity: Vec3,
    pub size: f32,
    pub lifetime: f32,
    pub intensity: f32,
}

/// Radiation source for persistent exposure
#[derive(Debug, Clone)]
pub struct RadiationSource {
    pub position: Vec3,
    pub strength: f32,        // Rads per second at 1 meter
    pub radius: f32,          // Effective radius
    pub decay_rate: f32,      // Per tick
    pub created_tick: u64,
}

/// Mushroom cloud data
#[derive(Debug, Clone)]
pub struct MushroomCloud {
    pub stem_radius: f32,
    pub cap_radius: f32,
    pub stem_height: f32,
    pub cap_height: f32,
    pub particles: Vec<CloudParticle>,
    pub glow_intensity: f32,
}

#[derive(Debug, Clone)]
pub struct CloudParticle {
    pub position: Vec3,
    pub velocity: Vec3,
    pub radius: f32,
    pub color: [f32; 4],
    pub lifetime: f32,
}

/// Shockwave particle for visual effect
#[derive(Debug, Clone)]
pub struct ShockwaveParticle {
    pub position: Vec3,
    pub radius: f32,
    pub thickness: f32,
    pub color: [f32; 4],
    pub lifetime: f32,
    pub max_lifetime: f32,
}

impl NukeExplosion {
    /// Create nuclear explosion from event
    pub fn from_event(event: ExplosionEvent) -> Self {
        let yield_kt = event.yield_kt;

        // Calculate radii based on yield (Glasstone formulas)
        let fireball_radius = 100.0 * yield_kt.powf(0.4);
        let blast_1psi = 1000.0 * yield_kt.powf(0.33);
        let blast_5psi = 400.0 * yield_kt.powf(0.33);
        let blast_20psi = 200.0 * yield_kt.powf(0.33);
        let thermal_radius = 2000.0 * yield_kt.powf(0.4);
        let radiation_radius = 1500.0 * yield_kt.powf(0.33);
        let crater_radius = 50.0 * yield_kt.powf(0.33);
        let crater_depth = crater_radius * 0.2;

        // Mushroom cloud dimensions
        let stem_radius = fireball_radius * 0.5;
        let cap_radius = fireball_radius * 2.5;
        let stem_height = fireball_radius * 3.0;
        let cap_height = fireball_radius * 1.5;

        Self {
            event: event.clone(),
            fireball_radius,
            blast_1psi,
            blast_5psi,
            blast_20psi,
            thermal_radius,
            radiation_radius,
            crater_radius,
            crater_depth,
            blocks_destroyed: Vec::new(),
            fallout_zones: Vec::new(),
            radiation_sources: Vec::new(),
            shockwave_particles: Vec::new(),
            mushroom_cloud: MushroomCloud {
                stem_radius,
                cap_radius,
                stem_height,
                cap_height,
                particles: Vec::new(),
                glow_intensity: 1.0,
            },
            timestamp: crate::physics::SIMULATION_TICK.load(Ordering::Relaxed),
        }
    }

    /// Process explosion: destroy blocks, generate effects
    pub fn process_explosion(&mut self, world: &mut dyn WorldAccess) {
        let pos = self.event.position;

        // 1. Destroy blocks in fireball (vaporize)
        self.destroy_blocks_in_radius(world, pos, self.fireball_radius, DestructionType::Vaporized);

        // 2. Destroy blocks in blast zones
        self.destroy_blocks_in_radius(world, pos, self.blast_20psi, DestructionType::Shattered);
        self.destroy_blocks_in_radius(world, pos, self.blast_5psi, DestructionType::Broken);
        self.destroy_blocks_in_radius(world, pos, self.blast_1psi, DestructionType::Displaced);

        // 3. Thermal radiation - burn flammable blocks
        self.apply_thermal_effects(world, pos);

        // 4. Generate crater
        self.generate_crater(world, pos);

        // 5. Create fallout zones
        self.create_fallout(pos);

        // 6. Create radiation sources
        self.create_radiation_sources(pos);

        // 7. Initialize mushroom cloud
        self.initialize_mushroom_cloud(pos);

        // 8. Initialize shockwave
        self.initialize_shockwave(pos);
    }

    /// Destroy blocks within radius
    fn destroy_blocks_in_radius(
        &mut self,
        world: &mut dyn WorldAccess,
        center: Vec3,
        radius: f32,
        dtype: DestructionType,
    ) {
        let r2 = radius * radius;
        let min_x = (center.x - radius).floor() as i32;
        let max_x = (center.x + radius).ceil() as i32;
        let min_y = (center.y - radius).floor() as i32;
        let max_y = (center.y + radius).ceil() as i32;
        let min_z = (center.z - radius).floor() as i32;
        let max_z = (center.z + radius).ceil() as i32;

        for x in min_x..=max_x {
            for y in min_y..=max_y {
                for z in min_z..=max_z {
                    let dx = x as f32 + 0.5 - center.x;
                    let dy = y as f32 + 0.5 - center.y;
                    let dz = z as f32 + 0.5 - center.z;
                    let dist2 = dx * dx + dy * dy + dz * dz;

                    if dist2 <= r2 {
                        let block_pos = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                        if let Some(block_id) = world.get_block(block_pos) {
                            if !block_id.is_empty() && block_id != "minecraft:air" {
                                let state = world.get_block_state(block_pos);
                                world.set_block(block_pos, "minecraft:air", BlockState::default());

                                self.blocks_destroyed.push(DestroyedBlock {
                                    position: block_pos,
                                    block_id: block_id.clone(),
                                    original_state: state,
                                    destruction_type: dtype,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    /// Apply thermal radiation effects
    fn apply_thermal_effects(&mut self, world: &mut dyn WorldAccess, center: Vec3) {
        let r2 = self.thermal_radius * self.thermal_radius;
        let min_x = (center.x - self.thermal_radius).floor() as i32;
        let max_x = (center.x + self.thermal_radius).ceil() as i32;
        let min_y = (center.y - self.thermal_radius).floor() as i32;
        let max_y = (center.y + self.thermal_radius).ceil() as i32;
        let min_z = (center.z - self.thermal_radius).floor() as i32;
        let max_z = (center.z + self.thermal_radius).ceil() as i32;

        let flammable = [
            "minecraft:wood", "minecraft:leaves", "minecraft:wool",
            "minecraft:tnt", "minecraft:hay_block", "minecraft:bookshelf",
        ];

        for x in min_x..=max_x {
            for z in min_z..=max_z {
                let dx = x as f32 + 0.5 - center.x;
                let dz = z as f32 + 0.5 - center.z;
                let horiz_dist2 = dx * dx + dz * dz;

                if horiz_dist2 <= r2 {
                    // Check column for flammable blocks
                    for y in min_y..=max_y {
                        let dy = y as f32 + 0.5 - center.y;
                        let dist2 = horiz_dist2 + dy * dy;

                        if dist2 <= r2 {
                            let block_pos = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                            if let Some(block_id) = world.get_block(block_pos) {
                                if flammable.iter().any(|f| block_id.contains(f)) {
                                    world.set_block(block_pos, "minecraft:fire", BlockState::default());
                                    self.blocks_destroyed.push(DestroyedBlock {
                                        position: block_pos,
                                        block_id,
                                        original_state: BlockState::default(),
                                        destruction_type: DestructionType::Burned,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Generate crater terrain deformation
    fn generate_crater(&mut self, world: &mut dyn WorldAccess, center: Vec3) {
        let r = self.crater_radius;
        let depth = self.crater_depth;
        let r2 = r * r;

        let min_x = (center.x - r).floor() as i32;
        let max_x = (center.x + r).ceil() as i32;
        let min_z = (center.z - r).floor() as i32;
        let max_z = (center.z + r).ceil() as i32;

        for x in min_x..=max_x {
            for z in min_z..=max_z {
                let dx = x as f32 + 0.5 - center.x;
                let dz = z as f32 + 0.5 - center.z;
                let horiz_dist2 = dx * dx + dz * dz;

                if horiz_dist2 <= r2 {
                    let horiz_dist = horiz_dist2.sqrt();
                    // Crater profile: parabolic
                    let crater_depth_at = depth * (1.0 - (horiz_dist / r).powf(2.0));

                    // Lower terrain in crater area
                    let base_y = center.y - crater_depth_at;
                    let max_y = (base_y).floor() as i32;

                    for y in (max_y..=(center.y.floor() as i32)).rev() {
                        let block_pos = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                        if let Some(block_id) = world.get_block(block_pos) {
                            if !block_id.is_empty() && block_id != "minecraft:air" {
                                world.set_block(block_pos, "minecraft:air", BlockState::default());
                            }
                        }
                    }

                    // Add rim ejecta
                    if (horiz_dist / r - 0.8).abs() < 0.1 {
                        let rim_y = (center.y + depth * 0.3).floor() as i32;
                        for y in rim_y..=rim_y + 3 {
                            let block_pos = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                            if world.get_block(block_pos) == Some("minecraft:air".to_string()) {
                                world.set_block(block_pos, "minecraft:stone", BlockState::default());
                            }
                        }
                    }
                }
            }
        }
    }

    /// Create fallout zones
    fn create_fallout(&mut self, center: Vec3) {
        let yield_kt = self.event.yield_kt;

        // Main fallout zone downwind
        let wind = crate::physics::get_wind_at_position(center);
        let wind_dir = if wind.norm() > 0.1 {
            wind.normalize()
        } else {
            Vector3::new(1.0, 0.0, 0.0)
        };

        // Primary fallout zone
        let primary_radius = 500.0 * yield_kt.powf(0.4);
        let primary_center = center + wind_dir * primary_radius * 0.5;

        self.fallout_zones.push(FalloutZone {
            center: primary_center,
            radius: primary_radius,
            intensity: 100.0 * yield_kt.powf(0.3), // Rads/tick at center
            half_life_ticks: 24000, // 20 minutes at 20 ticks/sec
            created_tick: crate::physics::SIMULATION_TICK.load(Ordering::Relaxed),
            particles: Vec::new(),
        });

        // Secondary zones (crosswind spread)
        for i in 1..4 {
            let angle = (i as f32 * 90.0).to_radians();
            let offset = Vector3::new(angle.cos(), 0.0, angle.sin()) * primary_radius * 0.3;
            self.fallout_zones.push(FalloutZone {
                center: primary_center + offset,
                radius: primary_radius * 0.5,
                intensity: 50.0 * yield_kt.powf(0.3),
                half_life_ticks: 24000,
                created_tick: crate::physics::SIMULATION_TICK.load(Ordering::Relaxed),
                particles: Vec::new(),
            });
        }

        // Initialize fallout particles
        for zone in &mut self.fallout_zones {
            for _ in 0..100 {
                let angle = rand::thread_rng().gen_range(0.0..2.0 * std::f32::consts::PI);
                let dist = rand::thread_rng().gen_range(0.0..zone.radius);
                zone.particles.push(FalloutParticle {
                    position: zone.center + Vector3::new(angle.cos() * dist, 50.0, angle.sin() * dist),
                    velocity: Vector3::new(
                        wind_dir.x * 0.5 + rand::thread_rng().gen_range(-0.2..0.2),
                        -0.1,
                        wind_dir.z * 0.5 + rand::thread_rng().gen_range(-0.2..0.2),
                    ),
                    size: rand::thread_rng().gen_range(0.5..2.0),
                    lifetime: 0.0,
                    intensity: zone.intensity * 0.01,
                });
            }
        }
    }

    /// Create persistent radiation sources
    fn create_radiation_sources(&mut self, center: Vec3) {
        let yield_kt = self.event.yield_kt;

        // Ground zero source
        self.radiation_sources.push(RadiationSource {
            position: center,
            strength: 10000.0 * yield_kt.powf(0.5),
            radius: self.radiation_radius,
            decay_rate: 0.0001,
            created_tick: crate::physics::SIMULATION_TICK.load(Ordering::Relaxed),
        });

        // Crater rim sources
        for i in 0..8 {
            let angle = (i as f32 * 45.0).to_radians();
            let pos = center + Vector3::new(angle.cos() * self.crater_radius, 0.0, angle.sin() * self.crater_radius);
            self.radiation_sources.push(RadiationSource {
                position: pos,
                strength: 1000.0 * yield_kt.powf(0.3),
                radius: 50.0,
                decay_rate: 0.0002,
                created_tick: crate::physics::SIMULATION_TICK.load(Ordering::Relaxed),
            });
        }
    }

    /// Initialize mushroom cloud particles
    fn initialize_mushroom_cloud(&mut self, center: Vec3) {
        let mc = &mut self.mushroom_cloud;

        // Stem particles
        for _ in 0..200 {
            let height = rand::thread_rng().gen_range(0.0..mc.stem_height);
            let radius = rand::thread_rng().gen_range(0.0..mc.stem_radius);
            let angle = rand::thread_rng().gen_range(0.0..2.0 * std::f32::consts::PI);

            mc.particles.push(CloudParticle {
                position: center + Vector3::new(
                    angle.cos() * radius,
                    height,
                    angle.sin() * radius,
                ),
                velocity: Vector3::new(
                    rand::thread_rng().gen_range(-2.0..2.0),
                    5.0 + rand::thread_rng().gen_range(0.0..10.0),
                    rand::thread_rng().gen_range(-2.0..2.0),
                ),
                radius: rand::thread_rng().gen_range(5.0..15.0),
                color: [0.6, 0.5, 0.4, 0.7],
                lifetime: 0.0,
            });
        }

        // Cap particles
        for _ in 0..500 {
            let height = mc.stem_height + rand::thread_rng().gen_range(0.0..mc.cap_height);
            let radius = mc.cap_radius * (1.0 - (height - mc.stem_height) / mc.cap_height);
            let radius = rand::thread_rng().gen_range(0.0..radius);
            let angle = rand::thread_rng().gen_range(0.0..2.0 * std::f32::consts::PI);

            mc.particles.push(CloudParticle {
                position: center + Vector3::new(
                    angle.cos() * radius,
                    height,
                    angle.sin() * radius,
                ),
                velocity: Vector3::new(
                    rand::thread_rng().gen_range(-3.0..3.0),
                    2.0 + rand::thread_rng().gen_range(0.0..5.0),
                    rand::thread_rng().gen_range(-3.0..3.0),
                ),
                radius: rand::thread_rng().gen_range(10.0..30.0),
                color: [0.7, 0.6, 0.5, 0.5],
                lifetime: 0.0,
            });
        }
    }

    /// Initialize shockwave particles
    fn initialize_shockwave(&mut self, center: Vec3) {
        // Ground shockwave ring
        for i in 0..64 {
            let angle = (i as f32 * 2.0 * std::f32::consts::PI) / 64.0;
            self.shockwave_particles.push(ShockwaveParticle {
                position: center + Vector3::new(angle.cos() * 10.0, 2.0, angle.sin() * 10.0),
                radius: 10.0,
                thickness: 2.0,
                color: [1.0, 0.8, 0.4, 0.8],
                lifetime: 0.0,
                max_lifetime: self.blast_1psi / 300.0, // Travels at ~300 m/s
            });
        }

        // Vertical shockwave (condensation cloud)
        for i in 0..32 {
            let angle = (i as f32 * 2.0 * std::f32::consts::PI) / 32.0;
            self.shockwave_particles.push(ShockwaveParticle {
                position: center + Vector3::new(angle.cos() * 5.0, 5.0, angle.sin() * 5.0),
                radius: 5.0,
                thickness: 1.0,
                color: [1.0, 1.0, 1.0, 0.6],
                lifetime: 0.0,
                max_lifetime: self.fireball_radius / 500.0,
            });
        }
    }

    /// Update explosion effects (called each tick)
    pub fn update_effects(&mut self, dt: f32, world: &mut dyn WorldAccess) {
        let current_tick = crate::physics::SIMULATION_TICK.load(Ordering::Relaxed);
        let age_ticks = current_tick - self.timestamp;

        // Update shockwave expansion
        for particle in &mut self.shockwave_particles {
            particle.lifetime += dt;
            if particle.lifetime < particle.max_lifetime {
                let progress = particle.lifetime / particle.max_lifetime;
                particle.radius = particle.max_lifetime * 300.0 * progress; // 300 m/s expansion
                particle.color[3] = (1.0 - progress) * 0.8;
            }
        }

        // Update mushroom cloud
        for particle in &mut self.mushroom_cloud.particles {
            particle.lifetime += dt;
            particle.position += particle.velocity * dt;
            particle.velocity.y *= 0.99; // Gravity slows rise
            particle.color[3] *= 0.999;
        }
        self.mushroom_cloud.glow_intensity *= 0.9995;

        // Update fallout zones
        for zone in &mut self.fallout_zones {
            let age = current_tick - zone.created_tick;
            let decay = 0.5_f32.powf(age as f32 / zone.half_life_ticks as f32);
            zone.intensity *= decay;

            for particle in &mut zone.particles {
                particle.lifetime += dt;
                particle.position += particle.velocity * dt;
                particle.velocity.y -= 0.01 * dt; // Settling
                particle.intensity = zone.intensity * 0.01 * decay;
            }
        }

        // Update radiation sources
        self.radiation_sources.retain_mut(|source| {
            let age = current_tick - source.created_tick;
            source.strength *= (-source.decay_rate * age as f32).exp();
            source.strength > 1.0
        });

        // Apply radiation damage to entities
        self.apply_radiation_damage(world);
    }

    /// Apply radiation damage to nearby entities
    fn apply_radiation_damage(&self, world: &mut dyn WorldAccess) {
        // Would iterate over entities in radius and apply radiation damage
        // Implementation depends on Horizon's entity system
    }

    /// Get radiation level at position (rads per tick)
    pub fn get_radiation_at(&self, pos: Vec3) -> f32 {
        let mut total = 0.0;

        for source in &self.radiation_sources {
            let dist = (pos - source.position).norm();
            if dist < source.radius && dist > 0.0 {
                total += source.strength / (dist * dist);
            } else if dist <= 1.0 {
                total += source.strength;
            }
        }

        for zone in &self.fallout_zones {
            let dist = (pos - zone.center).norm();
            if dist < zone.radius {
                total += zone.intensity * (1.0 - dist / zone.radius);
            }
        }

        total
    }
}

/// World access trait (Horizon provides implementation)
trait WorldAccess {
    fn get_block(&self, pos: Vec3) -> Option<String>;
    fn get_block_state(&self, pos: Vec3) -> BlockState;
    fn set_block(&mut self, pos: Vec3, block_id: &str, state: BlockState);
}

#[derive(Debug, Clone, Default)]
pub struct BlockState {
    pub properties: HashMap<String, String>,
}

/// Process all nuclear explosions
pub fn process_nuke_explosions(world: &mut dyn WorldAccess) {
    // Would get explosions from bomb simulation and process them
    // This is called from native_tick
}

/// Get all active nuclear explosions for rendering
pub fn get_active_nukes() -> Vec<NukeExplosion> {
    Vec::new() // Would retrieve from simulation
}

/// Command to trigger nuke test
pub fn cmd_test_nuke(args: &[&str]) -> String {
    let bomb_type = if args.is_empty() {
        BombType::TsarBomba
    } else {
        match args[0].to_lowercase().as_str() {
            "littleboy" => BombType::LittleBoy,
            "fatman" => BombType::FatMan,
            "tsar" | "tsarbomba" => BombType::TsarBomba,
            _ => return "Usage: /bomber testnuke [littleboy|fatman|tsar]".to_string(),
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

    // Create explosion event
    let event = ExplosionEvent::new(pos, bomb_type);

    format!("Test {} detonation at ({:.0}, {:.0}, {:.0}) - Yield: {:.1} kt",
        bomb_type.name(), pos.x, pos.y, pos.z, event.yield_kt)
}