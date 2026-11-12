//! Bomber Mod Rendering System
//!
//! GPU-instanced rendering for planes, bombs, explosions, and effects.
//! Integrates with Horizon's render pipeline.

use crate::bomber_runtime::{
    BombType, PlaneType, Vec3, UnitQuaternion,
    get_plane_model, get_bomb_model,
};
use crate::flight::{get_flight_hud_data, FlightHUDData};
use crate::ui::{get_radial_menu_data, RadialMenuData};
use crate::bomb::{get_active_bomb_entities, take_explosions, BombEntity};
use crate::nuke::{get_active_nukes, NukeExplosion, ShockwaveParticle, CloudParticle};
use nalgebra::{Vector3, Vector2, Matrix4, UnitQuaternion as NalgebraUnitQuaternion};
use std::collections::HashMap;

/// Render context provided by Horizon
pub trait RenderContext {
    fn submit_instanced(&mut self, mesh: u32, instances: &[InstanceData], material: u32);
    fn submit_mesh(&mut self, mesh: u32, transform: Matrix4<f32>, material: u32);
    fn submit_particles(&mut self, particles: &[ParticleInstance], material: u32);
    fn get_view_projection(&self) -> Matrix4<f32>;
    fn get_camera_position(&self) -> Vec3;
}

/// Instance data for GPU instancing
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct InstanceData {
    pub model_matrix: [[f32; 4]; 4],
    pub color: [f32; 4],
    pub uv_offset: [f32; 2],
    pub animation_frame: f32,
}

/// Particle instance data
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct ParticleInstance {
    pub position: [f32; 3],
    pub radius: f32,
    pub color: [f32; 4],
    pub velocity: [f32; 3],
    pub lifetime: f32,
    pub frame: f32,
}

/// Model registry
struct ModelRegistry {
    plane_models: HashMap<PlaneType, ModelHandle>,
    bomb_models: HashMap<BombType, ModelHandle>,
    explosion_materials: HashMap<String, u32>,
}

struct ModelHandle {
    mesh_id: u32,
    material_id: u32,
    bone_count: usize,
}

impl ModelRegistry {
    fn new() -> Self {
        Self {
            plane_models: HashMap::new(),
            bomb_models: HashMap::new(),
            explosion_materials: HashMap::new(),
        }
    }

    fn load_models(&mut self, render: &mut dyn RenderContext) {
        // Load plane models from Blockbench files
        for plane_type in PlaneType::all() {
            let model_name = get_plane_model(*plane_type);
            if let Some(handle) = load_model(render, model_name) {
                self.plane_models.insert(*plane_type, handle);
            }
        }

        // Load bomb models
        for bomb_type in BombType::all() {
            let model_name = get_bomb_model(*bomb_type);
            if let Some(handle) = load_model(render, model_name) {
                self.bomb_models.insert(*bomb_type, handle);
            }
        }

        // Create explosion materials
        self.explosion_materials.insert("fire".to_string(), create_fire_material(render));
        self.explosion_materials.insert("smoke".to_string(), create_smoke_material(render));
        self.explosion_materials.insert("shockwave".to_string(), create_shockwave_material(render));
        self.explosion_materials.insert("nuclear_glow".to_string(), create_nuclear_glow_material(render));
        self.explosion_materials.insert("fallout".to_string(), create_fallout_material(render));
    }

    fn get_plane_model(&self, plane_type: PlaneType) -> Option<&ModelHandle> {
        self.plane_models.get(&plane_type)
    }

    fn get_bomb_model(&self, bomb_type: BombType) -> Option<&ModelHandle> {
        self.bomb_models.get(&bomb_type)
    }
}

static MODEL_REGISTRY: std::sync::OnceLock<std::sync::Mutex<ModelRegistry>> = std::sync::OnceLock::new();

fn get_registry() -> &'static std::sync::Mutex<ModelRegistry> {
    MODEL_REGISTRY.get_or_init(|| std::sync::Mutex::new(ModelRegistry::new()))
}

/// Initialize rendering system
pub fn init_rendering(render: &mut dyn RenderContext) {
    get_registry().lock().unwrap().load_models(render);
}

/// Main render callback (called from Horizon's render pipeline)
pub fn native_render(render: &mut dyn RenderContext) {
    let registry = get_registry().lock().unwrap();

    // Render planes
    render_planes(render, &registry);

    // Render bombs
    render_bombs(render, &registry);

    // Render explosions
    render_explosions(render, &registry);

    // Render nuclear effects
    render_nuclear_effects(render, &registry);
}

