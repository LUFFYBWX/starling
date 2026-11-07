//! Finds grown amethyst clusters and selected artificial-light blocks in resident chunks.
//!
//! Detection is based on block-state registry names, never on light level alone.
//! This avoids treating lava, fire, or other natural light sources as player clues.
use crate::chunk::Chunk;
use std::collections::{HashMap, HashSet, VecDeque};

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
    /// True for the torch/lantern that emitted the light; false for an illuminated block.
    pub is_source: bool,
}

/// A loaded container or utility block entity highlighted by the ESP.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockEntityMarker {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// 0 = chest/barrel, 1 = shulker, 2 = other container or utility, 3 = generated structure exclusion.
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

/// Scan loaded chunks for any unfinished amethyst bud. A geode containing one
/// of these is excluded from suspicion scoring, even if it also has mature
/// clusters: the requested signal is a fully-grown geode.
pub fn scan_loaded_small_buds_near(
    world: &crate::world::World,
    facts: &crate::blocks::BlockFacts,
    center_chunk: (i32, i32),
    radius_chunks: i32,
) -> Vec<AmethystMarker> {
    let states = state_ids_named(
        facts,
        &[
            "minecraft:small_amethyst_bud",
            "minecraft:medium_amethyst_bud",
            "minecraft:large_amethyst_bud",
        ],
    );
    scan_loaded_states_near(world, &states, facts.air.0, center_chunk, radius_chunks)
}

/// Scan loaded chunks for blocks illuminated by selected artificial light sources.
/// Natural emitters (including lava) are never used as propagation origins.
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
    ];
    const DIRECTIONS: [(i32, i32, i32); 6] = [
        (1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1),
    ];
    let source_states = state_ids_named(facts, LIGHT_BLOCKS);
    if source_states.is_empty() {
        return Vec::new();
    }
    let sources = scan_loaded_states_near(
        world, &source_states, facts.air.0, center_chunk, radius_chunks,
    );
    let mut illuminated = HashSet::new();
    let mut source_markers = HashSet::new();
    let mut best: HashMap<(i32, i32, i32), u8> = HashMap::new();
    let mut queue = VecDeque::new();
    let min_chunk = (center_chunk.0 - radius_chunks.max(0), center_chunk.1 - radius_chunks.max(0));
    let max_chunk = (center_chunk.0 + radius_chunks.max(0), center_chunk.1 + radius_chunks.max(0));

    // Seed one shared flood fill with every selected torch/lantern. This avoids
    // repeating a 15-block light search independently for every source.
    for source in sources {
        let pos = (source.x, source.y, source.z);
        let in_generated_structure = world.chunks
            .get(&(source.x.div_euclid(16), source.z.div_euclid(16)))
            .is_some_and(|chunk| !chunk.structure_refs.is_empty() || !chunk.structure_starts.is_empty());
        if in_generated_structure {
            continue;
        }
        source_markers.insert(pos);
        let level = facts.get(world.block(source.x, source.y, source.z)).emission.min(15);
        if level > best.get(&pos).copied().unwrap_or(0) {
            best.insert(pos, level);
            queue.push_back((pos, level));
        }
    }
    while let Some(((x, y, z), level)) = queue.pop_front() {
        if best.get(&(x, y, z)).copied() != Some(level) || level <= 1 {
            continue;
        }
        for (dx, dy, dz) in DIRECTIONS {
            let pos = (x + dx, y + dy, z + dz);
            let (cx, cz) = (pos.0.div_euclid(16), pos.2.div_euclid(16));
            if cx < min_chunk.0 || cx > max_chunk.0 || cz < min_chunk.1 || cz > max_chunk.1
                || pos.1 < world.bottom_y() || pos.1 >= world.top_y()
                || !world.loaded(pos.0, pos.2)
            {
                continue;
            }
            let state = world.block(pos.0, pos.1, pos.2);
            let cell = facts.get(state);
            // Natural emitters (lava, glowstone, etc.) never seed or extend
            // this scan. Selected torches/lanterns remain traversable.
            if cell.emission > 0 && !source_states.contains(&state.0) {
                continue;
            }
            let next = level.saturating_sub(cell.dampening.max(1));
            if next == 0 || best.get(&pos).is_some_and(|old| *old >= next) {
                continue;
            }
            best.insert(pos, next);
            queue.push_back((pos, next));
            if !source_states.contains(&state.0) {
                illuminated.insert(pos);
            }
        }
    }
    let mut markers: Vec<_> = illuminated.into_iter()
        .map(|(x, y, z)| ArtificialLightMarker { x, y, z, is_source: false })
        .collect();
    markers.extend(source_markers.into_iter().map(|(x, y, z)| ArtificialLightMarker { x, y, z, is_source: true }));
    markers.sort_by_key(|m| (m.x, m.y, m.z));
    markers
}

