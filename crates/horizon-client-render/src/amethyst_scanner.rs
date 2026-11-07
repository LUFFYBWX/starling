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


/// A loaded container or utility block entity highlighted by the ESP.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockEntityMarker {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// 0 = chest/barrel, 1 = shulker, 2 = other container or utility.
    pub kind: u8,
}

/// Evidence score for one resident chunk. This is a heuristic, not a real
/// statistical probability that a player base exists there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkSuspicion {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub score: u8,
    /// Vertical bounds of the evidence that made this chunk suspicious.
    pub min_y: i32,
    pub max_y: i32,
    pub block_entities: u16,
    pub shulkers: u16,
    pub clusters: u16,
    pub artificial_lights: u16,
}

/// Finds block entities whose block state is a useful ESP target. The world
/// map contains only block entities in currently loaded chunks.
pub fn scan_loaded_block_entities_near(
    world: &crate::world::World,
    facts: &crate::blocks::BlockFacts,
    center_chunk: (i32, i32),
    radius_chunks: i32,
) -> Vec<BlockEntityMarker> {
    const TARGETS: &[&str] = &[
        "minecraft:chest", "minecraft:trapped_chest", "minecraft:barrel",
        "minecraft:ender_chest",
        "minecraft:shulker_box", "minecraft:white_shulker_box",
        "minecraft:orange_shulker_box", "minecraft:magenta_shulker_box",
        "minecraft:light_blue_shulker_box", "minecraft:yellow_shulker_box",
        "minecraft:lime_shulker_box", "minecraft:pink_shulker_box",
        "minecraft:gray_shulker_box", "minecraft:light_gray_shulker_box",
        "minecraft:cyan_shulker_box", "minecraft:purple_shulker_box",
        "minecraft:blue_shulker_box", "minecraft:brown_shulker_box",
        "minecraft:green_shulker_box", "minecraft:red_shulker_box",
        "minecraft:black_shulker_box",
        "minecraft:furnace", "minecraft:blast_furnace", "minecraft:smoker",
        "minecraft:hopper", "minecraft:dispenser", "minecraft:dropper",
        "minecraft:brewing_stand", "minecraft:crafter",
    ];
    let radius = radius_chunks.max(0);
    world.block_entities.iter().filter_map(|(&(x, y, z), _)| {
        let (cx, cz) = (x.div_euclid(16), z.div_euclid(16));
        if (cx - center_chunk.0).pow(2) + (cz - center_chunk.1).pow(2) > radius.pow(2) {
            return None;
        }
        let name = facts.get(world.block(x, y, z)).name.as_ref();
        if !TARGETS.contains(&name) {
            return None;
        }
        let kind = if name.ends_with("shulker_box") { 1 }
            else if name.ends_with(":chest") || name.ends_with(":trapped_chest")
                || name.ends_with(":barrel") || name.ends_with(":ender_chest") { 0 }
            else { 2 };
        Some(BlockEntityMarker { x, y, z, kind })
    }).collect()
}

/// Scores loaded chunks using block entities as the primary evidence. Nearby
/// amethyst clusters and artificial lights only strengthen an existing lead.
pub fn score_suspicious_chunks(
    block_entities: &[BlockEntityMarker],
    clusters: &[AmethystMarker],
    lights: &[ArtificialLightMarker],
) -> Vec<ChunkSuspicion> {
    use std::collections::HashSet;
    let mut candidates: HashSet<(i32, i32)> = HashSet::new();
    for marker in block_entities {
        candidates.insert((marker.x.div_euclid(16), marker.z.div_euclid(16)));
    }
    let mut results = Vec::new();
    for (cx, cz) in candidates {
        let in_neighborhood = |x: i32, z: i32| (x - cx).abs() <= 1 && (z - cz).abs() <= 1;
        let nearby_entities: Vec<_> = block_entities.iter().filter(|m| in_neighborhood(m.x.div_euclid(16), m.z.div_euclid(16))).collect();
        let nearby_clusters: Vec<_> = clusters.iter().filter(|m| in_neighborhood(m.x.div_euclid(16), m.z.div_euclid(16))).collect();
        let nearby_lights = lights.iter().filter(|m| in_neighborhood(m.x.div_euclid(16), m.z.div_euclid(16))).count();
        let shulkers = nearby_entities.iter().filter(|m| m.kind == 1).count();
        let storage = nearby_entities.iter().filter(|m| m.kind == 0).count();
        let utilities = nearby_entities.len() - shulkers - storage;

        // Cluster spread across multiple chunks is a stronger geode-region
        // clue than a lone cluster, but it never creates suspicion by itself.
        let cluster_chunks: HashSet<_> = nearby_clusters.iter().map(|m| (m.x.div_euclid(16), m.z.div_euclid(16))).collect();
        let mut score = (shulkers * 35).min(70)
            + (storage * 18).min(36)
            + (utilities * 8).min(16);
        if nearby_clusters.len() >= 4 { score += 8; }
        if nearby_clusters.len() >= 8 { score += 8; }
        if cluster_chunks.len() >= 2 && nearby_clusters.len() >= 4 { score += 10; }
        if nearby_lights >= 3 { score += 5; }
        if nearby_lights >= 6 { score += 5; }
        // Requiring at least one detected block entity prevents natural
        // geodes or lighting alone from flagging an entire region.
        if score >= 35 {
            // Restrict the ESP outline to the vertical span containing actual
            // evidence, with a small margin so the player can see the nearby
            // cavity without highlighting the entire world-height column.
            let evidence_y = nearby_entities.iter().map(|m| m.y)
                .chain(nearby_clusters.iter().map(|m| m.y))
                .chain(lights.iter().filter(|m| in_neighborhood(m.x.div_euclid(16), m.z.div_euclid(16))).map(|m| m.y));
            let (evidence_min, evidence_max) = evidence_y.fold((i32::MAX, i32::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)));
            let min_y = evidence_min.saturating_sub(8);
            let max_y = evidence_max.saturating_add(9);
            results.push(ChunkSuspicion {
                chunk_x: cx, chunk_z: cz, score: score.min(100) as u8,
                min_y, max_y,
                block_entities: nearby_entities.len().min(u16::MAX as usize) as u16,
                shulkers: shulkers.min(u16::MAX as usize) as u16,
                clusters: nearby_clusters.len().min(u16::MAX as usize) as u16,
                artificial_lights: nearby_lights.min(u16::MAX as usize) as u16,
            });
        }
    }
    results.sort_by(|a, b| b.score.cmp(&a.score));
    results
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