/// Render all active planes
fn render_planes(render: &mut dyn RenderContext, registry: &ModelRegistry) {
    // Get active planes from simulation
    // This would integrate with the plane simulation

    // For each plane, build instance data
    // render.submit_instanced(mesh, instances, material);
}

/// Render all active bombs
fn render_bombs(render: &mut dyn RenderContext, registry: &ModelRegistry) {
    let bombs = get_active_bomb_entities();
    if bombs.is_empty() {
        return;
    }

    // Group by bomb type for instancing
    let mut by_type: HashMap<BombType, Vec<&BombEntity>> = HashMap::new();
    for bomb in &bombs {
        by_type.entry(bomb.physics.bomb_type).or_default().push(bomb);
    }

    for (bomb_type, entities) in by_type {
        if let Some(model) = registry.get_bomb_model(bomb_type) {
            let mut instances = Vec::with_capacity(entities.len());

            for entity in entities {
                let p = &entity.physics;

                // Build model matrix
                let translation = Matrix4::new_translation(&Vector3::new(p.position.x, p.position.y, p.position.z));
                let rotation = Matrix4::from(p.rotation.into_inner());
                let scale = Matrix4::new_nonuniform_scaling(&Vector3::new(1.0, 1.0, 1.0));

                let model_matrix = translation * rotation * scale;

                // Animation frame for fin deployment
                let anim_frame = if entity.physics.bomb_type.is_guided() {
                    entity.time_since_release.min(1.0) // Fin deploy animation
                } else {
                    0.0
                };

                instances.push(InstanceData {
                    model_matrix: model_matrix.into(),
                    color: [1.0, 1.0, 1.0, 1.0],
                    uv_offset: [0.0, 0.0],
                    animation_frame: anim_frame,
                });
            }

            render.submit_instanced(model.mesh_id, &instances, model.material_id);
        }
    }
}

/// Render conventional explosions
fn render_explosions(render: &mut dyn RenderContext, registry: &ModelRegistry) {
    let explosions = take_explosions();
    if explosions.is_empty() {
        return;
    }

    for explosion in explosions {
        // Generate explosion particles
        let particles = generate_explosion_particles(&explosion);

        if let Some(material) = registry.explosion_materials.get("fire") {
            render.submit_particles(&particles.fire, *material);
        }
        if let Some(material) = registry.explosion_materials.get("smoke") {
            render.submit_particles(&particles.smoke, *material);
        }
        if let Some(material) = registry.explosion_materials.get("shockwave") {
            render.submit_particles(&particles.shockwave, *material);
        }
    }
}

/// Explosion particle groups
struct ExplosionParticles {
    fire: Vec<ParticleInstance>,
    smoke: Vec<ParticleInstance>,
    shockwave: Vec<ParticleInstance>,
}

/// Generate particles for conventional explosion
fn generate_explosion_particles(explosion: &ExplosionEvent) -> ExplosionParticles {
    let pos = explosion.position;
    let yield_kt = explosion.yield_kt;

    let mut fire = Vec::new();
    let mut smoke = Vec::new();
    let mut shockwave = Vec::new();

    let intensity = if yield_kt > 0.0 { yield_kt.powf(0.33) } else { 1.0 };
    let count = (50.0 * intensity) as usize;

    // Fire particles
    for _ in 0..count {
        let dir = random_direction();
        let dist = rand::thread_rng().gen_range(0.0..20.0 * intensity);
        fire.push(ParticleInstance {
            position: [pos.x + dir.x * dist, pos.y + dir.y * dist, pos.z + dir.z * dist],
            radius: rand::thread_rng().gen_range(2.0..8.0) * intensity,
            color: [
                1.0,
                rand::thread_rng().gen_range(0.4..0.8),
                rand::thread_rng().gen_range(0.0..0.2),
                0.8,
            ],
            velocity: [dir.x * 5.0, dir.y * 5.0 + 10.0, dir.z * 5.0],
            lifetime: 0.0,
            frame: 0.0,
        });
    }

    // Smoke particles
    for _ in 0..count * 2 {
        let dir = random_direction();
        let dist = rand::thread_rng().gen_range(0.0..30.0 * intensity);
        smoke.push(ParticleInstance {
            position: [pos.x + dir.x * dist, pos.y + dir.y * dist, pos.z + dir.z * dist],
            radius: rand::thread_rng().gen_range(5.0..20.0) * intensity,
            color: [0.3, 0.3, 0.3, 0.6],
            velocity: [dir.x * 2.0, dir.y * 2.0 + 5.0, dir.z * 2.0],
            lifetime: 0.0,
            frame: 0.0,
        });
    }

    // Shockwave ring
    for i in 0..32 {
        let angle = (i as f32 * 2.0 * std::f32::consts::PI) / 32.0;
        shockwave.push(ParticleInstance {
            position: [pos.x, pos.y + 2.0, pos.z],
            radius: 10.0 * intensity,
            color: [1.0, 0.8, 0.4, 0.8],
            velocity: [angle.cos() * 100.0, 0.0, angle.sin() * 100.0],
            lifetime: 0.0,
            frame: angle,
        });
    }

    ExplosionParticles { fire, smoke, shockwave }
}

