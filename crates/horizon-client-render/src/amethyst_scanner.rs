//! Finds grown amethyst clusters and selected artificial-light blocks in resident chunks.
//!
//! Detection is based on block-state registry names, never on light level alone.
//! This avoids treating lava, fire, or other natural light sources as player clues.
use crate::chunk::Chunk;
use std::collections::HashSet;

/// A matching block in absolute world coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AmethystMarker {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// A selected artificial-light block in absolute world coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArtificialLightMarker {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Scan one already-loaded chunk for any of the supplied block-state IDs.
fn scan_chunk_states(
    chunk: &Chunk,
    target_states: &HashSet<u32>,
    air_state: u32,
) -> Vec<AmethystMarker> {
    let mut found = Vec::new();
    if target_states.is_empty() {
        return found;
    }
    let base_x = chunk.x * 16;
    let base_z = chunk.z * 16;

    for (section_index, section) in chunk.sections.iter().enumerate() {
        let base_y = chunk.min_y() + section_index as i32 * 16;
        if section.is_uniform(air_state) {
            continue;
        }
        if section.is_uniform(*target_states.iter().next().unwrap()) && target_states.len() == 1 {
            for local_y in 0..16 {
                for local_z in 0..16 {
                    for local_x in 0..16 {
                        found.push(AmethystMarker {
                            x: base_x + local_x,
                            y: base_y + local_y,
                            z: base_z + local_z,
                        });
                    }
                }
            }
            continue;
        }
        let mut states = [0u32; 4096];
        section.unpack(&mut states);
        for (index, state) in states.into_iter().enumerate() {
            if !target_states.contains(&state) {
                continue;
            }
            found.push(AmethystMarker {
                x: base_x + (index & 15) as i32,
                y: base_y + (index >> 8) as i32,
                z: base_z + ((index >> 4) & 15) as i32,
            });
        }
    }
    found
}

fn state_ids_named(facts: &crate::blocks::BlockFacts, names: &[&str]) -> HashSet<u32> {
    facts.states.iter().enumerate().filter_map(|(id, state)| {
        names.contains(&state.name.as_ref()).then_some(id as u32)
    }).collect()
}

fn scan_loaded_states_near(
    world: &crate::world::World,
    states: &HashSet<u32>,
    air_state: u32,
    center_chunk: (i32, i32),
    radius_chunks: i32,
) -> Vec<AmethystMarker> {
    if states.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    let radius_squared = radius_chunks.max(0).pow(2);
    for (&(chunk_x, chunk_z), chunk) in &world.chunks {
        let dx = chunk_x - center_chunk.0;
        let dz = chunk_z - center_chunk.1;
        if dx * dx + dz * dz > radius_squared {
            continue;
        }
        found.extend(scan_chunk_states(chunk, states, air_state));
    }
    found
}

/// Scan loaded chunks for fully-grown amethyst clusters (not budding blocks).
pub fn scan_loaded_chunks_near(
    world: &crate::world::World,
    facts: &crate::blocks::BlockFacts,
    center_chunk: (i32, i32),
    radius_chunks: i32,
) -> Vec<AmethystMarker> {
    let states = state_ids_named(facts, &["minecraft:amethyst_cluster"]);
    scan_loaded_states_near(world, &states, facts.air.0, center_chunk, radius_chunks)
}

/// Scan loaded chunks for explicitly selected artificial-light block types.
/// Soul and wall-mounted variants are included; lava and fire are deliberately excluded.
pub fn scan_loaded_artificial_lights_near(
    world: &crate::world::World,
    facts: &crate::blocks::BlockFacts,
    center_chunk: (i32, i32),
    radius_chunks: i32,
) -> Vec<ArtificialLightMarker> {
    const LIGHT_BLOCKS: &[&str] = &[
        "minecraft:torch",
        "minecraft:wall_torch",
        "minecraft:soul_torch",
        "minecraft:soul_wall_torch",
        "minecraft:lantern",
        "minecraft:soul_lantern",
        "minecraft:sea_lantern",
        "minecraft:glowstone",
    ];
    scan_loaded_states_near(
        world,
        &state_ids_named(facts, LIGHT_BLOCKS),
        facts.air.0,
        center_chunk,
        radius_chunks,
    )
    .into_iter()
    .map(|p| ArtificialLightMarker { x: p.x, y: p.y, z: p.z })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{Chunk, ColumnLayout};

    #[test]
    fn finds_targets_at_absolute_coordinates_across_sections() {
        let mut chunk = Chunk::with_layout(-2, 3, ColumnLayout::new(-64, 384).unwrap());
        chunk.set(4, -32, 7, 42);
        chunk.set(5, -31, 7, 99);
        chunk.set(15, 319, 0, 42);
        let found = scan_chunk_states(&chunk, &HashSet::from([42]), 0);
        assert_eq!(found, vec![
            AmethystMarker { x: -28, y: -32, z: 55 },
            AmethystMarker { x: -17, y: 319, z: 48 },
        ]);
    }

    #[test]
    fn empty_chunk_has_no_markers() {
        assert!(scan_chunk_states(&Chunk::empty(0, 0), &HashSet::from([42]), 0).is_empty());
    }
}