/// Whether a container belongs to generated loot or a known generated structure.
/// Horizon preserves unrolled loot tables and rolled results after opening. Chunk
/// structure references also cover generated chests that have no saved loot tag.
fn has_generated_loot_table(chunk: &Chunk, position: (i32, i32, i32)) -> bool {
    let saved_loot = chunk.saved.as_ref().is_some_and(|saved| {
        saved.block_entity_sources.iter().any(|(pos, tag)| {
            *pos == position && tag.get("LootTable").is_some()
        }) || saved.rolled_loot.iter().any(|loot| loot.pos == position)
    });
    saved_loot || !chunk.structure_refs.is_empty() || !chunk.structure_starts.is_empty()
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
        // Trial Chambers are structures, not player bases. Keep their signature
        // block entities as exclusion markers so nearby loot/mechanisms do not
        // inflate base suspicion.
        "minecraft:trial_spawner", "minecraft:vault", "minecraft:mob_spawner",
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
        // Naturally generated chests often carry a structure loot table. Skip
        // both unopened tables and their tracked rolled form after first opening.
        if (name == "minecraft:chest" || name == "minecraft:trapped_chest" || name == "minecraft:barrel")
            && world.chunks.get(&(cx, cz)).is_some_and(|chunk| {
                has_generated_loot_table(chunk, (x, y, z))
            })
        {
            return None;
        }
        let kind = if name == "minecraft:trial_spawner" || name == "minecraft:vault" || name == "minecraft:mob_spawner" { 3 }
            else if name.ends_with("shulker_box") { 1 }
            else if name.ends_with(":chest") || name.ends_with(":trapped_chest")
                || name.ends_with(":barrel") || name.ends_with(":ender_chest") { 0 }
            else { 2 };
        Some(BlockEntityMarker { x, y, z, kind })
    }).collect()
}

/// Groups mature clusters that belong to the same compact geode area.
/// Vanilla geodes are small; a 12-block neighborhood keeps nearby geodes apart.
fn grown_geode_groups(clusters: &[AmethystMarker]) -> Vec<Vec<AmethystMarker>> {
    let mut remaining = clusters.to_vec();
    let mut groups = Vec::new();
    while !remaining.is_empty() {
        let mut group = vec![remaining.swap_remove(0)];
        let mut changed = true;
        while changed {
            changed = false;
            let mut index = 0;
            while index < remaining.len() {
                let joins_group = group.iter().any(|grown| {
                    let dx = i64::from(grown.x) - i64::from(remaining[index].x);
                    let dy = i64::from(grown.y) - i64::from(remaining[index].y);
                    let dz = i64::from(grown.z) - i64::from(remaining[index].z);
                    dx * dx + dy * dy + dz * dz <= 12 * 12
                });
                if joins_group {
                    group.push(remaining.swap_remove(index));
                    changed = true;
                } else {
                    index += 1;
                }
            }
        }
        groups.push(group);
    }
    groups
}

