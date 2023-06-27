use bevy::prelude::*;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_save::prelude::*;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct Player;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct Head;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct DespawnOnLoad;

fn setup(mut commands: Commands) {
    commands
        .spawn((SpatialBundle::default(), Player, DespawnOnLoad))
        .with_children(|p| {
            p.spawn((Head, DespawnOnLoad));
        });

    // to reproduce the error, hit the following keys:
    // P         - Print head info and check parent exists
    // ENTER     - Save
    // R         - Reset, delete all player entities
    // BACKSPACE - Load
    // P         - Print head info <== head will have an invalid parent
    println!("Controls:");
    println!("P: to print debug info on `Head` entities and to validate their parent exists");
    println!("R: to recursively delete all `Player` entities");
    println!("ENTER: Save");
    println!("BACKSPACE: Load");
}

fn interact(world: &mut World) {
    let keys = world.resource::<Input<KeyCode>>();

    if keys.just_released(KeyCode::Return) {
        info!("Save");
        world.save("example").expect("Failed to save");
    } else if keys.just_released(KeyCode::Back) {
        info!("Load");
        world.load("example").expect("Failed to load");
        world
            .load_applier("example")
            .expect("Failed to load")
            .mapping(MappingMode::Strict)
            .apply()
            .expect("Failed to apply");
    } else if keys.just_pressed(KeyCode::E) {
        info!("Info");
        for entity in world.iter_entities() {
            info!("Entity: {:?}", entity.id());
            for component_id in entity.archetype().components() {
                if let Some(component) = world.components().get_info(component_id) {
                    info!("  {:?}: {:?}", entity.id(), component.name());
                }
            }
        }
    }
}

fn handle_keys(
    keys: Res<Input<KeyCode>>,
    head_query: Query<(Entity, &Parent)>,
    despawn_query: Query<Entity, With<Player>>,
    mut commands: Commands,
) {
    // Print head debug info, check that all heads have a valid parent
    if keys.just_released(KeyCode::P) {
        println!("{} Heads", head_query.iter().len());
        for (entity, parent) in &head_query {
            println!("  Head {:?} has parent: {:?}", entity, parent.get());
            if commands.get_entity(parent.get()).is_none() {
                println!("    X - Head parent does not exist!");
            } else {
                println!("    Ok - Head parent exists, all good")
            }
        }
    }

    // Reset, delete all entities
    if keys.just_released(KeyCode::R) {
        for entity in &despawn_query {
            commands.entity(entity).despawn_recursive();
        }
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.build().set(AssetPlugin {
            asset_folder: "examples/assets".to_owned(),
            ..default()
        }))
        // Inspector
        .add_plugin(WorldInspectorPlugin::new())
        // Bevy Save
        .add_plugins(SavePlugins)
        .insert_resource(AppDespawnMode::new(DespawnMode::unmapped_with::<
            With<DespawnOnLoad>,
        >()))
        .insert_resource(AppMappingMode::new(MappingMode::Strict))
        .register_saveable::<Player>()
        .register_saveable::<Parent>()
        .register_saveable::<Head>()
        .register_saveable::<Children>()
        .register_saveable::<DespawnOnLoad>()
        .register_type::<Head>()
        .register_type::<Player>()
        .add_startup_system(setup)
        .add_system(interact)
        .add_system(handle_keys)
        .run();
}