/// Render nuclear explosion effects
fn render_nuclear_effects(render: &mut dyn RenderContext, registry: &ModelRegistry) {
    let nukes = get_active_nukes();
    if nukes.is_empty() {
        return;
    }

    for nuke in nukes {
        // Shockwave
        render_shockwave(render, registry, &nuke);

        // Mushroom cloud
        render_mushroom_cloud(render, registry, &nuke);

        // Fallout
        render_fallout(render, registry, &nuke);

        // Nuclear glow
        render_nuclear_glow(render, registry, &nuke);
    }
}

/// Render expanding shockwave
fn render_shockwave(render: &mut dyn RenderContext, registry: &ModelRegistry, nuke: &NukeExplosion) {
    if let Some(material) = registry.explosion_materials.get("shockwave") {
        let particles: Vec<ParticleInstance> = nuke.shockwave_particles.iter()
            .filter(|p| p.lifetime < p.max_lifetime)
            .map(|p| ParticleInstance {
                position: [p.position.x, p.position.y, p.position.z],
                radius: p.radius,
                color: p.color,
                velocity: [0.0, 0.0, 0.0],
                lifetime: p.lifetime,
                frame: p.lifetime / p.max_lifetime,
            })
            .collect();

        if !particles.is_empty() {
            render.submit_particles(&particles, *material);
        }
    }
}

/// Render mushroom cloud
fn render_mushroom_cloud(render: &mut dyn RenderContext, registry: &ModelRegistry, nuke: &NukeExplosion) {
    if let Some(material) = registry.explosion_materials.get("smoke") {
        let particles: Vec<ParticleInstance> = nuke.mushroom_cloud.particles.iter()
            .map(|p| ParticleInstance {
                position: [p.position.x, p.position.y, p.position.z],
                radius: p.radius,
                color: p.color,
                velocity: [p.velocity.x, p.velocity.y, p.velocity.z],
                lifetime: p.lifetime,
                frame: 0.0,
            })
            .collect();

        if !particles.is_empty() {
            render.submit_particles(&particles, *material);
        }
    }
}

/// Render fallout particles
fn render_fallout(render: &mut dyn RenderContext, registry: &ModelRegistry, nuke: &NukeExplosion) {
    if let Some(material) = registry.explosion_materials.get("fallout") {
        for zone in &nuke.fallout_zones {
            let particles: Vec<ParticleInstance> = zone.particles.iter()
                .map(|p| ParticleInstance {
                    position: [p.position.x, p.position.y, p.position.z],
                    radius: p.size,
                    color: [0.4, 0.3, 0.2, p.intensity.min(0.5)],
                    velocity: [p.velocity.x, p.velocity.y, p.velocity.z],
                    lifetime: p.lifetime,
                    frame: 0.0,
                })
                .collect();

            if !particles.is_empty() {
                render.submit_particles(&particles, *material);
            }
        }
    }
}

/// Render nuclear glow at ground zero
fn render_nuclear_glow(render: &mut dyn RenderContext, registry: &ModelRegistry, nuke: &NukeExplosion) {
    if let Some(material) = registry.explosion_materials.get("nuclear_glow") {
        let age = crate::physics::SIMULATION_TICK.load(std::sync::atomic::Ordering::Relaxed) - nuke.timestamp;
        let progress = (age as f32 * 0.05).min(1.0);
        let intensity = (1.0 - progress) * nuke.mushroom_cloud.glow_intensity;

        if intensity > 0.01 {
            let particles = vec![ParticleInstance {
                position: [nuke.event.position.x, nuke.event.position.y + 10.0, nuke.event.position.z],
                radius: nuke.fireball_radius * progress,
                color: [1.0, 0.8, 0.3, intensity * 0.5],
                velocity: [0.0, 0.0, 0.0],
                lifetime: 0.0,
                frame: progress,
            }];

            render.submit_particles(&particles, *material);
        }
    }
}

