//! Bomber Mod for Horizon 26.3
//!
//! A hyper-realistic bomber simulation mod featuring:
//! - 6 historical bomber aircraft (B-17G, B-24, B-29, B-52H, B-1B, B-2)
//! - 12 bomb types (Mk81-84, BLU-109, GBU-31, GBU-28, GBU-43, Grand Slam, Little Boy, Fat Man, Tsar Bomba)
//! - Custom RK4 physics engine with realistic aerodynamics
//! - First-person flight mode with full cockpit instruments
//! - Battlefield 5-style radial bomb picker
//! - Nuclear explosions with block destruction, craters, fallout, and radiation
//! - GPU-instanced rendering with Blockbench models and animations

// Core modules
pub mod bomber_runtime;
pub mod physics;
pub mod plane;
pub mod bomb;
pub mod nuke;
pub mod flight;
pub mod ui;
pub mod render;
pub mod command;
pub mod settings;
pub mod key;
pub mod mods;

// Re-export main types
pub use bomber_runtime::{
    BombType, PlaneType, BombTier, Vec3,
    BombPhysics, PlanePhysics, ExplosionEvent,
    FuzeType, ModSettings, PhysicsRealism,
    SIMULATION_ENABLED, DEBUG_PHYSICS, NUKE_EFFECTS_ENABLED,
    init_simulations, with_bomb_simulation, with_plane_simulation,
    with_flight_session, with_settings, next_bomb_id, next_plane_id,
    get_plane_model_name, get_bomb_model_name,
};

// Entry points called by Horizon

/// Initialize the mod (called on load)
#[no_mangle]
pub extern "C" fn bomber_mod_init() {
    bomber_runtime::init_simulations();
    crate::render::init_rendering(&mut DummyRenderContext);
}

/// Mod metadata
#[no_mangle]
pub extern "C" fn bomber_mod_metadata() -> &'static str {
    r#"{
        "name": "Bomber Command",
        "version": "0.1.0",
        "author": "Your Name",
        "description": "Hyper-realistic bomber simulation with 6 aircraft, 12 bombs, custom physics, and nuclear effects",
        "game_version": "26.3",
        "hooks_version": 359,
        "entry_points": {
            "native_mods": "bomber::mods::native_mods",
            "native_key": "bomber::key::native_key",
            "native_key_hold": "bomber::key::native_key_hold",
            "native_mouse_move": "bomber::key::native_mouse_move",
            "native_mouse_scroll": "bomber::key::native_mouse_scroll",
            "native_gamepad_input": "bomber::key::native_gamepad_input",
            "native_command": "bomber::command::native_command",
            "native_settings": "bomber::settings::native_settings",
            "native_tick": "bomber::physics::native_tick",
            "native_render": "bomber::render::native_render",
            "native_hud": "bomber::flight::native_hud",
            "native_player_join": "bomber::plane::native_player_join",
            "native_player_leave": "bomber::plane::native_player_leave"
        }
    }"#
}

/// Called every tick (20 Hz)
#[no_mangle]
pub extern "C" fn bomber_native_tick() {
    crate::physics::native_tick();
    crate::bomb::update_bomb_entities(0.05);
    crate::nuke::process_nuke_explosions(&mut DummyWorldAccess);
}

/// Called for rendering
#[no_mangle]
pub extern "C" fn bomber_native_render(render: &mut dyn crate::render::RenderContext) {
    crate::render::native_render(render);
}

/// Called for HUD drawing
#[no_mangle]
pub extern "C" fn bomber_native_hud(batch: &mut dyn crate::flight::HudBatch, corners: &mut dyn crate::flight::HudCorners) {
    crate::flight::native_hud(batch, corners);
}

/// Key press handler
#[no_mangle]
pub extern "C" fn bomber_native_key(key: &str, pressed: bool) {
    crate::key::native_key(key, pressed);
}

/// Key hold handler
#[no_mangle]
pub extern "C" fn bomber_native_key_hold(key: &str, held: bool) {
    crate::key::native_key_hold(key, held);
}

/// Mouse move handler
#[no_mangle]
pub extern "C" fn bomber_native_mouse_move(dx: f32, dy: f32) {
    crate::key::native_mouse_move(dx, dy);
}

/// Mouse scroll handler
#[no_mangle]
pub extern "C" fn bomber_native_mouse_scroll(delta: f32) {
    crate::key::native_mouse_scroll(delta);
}

/// Gamepad input handler
#[no_mangle]
pub extern "C" fn bomber_native_gamepad_input(
    left_stick_x: f32,
    left_stick_y: f32,
    right_stick_x: f32,
    right_stick_y: f32,
    left_trigger: f32,
    right_trigger: f32,
    buttons: u32,
) {
    crate::key::native_gamepad_input(
        left_stick_x, left_stick_y,
        right_stick_x, right_stick_y,
        left_trigger, right_trigger,
        buttons,
    );
}

/// Command handler
#[no_mangle]
pub extern "C" fn bomber_native_command(args: &[&str]) -> String {
    crate::command::native_command(args)
}

/// Settings handler
#[no_mangle]
pub extern "C" fn bomber_native_settings(settings_ui: &mut dyn crate::settings::SettingsUi) {
    crate::settings::native_settings(settings_ui);
}

/// Player join handler
#[no_mangle]
pub extern "C" fn bomber_native_player_join(player_id: u32) {
    crate::plane::native_player_join(player_id);
}

/// Player leave handler
#[no_mangle]
pub extern "C" fn bomber_native_player_leave(player_id: u32) {
    crate::plane::native_player_leave(player_id);
}

/// Mod list handler
#[no_mangle]
pub extern "C" fn bomber_native_mods(mods: &mut dyn crate::mods::ModList) {
    crate::mods::native_mods(mods);
}

// Dummy implementations for initialization
struct DummyRenderContext;
impl crate::render::RenderContext for DummyRenderContext {
    fn submit_instanced(&mut self, _mesh: u32, _instances: &[crate::render::InstanceData], _material: u32) {}
    fn submit_mesh(&mut self, _mesh: u32, _transform: nalgebra::Matrix4<f32>, _material: u32) {}
    fn submit_particles(&mut self, _particles: &[crate::render::ParticleInstance], _material: u32) {}
    fn get_view_projection(&self) -> nalgebra::Matrix4<f32> { nalgebra::Matrix4::identity() }
    fn get_camera_position(&self) -> crate::bomber_runtime::Vec3 { crate::bomber_runtime::Vec3::zeros() }
}

struct DummyWorldAccess;
impl crate::nuke::WorldAccess for DummyWorldAccess {
    fn get_block(&self, _pos: crate::bomber_runtime::Vec3) -> Option<String> { None }
    fn get_block_state(&self, _pos: crate::bomber_runtime::Vec3) -> crate::nuke::BlockState { crate::nuke::BlockState::default() }
    fn set_block(&mut self, _pos: crate::bomber_runtime::Vec3, _block_id: &str, _state: crate::nuke::BlockState) {}
}

// Re-export nalgebra for convenience
pub use nalgebra::{Vector3, Vector2, Matrix4, UnitQuaternion};