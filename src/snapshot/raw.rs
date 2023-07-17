use std::collections::HashSet;

use bevy::{
    ecs::{entity::EntityMap, reflect::ReflectMapEntities, system::CommandQueue},
    prelude::*,
    reflect::TypeRegistration,
    utils::hashbrown::hash_map::Entry,
};

use crate::{entity::SaveableEntity, prelude::*};

pub(crate) struct RawSnapshot {
    pub(crate) resources: Vec<Box<dyn Reflect>>,
    pub(crate) entities: Vec<SaveableEntity>,
}

impl RawSnapshot {
    pub(crate) fn default() -> Self {
        Self {
            resources: Vec::default(),
            entities: Vec::default(),
        }
    }
}

#[doc(hidden)]
impl<'w, F> Build for Builder<'w, RawSnapshot, F>
where
    F: Fn(&&TypeRegistration) -> bool,
{
    type Output = RawSnapshot;

    fn extract_entities(mut self, entities: impl Iterator<Item = Entity>) -> Self {
        let registry_arc = self.world.resource::<AppTypeRegistry>();
        let registry = registry_arc.read();

        let saveables = self.world.resource::<SaveableRegistry>();

        for entity in entities {
            let mut entry = SaveableEntity {
                entity: entity.to_bits(),
                components: Vec::new(),
            };

            let entity = self.world.entity(entity);

            for component_id in entity.archetype().components() {
                let reflect = self
                    .world
                    .components()
                    .get_info(component_id)
                    .filter(|info| saveables.contains(info.name()))
                    .and_then(|info| info.type_id())
                    .and_then(|id| registry.get(id))
                    .filter(&self.filter)
                    .and_then(|reg| reg.data::<ReflectComponent>())
                    .and_then(|reflect| reflect.reflect(entity));

                if let Some(reflect) = reflect {
                    entry.components.push(reflect.clone_value());
                }
            }

            self.entities.insert(entity.id(), entry);
        }

        self
    }

    fn extract_all_entities(self) -> Self {
        let world = self.world;
        self.extract_entities(world.iter_entities().map(|e| e.id()))
    }

    fn extract_resources<S: Into<String>>(mut self, resources: impl Iterator<Item = S>) -> Self {
        let names = resources.map(|s| s.into()).collect::<HashSet<String>>();

        let mut builder = Builder::new::<RawSnapshot>(self.world)
            .filter(|reg: &&TypeRegistration| names.contains(reg.type_name()) && (self.filter)(reg))
            .extract_all_resources();

        self.resources.append(&mut builder.resources);

        self
    }

    fn extract_all_resources(mut self) -> Self {
        let registry_arc = self.world.resource::<AppTypeRegistry>();
        let registry = registry_arc.read();

        let saveables = self.world.resource::<SaveableRegistry>();

        saveables
            .types()
            .filter_map(|name| Some((name, registry.get_with_name(name)?)))
            .filter(|(_, reg)| (self.filter)(reg))
            .filter_map(|(name, reg)| Some((name, reg.data::<ReflectResource>()?)))
            .filter_map(|(name, res)| Some((name, res.reflect(self.world)?)))
            .map(|(name, reflect)| (name, reflect.clone_value()))
            .for_each(|(name, reflect)| {
                self.resources.insert(name.clone(), reflect);
            });

        self
    }

    fn clear_entities(mut self) -> Self {
        self.entities.clear();
        self
    }

    fn clear_resources(mut self) -> Self {
        self.resources.clear();
        self
    }

    fn clear_empty(mut self) -> Self {
        self.entities.retain(|_, e| !e.is_empty());
        self
    }

    fn build(self) -> Self::Output {
        RawSnapshot {
            resources: self.resources.into_values().collect(),
            entities: self.entities.into_values().collect(),
        }
    }
}

