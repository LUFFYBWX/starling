//! Finds budding-amethyst blocks in resident client chunks.
//!
//! The caller resolves the block-state ID from the active registry. This keeps
//! the scanner independent of vanilla IDs and content-pack registry ordering.
use crate::chunk::Chunk;

/// A budding-amethyst block in absolute world coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AmethystMarker {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Scan one already-loaded chunk for a specific block-state ID.
///
/// Uniform sections are handled without unpacking their palette. Non-uniform
/// sections are unpacked once and scanned in the chunk's native index order.
pub fn scan_chunk(chunk: &Chunk, target_state: u32) -> Vec<AmethystMarker> {
    let mut found = Vec::new();
    let base_x = chunk.x * 16;
    let base_z = chunk.z * 16;

    for (section_index, section) in chunk.sections.iter().enumerate() {
        let base_y = chunk.min_y() + section_index as i32 * 16;

        if section.is_uniform(target_state) {
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
        if section.is_uniform(0) {
            continue;
        }

        let mut states = [0u32; 4096];
        section.unpack(&mut states);
        for (index, state) in states.into_iter().enumerate() {
            if state != target_state {
                continue;
            }
            let local_x = (index & 15) as i32;
            let local_z = ((index >> 4) & 15) as i32;
            let local_y = (index >> 8) as i32;
            found.push(AmethystMarker {
                x: base_x + local_x,
                y: base_y + local_y,
                z: base_z + local_z,
            });
        }
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{Chunk, ColumnLayout};

    #[test]
    fn finds_targets_at_absolute_coordinates_across_sections() {
        let mut chunk = Chunk::with_layout( -2, 3, ColumnLayout::new(-64, 384).unwrap());
        chunk.set(4, -32, 7, 42);
        chunk.set(5, -31, 7, 99);
        chunk.set(15, 319, 0, 42);

        let found = scan_chunk(&chunk, 42);
        assert_eq!(
            found,
            vec![
                AmethystMarker { x: -28, y: -32, z: 55 },
                AmethystMarker { x: -17, y: 319, z: 48 },
            ]
        );
    }

    #[test]
    fn uniform_target_section_is_reported_completely() {
        let mut chunk = Chunk::empty(1, -1);
        for y in -64..-48 {
            for z in 0..16 {
                for x in 0..16 {
                    chunk.set(x, y, z, 77);
                }
            }
        }

        let found = scan_chunk(&chunk, 77);
        assert_eq!(found.len(), 4096);
        assert!(found.contains(&AmethystMarker { x: 16, y: -64, z: -16 }));
        assert!(found.contains(&AmethystMarker { x: 31, y: -49, z: -1 }));
    }

    #[test]
    fn empty_chunk_has_no_markers() {
        assert!(scan_chunk(&Chunk::empty(0, 0), 42).is_empty());
    }
}