/// Scores chunks around fully-grown geodes. Mature clusters are the primary
/// signal; nearby storage, utility blocks, and artificial lights reinforce it.
/// Any geode group containing a small/medium/large bud is rejected entirely.
pub fn score_suspicious_chunks(
    block_entities: &[BlockEntityMarker],
    clusters: &[AmethystMarker],
    small_buds: &[AmethystMarker],
    lights: &[ArtificialLightMarker],
    minimum_score: u8,
) -> Vec<ChunkSuspicion> {
    use std::collections::HashSet;

    let trial_markers: Vec<_> = block_entities.iter().filter(|m| m.kind == 3).collect();
    let groups = grown_geode_groups(clusters);
    let mut results = Vec::new();

    for group in groups {
        // One unfinished crystal anywhere in this geode invalidates the group.
        let has_small_bud = group.iter().any(|grown| {
            small_buds.iter().any(|bud| {
                let dx = i64::from(grown.x) - i64::from(bud.x);
                let dy = i64::from(grown.y) - i64::from(bud.y);
                let dz = i64::from(grown.z) - i64::from(bud.z);
                dx * dx + dy * dy + dz * dz <= 12 * 12
            })
        });
        if has_small_bud {
            continue;
        }

        let candidate_chunks: HashSet<_> = group
            .iter()
            .map(|m| (m.x.div_euclid(16), m.z.div_euclid(16)))
            .collect();

        for (cx, cz) in candidate_chunks {
            // Known generated-structure spawners/vaults exclude nearby chunks.
            let near_trial_chamber = trial_markers.iter().any(|m| {
                (m.x.div_euclid(16) - cx).abs() <= 3
                    && (m.z.div_euclid(16) - cz).abs() <= 3
            });
            if near_trial_chamber {
                continue;
            }

            let in_neighborhood = |x: i32, z: i32| {
                (x - cx).abs() <= 1 && (z - cz).abs() <= 1
            };
            let nearby_clusters: Vec<_> = group
                .iter()
                .filter(|m| in_neighborhood(m.x.div_euclid(16), m.z.div_euclid(16)))
                .collect();
            // One or two mature clusters are not enough to identify a base.
            if nearby_clusters.len() < 4 {
                continue;
            }

            let nearby_entities: Vec<_> = block_entities
                .iter()
                .filter(|m| m.kind != 3 && in_neighborhood(m.x.div_euclid(16), m.z.div_euclid(16)))
                .collect();
            let nearby_lights: Vec<_> = lights
                .iter()
                .filter(|m| m.is_source && in_neighborhood(m.x.div_euclid(16), m.z.div_euclid(16)))
                .collect();

            let shulkers = nearby_entities.iter().filter(|m| m.kind == 1).count();
            let storage = nearby_entities.iter().filter(|m| m.kind == 0).count();
            let utilities = nearby_entities.len() - shulkers - storage;
            let cluster_chunks: HashSet<_> = nearby_clusters
                .iter()
                .map(|m| (m.x.div_euclid(16), m.z.div_euclid(16)))
                .collect();

            // Mature geodes carry the largest share of the score. Other signs
            // add confidence but cannot turn an isolated chest into a lead.
            let mut score = (nearby_clusters.len() * 8).min(64)
                + (shulkers * 40).min(80)
                + (storage * 12).min(36)
                + (utilities * 5).min(15);
            if cluster_chunks.len() >= 2 && nearby_clusters.len() >= 4 {
                score += 10;
            }
            if nearby_lights.len() >= 3 {
                score += 5;
            }
            if nearby_lights.len() >= 6 {
                score += 5;
            }

            let has_base_like_storage = shulkers >= 2
                || (shulkers >= 1 && storage >= 2)
                || (storage >= 4 && utilities >= 2);
            // Strong artificial lighting can reinforce a large mature geode
            // even when the area has few storage block entities.
            let has_light_supported_geode = nearby_clusters.len() >= 8 && nearby_lights.len() >= 3;

            if (has_base_like_storage || has_light_supported_geode) && score >= usize::from(minimum_score) {
                let evidence_y = nearby_entities
                    .iter()
                    .map(|m| m.y)
                    .chain(nearby_clusters.iter().map(|m| m.y))
                    .chain(nearby_lights.iter().map(|m| m.y));
                let (evidence_min, evidence_max) = evidence_y.fold(
                    (i32::MAX, i32::MIN),
                    |(lo, hi), y| (lo.min(y), hi.max(y)),
                );
                results.push(ChunkSuspicion {
                    chunk_x: cx,
                    chunk_z: cz,
                    score: score.min(100) as u8,
                    min_y: evidence_min.saturating_sub(8),
                    max_y: evidence_max.saturating_add(9),
                    block_entities: nearby_entities.len().min(u16::MAX as usize) as u16,
                    shulkers: shulkers.min(u16::MAX as usize) as u16,
                    clusters: nearby_clusters.len().min(u16::MAX as usize) as u16,
                    artificial_lights: nearby_lights.len().min(u16::MAX as usize) as u16,
                });
            }
        }
    }

    results.sort_by(|a, b| b.score.cmp(&a.score));
    results.dedup_by_key(|chunk| (chunk.chunk_x, chunk.chunk_z));
    results
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{Chunk, ColumnLayout};

    #[test]
    fn generated_loot_chest_is_recognized_after_loot_was_rolled() {
        use crate::chunk::{ChunkExtras, RolledLoot};
        use horizon_world::nbt::Tag;
        use std::sync::Arc;

        let position = (8, 64, 8);
        let mut chunk = Chunk::empty(0, 0);
        let extras = ChunkExtras {
            rolled_loot: vec![RolledLoot {
                pos: position,
                table: "minecraft:chests/simple_dungeon".into(),
                seed: None,
                items: Tag::List(Vec::new()),
            }],
            ..Default::default()
        };
        chunk.saved = Some(Arc::new(extras));
        assert!(has_generated_loot_table(&chunk, position));
        assert!(!has_generated_loot_table(&chunk, (9, 64, 8)));

        // Freshly generated structures may have no saved chunk extras yet.
        let mut structure_chunk = Chunk::empty(0, 0);
        structure_chunk.structure_refs.push((Arc::from("minecraft:mineshaft"), (0, 0)));
        assert!(has_generated_loot_table(&structure_chunk, position));
        assert!(!has_generated_loot_table(&Chunk::empty(0, 0), position));
    }

    #[test]
    fn common_generated_storage_alone_does_not_flag_a_chunk() {
        let chest = BlockEntityMarker { x: 8, y: 64, z: 8, kind: 0 };
        assert!(score_suspicious_chunks(&[chest], &[], &[], &[], 45).is_empty());
        let four_chests = [
            chest,
            BlockEntityMarker { x: 9, y: 64, z: 8, kind: 0 },
            BlockEntityMarker { x: 10, y: 64, z: 8, kind: 0 },
            BlockEntityMarker { x: 11, y: 64, z: 8, kind: 0 },
        ];
        assert!(score_suspicious_chunks(&four_chests, &[], &[], &[], 45).is_empty());
    }

    #[test]
    fn one_shulker_without_other_base_evidence_does_not_flag() {
        let shulker = BlockEntityMarker { x: 8, y: 64, z: 8, kind: 1 };
        assert!(score_suspicious_chunks(&[shulker], &[], &[], &[], 45).is_empty());
    }

    #[test]
    fn mixed_storage_and_utilities_can_flag_a_chunk() {
        let entities = [
            BlockEntityMarker { x: 8, y: 64, z: 8, kind: 0 },
            BlockEntityMarker { x: 9, y: 64, z: 8, kind: 0 },
            BlockEntityMarker { x: 10, y: 64, z: 8, kind: 0 },
            BlockEntityMarker { x: 11, y: 64, z: 8, kind: 0 },
            BlockEntityMarker { x: 12, y: 64, z: 8, kind: 2 },
            BlockEntityMarker { x: 13, y: 64, z: 8, kind: 2 },
        ];
        let clusters = [
            AmethystMarker { x: 4, y: 20, z: 8 },
            AmethystMarker { x: 5, y: 20, z: 8 },
            AmethystMarker { x: 6, y: 20, z: 8 },
            AmethystMarker { x: 7, y: 20, z: 8 },
        ];
        let scored = score_suspicious_chunks(&entities, &clusters, &[], &[], 45);
        assert_eq!(scored.len(), 1);
        assert!(scored[0].score >= 45);
    }


    #[test]
    fn one_or_two_mature_clusters_never_flag_a_chunk() {
        let entities = [
            BlockEntityMarker { x: 8, y: 64, z: 8, kind: 1 },
            BlockEntityMarker { x: 9, y: 64, z: 8, kind: 1 },
        ];
        let clusters = [
            AmethystMarker { x: 8, y: 20, z: 8 },
            AmethystMarker { x: 9, y: 20, z: 8 },
        ];
        assert!(score_suspicious_chunks(&entities, &clusters, &[], &[], 45).is_empty());
    }

    #[test]
    fn any_unfinished_bud_rejects_its_geode_group() {
        let entities = [
            BlockEntityMarker { x: 8, y: 20, z: 8, kind: 1 },
            BlockEntityMarker { x: 9, y: 20, z: 8, kind: 1 },
        ];
        let clusters = [
            AmethystMarker { x: 8, y: 20, z: 8 },
            AmethystMarker { x: 9, y: 20, z: 8 },
            AmethystMarker { x: 10, y: 20, z: 8 },
            AmethystMarker { x: 11, y: 20, z: 8 },
            AmethystMarker { x: 12, y: 20, z: 8 },
        ];
        let small_buds = [AmethystMarker { x: 13, y: 20, z: 8 }];
        assert!(score_suspicious_chunks(&entities, &clusters, &small_buds, &[], 45).is_empty());
    }

    #[test]
    fn mature_geode_and_nearby_storage_can_flag_a_chunk() {
        let entities = [
            BlockEntityMarker { x: 8, y: 20, z: 8, kind: 1 },
            BlockEntityMarker { x: 9, y: 20, z: 8, kind: 1 },
        ];
        let clusters = [
            AmethystMarker { x: 8, y: 20, z: 8 },
            AmethystMarker { x: 9, y: 20, z: 8 },
            AmethystMarker { x: 10, y: 20, z: 8 },
            AmethystMarker { x: 11, y: 20, z: 8 },
            AmethystMarker { x: 12, y: 20, z: 8 },
        ];
        let scored = score_suspicious_chunks(&entities, &clusters, &[], &[], 45);
        assert!(!scored.is_empty());
        assert!(scored[0].clusters >= 4);
    }

    #[test]
    fn a_large_mature_geode_with_artificial_lighting_can_flag_a_chunk() {
        let clusters: Vec<_> = (0..8)
            .map(|x| AmethystMarker { x, y: 20, z: 8 })
            .collect();
        let lights: Vec<_> = (0..3)
            .map(|x| ArtificialLightMarker { x, y: 20, z: 10, is_source: true })
            .collect();
        let scored = score_suspicious_chunks(&[], &clusters, &[], &lights, 45);
        assert!(!scored.is_empty());
    }

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