/// Generate random unit direction
fn random_direction() -> Vector3<f32> {
    let u = rand::thread_rng().gen_range(-1.0..1.0);
    let theta = rand::thread_rng().gen_range(0.0..2.0 * std::f32::consts::PI);
    let s = (1.0 - u * u).sqrt();
    Vector3::new(s * theta.cos(), u, s * theta.sin())
}

/// Load model from Blockbench file
fn load_model(render: &mut dyn RenderContext, name: &str) -> Option<ModelHandle> {
    // Horizon would load the .bbmodel file and create GPU resources
    // This is a placeholder
    None
}

/// Create fire material
fn create_fire_material(render: &mut dyn RenderContext) -> u32 {
    0 // Placeholder
}

/// Create smoke material
fn create_smoke_material(render: &mut dyn RenderContext) -> u32 {
    0 // Placeholder
}

/// Create shockwave material
fn create_shockwave_material(render: &mut dyn RenderContext) -> u32 {
    0 // Placeholder
}

/// Create nuclear glow material
fn create_nuclear_glow_material(render: &mut dyn RenderContext) -> u32 {
    0 // Placeholder
}

/// Create fallout material
fn create_fallout_material(render: &mut dyn RenderContext) -> u32 {
    0 // Placeholder
}

/// Animation system for plane models
pub fn update_plane_animations(plane: &mut crate::bomber_runtime::PlanePhysics, dt: f32) {
    // Update propeller rotation
    let prop_speed = plane.throttle * 100.0 + 10.0;
    // Would update bone rotations in model

    // Update control surfaces
    // Ailerons, elevator, rudder based on angular velocity

    // Update bomb bay doors
    // Would animate open/close

    // Update landing gear
    // Would animate retract/extend

    // B-1B wing sweep
    if plane.plane_type == PlaneType::B1B {
        // Would animate wing sweep bones
    }
}

/// Animation system for bomb models
pub fn update_bomb_animations(bomb: &mut BombEntity, dt: f32) {
    // Fin deployment animation
    if bomb.physics.bomb_type.is_guided() {
        // Would animate fin deployment
    }

    // Parachute deployment
    if bomb.parachute_deployed {
        // Would animate parachute opening
    }

    // Snakeye retarder
    // Would animate petal deployment
}

/// Get plane animation parameters for rendering
pub fn get_plane_animation_data(plane: &crate::bomber_runtime::PlanePhysics) -> PlaneAnimationData {
    PlaneAnimationData {
        propeller_rotation: plane.throttle * 1000.0,
        aileron_left: -plane.angular_velocity.x * 0.5,
        aileron_right: plane.angular_velocity.x * 0.5,
        elevator: -plane.angular_velocity.y * 0.5,
        rudder: -plane.angular_velocity.z * 0.5,
        bomb_bay: if plane.bomb_bay_open { 1.0 } else { 0.0 },
        gear: if plane.gear_down { 1.0 } else { 0.0 },
        wing_sweep: plane.wing_sweep,
        flaps: plane.flap_setting,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PlaneAnimationData {
    pub propeller_rotation: f32,
    pub aileron_left: f32,
    pub aileron_right: f32,
    pub elevator: f32,
    pub rudder: f32,
    pub bomb_bay: f32,
    pub gear: f32,
    pub wing_sweep: f32,
    pub flaps: f32,
}

/// Get bomb animation parameters
pub fn get_bomb_animation_data(bomb: &BombEntity) -> BombAnimationData {
    let fin_deploy = if bomb.physics.bomb_type.is_guided() {
        bomb.time_since_release.min(1.0)
    } else {
        0.0
    };

    let parachute = if bomb.parachute_deployed { 1.0 } else { 0.0 };

    BombAnimationData {
        fin_deploy,
        parachute,
        snakeye: 0.0, // Would track separately
        canard: 0.0,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BombAnimationData {
    pub fin_deploy: f32,
    pub parachute: f32,
    pub snakeye: f32,
    pub canard: f32,
}