impl<'a> Applier<'a, &'a RawSnapshot> {
    pub(crate) fn apply(self) -> Result<(), SaveableError> {
        let registry_arc = self.world.resource::<AppTypeRegistry>().clone();
        let registry = registry_arc.read();

        // Resources

        for resource in &self.snapshot.resources {
            let reg = registry
                .get_with_name(resource.type_name())
                .ok_or_else(|| SaveableError::UnregisteredType {
                    type_name: resource.type_name().to_string(),
                })?;

            let data = reg.data::<ReflectResource>().ok_or_else(|| {
                SaveableError::UnregisteredResource {
                    type_name: resource.type_name().to_string(),
                }
            })?;

            data.insert(self.world, resource.as_reflect());

            if let Some(mapper) = reg.data::<ReflectMapEntities>() {
                mapper
                    .map_entities(self.world, &self.map)
                    .map_err(SaveableError::MapEntitiesError)?;
            }
        }

        // Entities

        let despawn_default = self
            .world
            .get_resource::<AppDespawnMode>()
            .cloned()
            .unwrap_or_default();

        let despawn = self.despawn.as_ref().unwrap_or(&despawn_default);

        match despawn {
            DespawnMode::Missing | DespawnMode::MissingWith(_) => {
                let valid = self
                    .snapshot
                    .entities
                    .iter()
                    .map(|e| e.try_map(&self.map))
                    .collect::<HashSet<_>>();

                let mut invalid = self
                    .world
                    .iter_entities()
                    .map(|e| e.id())
                    .filter(|e| !valid.contains(e))
                    .collect::<Vec<_>>();

                if let DespawnMode::MissingWith(filter) = despawn {
                    let matches = filter.collect(self.world);
                    invalid.retain(|e| matches.contains(e));
                }

                for entity in invalid {
                    self.world.despawn(entity);
                }
            }

            DespawnMode::Unmapped | DespawnMode::UnmappedWith(_) => {
                let valid = self
                    .snapshot
                    .entities
                    .iter()
                    .filter_map(|e| e.map(&self.map))
                    .collect::<HashSet<_>>();

                let mut invalid = self
                    .world
                    .iter_entities()
                    .map(|e| e.id())
                    .filter(|e| !valid.contains(e))
                    .collect::<Vec<_>>();

                if let DespawnMode::UnmappedWith(filter) = despawn {
                    let matches = filter.collect(self.world);
                    invalid.retain(|e| matches.contains(e));
                }

                for entity in invalid {
                    self.world.despawn(entity);
                }
            }
            DespawnMode::All => {
                let invalid = self
                    .world
                    .iter_entities()
                    .map(|e| e.id())
                    .collect::<Vec<_>>();

                for entity in invalid {
                    self.world.despawn(entity);
                }
            }
            DespawnMode::AllWith(filter) => {
                let invalid = filter.collect(self.world);

                for entity in invalid {
                    self.world.despawn(entity);
                }
            }
            DespawnMode::None => {}
        }

        let mapping_default = self
            .world
            .get_resource::<AppMappingMode>()
            .cloned()
            .unwrap_or_default();

        let mapping = self.mapping.as_ref().unwrap_or(&mapping_default);

        // the entity map should always contain all existing entities, because it gets applied to the whole world.
        let mut fallback = EntityMap::default();
        for entity in self.world.iter_entities() {
            fallback.insert(entity.id(), entity.id());
        }

        let mut spawned = Vec::new();

        // Apply snapshot entities
        for saved in &self.snapshot.entities {
            let index = saved.entity;
            let old_entity = Entity::from_bits(index);

            // If Simple, attempt to map an existing entity ID first. If Strict, get a new entity
            let entity = match &mapping {
                MappingMode::Simple => saved
                    .map(&self.map)
                    .or_else(|| fallback.get(old_entity).ok())
                    .unwrap_or_else(|| self.world.spawn_empty().id()),
                MappingMode::Strict => self.world.spawn_empty().id(),
            };

            fallback.insert(old_entity, entity);

            spawned.push(entity);

            let entity_mut = &mut self.world.entity_mut(entity);

            for component in &saved.components {
                let reg = registry
                    .get_with_name(component.type_name())
                    .ok_or_else(|| SaveableError::UnregisteredType {
                        type_name: component.type_name().to_string(),
                    })?;

                let data = reg.data::<ReflectComponent>().ok_or_else(|| {
                    SaveableError::UnregisteredComponent {
                        type_name: component.type_name().to_string(),
                    }
                })?;

                data.apply_or_insert(entity_mut, &**component);
            }
        }

        for reg in registry.iter() {
            if let Some(mapper) = reg.data::<ReflectMapEntities>() {
                mapper
                    .map_entities(self.world, &fallback)
                    .map_err(SaveableError::MapEntitiesError)?;
            }
        }

        // TODO: Fix `Children`
        // If the `Children` type is included in saves, it may contain references to entities that no longer exist. We
        // should probably filter them out.
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, self.world);
        for &entity in &spawned {
            let entity_ref = self.world.entity(entity);
            let mut entity_mut = commands.entity(entity);
            if let Some(parent) = entity_ref.get::<Parent>() {
                entity_mut.set_parent(parent.get());
            }
        }
        queue.apply(self.world);

        // Entity hook
        if let Some(hook) = &self.hook {
            let mut queue = CommandQueue::default();
            let mut commands = Commands::new(&mut queue, self.world);

            for entity in spawned {
                let entity_ref = self.world.entity(entity);
                let mut entity_mut = commands.entity(entity);

                hook(&entity_ref, &mut entity_mut);
            }

            queue.apply(self.world);
        }

        Ok(())
    }
}

impl CloneReflect for RawSnapshot {
    fn clone_value(&self) -> Self {
        Self {
            resources: self.resources.clone_value(),
            entities: self.entities.iter().map(|e| e.clone_value()).collect(),
        }
    }
}
