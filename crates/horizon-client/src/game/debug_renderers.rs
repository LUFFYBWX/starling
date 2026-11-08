//! The debug options screen's renderer entries (26.3 `DebugRenderer`): the
//! water levels, heightmaps, collision boxes, light levels and sturdy faces
//! around the camera. Lines join the debug gizmos; fills and billboard texts
//! go with the entities' translucent geometry, as the test markers do.
//!
//! No-ops, for want of the data: `chunk_section_paths`,
//! `chunk_section_octree` and `chunk_section_visibility` (Horizon culls on
//! the GPU, with no section occlusion graph or octree),
//! `visualize_chunks_on_server` (the server's chunk statuses),
//! `visualize_sky_light_sections` (the light engine's section data) and
//! `visualize_entity_supporting_blocks` (`Entity.mainSupportingBlockPos`).
use super::Game;
use horizon_client::{
    block_entity_render::vertex_light,
    debug_chart::srgb_lerp,
    gizmos::{self as g, Gizmos},
    hud::HudBatch,
    hud_layout::abgr,
    mesher::Vertex,
    text::Text,
};
use horizon_core::{geometry::Vec3, position::BlockPos};
use horizon_world::{collision_context::CollisionContext, heightmap::HeightmapType};

/// `WaterDebugRenderer`'s fill: `ARGB.colorFromFloat(0.15F, 0, 1, 0)`.
const WATER_FILL: u32 = 0x2600_FF00;
/// `SolidFaceRenderer`'s fill (-2130771968).
const SOLID_FACE_FILL: u32 = 0x80FF_0000;
/// Translucent violet fill for grown amethyst clusters found by the scanner.
const AMETHYST_FILL: u32 = 0x70C060FF;
/// Translucent cyan fill for selected artificial-light blocks.
const ARTIFICIAL_LIGHT_FILL: u32 = 0x70FFB020;
/// `TextGizmo.Style.DEFAULT_SCALE`.
const TEXT_SCALE: f32 = 0.32;
/// `HeightMapRenderer.BOX_HEIGHT`.
const BOX_HEIGHT: f64 = 0.09375;

/// `HeightMapRenderer.getColor`, for the client chunk's heightmaps.
fn heightmap_color(kind: HeightmapType) -> u32 {
    match kind {
        HeightmapType::WorldSurfaceWg => 0xFFFF_FF00,
        HeightmapType::OceanFloorWg => 0xFFFF_00FF,
        HeightmapType::WorldSurface => 0xFF00_B200,
        HeightmapType::OceanFloor => 0xFF00_007F,
        HeightmapType::MotionBlocking => 0xFF00_4C4C,
        HeightmapType::MotionBlockingNoLeaves => 0xFF00_7F7F,
    }
}

/// `BlockPos.betweenClosed` of the cube `radius` around `center`.
fn cube(center: [i32; 3], radius: i32) -> impl Iterator<Item = [i32; 3]> {
    let r = -radius..=radius;
    r.clone().flat_map(move |dy| {
        let r = r.clone();
        r.clone().flat_map(move |dz| {
            r.clone()
                .map(move |dx| [center[0] + dx, center[1] + dy, center[2] + dz])
        })
    })
}

impl Game<'_> {
    /// `CollisionBoxRenderer`: the block collisions within 6 of the camera
    /// entity's box, white.
    pub(super) fn debug_renderer_gizmos(&self, g: &mut Gizmos) {
        if !self.debug_entry("visualize_collision_boxes") {
            return;
        }
        let Some(p) = &self.player else {
            return;
        };
        let bounds = p.bounds().inflate(Vec3::new(6.0, 6.0, 6.0));
        let (lo, hi) = (bounds.min_position(), bounds.max_position());
        let mut scene = horizon_client::collisions::SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        for y in lo.y.floor() as i32..=hi.y.floor() as i32 {
            for z in lo.z.floor() as i32..=hi.z.floor() as i32 {
                for x in lo.x.floor() as i32..=hi.x.floor() as i32 {
                    let Ok(shape) = self.collision.collision_shape_with_context(
                        &mut scene,
                        self.world.block(x, y, z),
                        BlockPos::new(x, y, z),
                        &CollisionContext::empty(false),
                    ) else {
                        continue;
                    };
                    let at = [f64::from(x), f64::from(y), f64::from(z)];
                    for b in shape.boxes() {
                        let (min, max) = (b.min_position(), b.max_position());
                        g.cuboid(
                            [at[0] + min.x, at[1] + min.y, at[2] + min.z],
                            [at[0] + max.x, at[1] + max.y, at[2] + max.z],
                            g::WHITE,
                            g::STROKE_WIDTH,
                        );
                    }
                }
            }
        }
    }

    /// The fill and text renderers' geometry for this frame.
    pub(super) fn debug_renderer_geometry(&mut self, camera: glam::DVec3, out: &mut Vec<Vertex>, see_through: &mut Vec<Vertex>) {
        let water = self.debug_entry("visualize_water_levels");
        let heightmap = self.debug_entry("visualize_heightmap");
        let block_light = self.debug_entry("visualize_block_light_levels");
        let sky_light = self.debug_entry("visualize_sky_light_levels");
        let solid = self.debug_entry("visualize_solid_faces");
        let scanner = &self.settings.extras;
        let amethyst_markers = if scanner.scanner_enabled && scanner.scanner_amethyst {
            self.extras.amethyst_markers.clone()
        } else {
            Vec::new()
        };
        let artificial_light_markers =
            if scanner.scanner_enabled && scanner.scanner_artificial_lights {
                self.extras.artificial_light_markers.clone()
            } else {
                Vec::new()
            };
        let honey_block_markers = if scanner.scanner_enabled {
            self.extras.honey_block_markers.clone()
        } else {
            Vec::new()
        };
        let suspicious_chunks = if scanner.scanner_enabled && scanner.scanner_chunks {
            self.extras.suspicious_chunks.clone()
        } else {
            Vec::new()
        };
        let block_entity_markers =
            if scanner.scanner_enabled && scanner.scanner_block_entities {
                self.extras.block_entity_markers.clone()
            } else {
                Vec::new()
            };
        let Some(p) = &self.player else {
            return;
        };
        if !(water || heightmap || block_light || sky_light || solid || !amethyst_markers.is_empty() || !artificial_light_markers.is_empty() || !block_entity_markers.is_empty() || !honey_block_markers.is_empty() || !suspicious_chunks.is_empty()) {
            return;
        }
        let feet = [p.position.x, p.position.y, p.position.z].map(|v| v.floor() as i32);
        let eye = camera.to_array().map(|v| v.floor() as i32);
        let mut fills: Vec<([[f64; 3]; 4], u32)> = Vec::new();
        let mut esp_fills: Vec<([[f64; 3]; 4], u32)> = Vec::new();
        let mut texts: Vec<([f64; 3], String, u32)> = Vec::new();
        // One thin strip per side: a clean 16x16 outline at the surface,
        // with no filled blocks inside the chunk.
        for chunk in &suspicious_chunks {
            if !self.world.chunks.contains_key(&(chunk.chunk_x, chunk.chunk_z)) {
                continue;
            }
            let bx = chunk.chunk_x * 16;
            let bz = chunk.chunk_z * 16;
            let x = f64::from(bx);
            let z = f64::from(bz);
            // Keep all four chunk edges on one horizontal plane. Sampling
            // each edge independently made borders follow terrain height changes.
            let border_y = f64::from(self.column_height(
                HeightmapType::WorldSurface, bx + 8, bz + 8,
            )) + 0.035;
            let (y_north, y_south, y_west, y_east) = (border_y, border_y, border_y, border_y);
            let color = 0xF0B040FF;
            // The displayed percentage is the scanner's existing 0..100
            // evidence score, computed from the same filter criteria.
            let label_y = f64::from(self.column_height(
                HeightmapType::WorldSurface,
                bx + 8,
                bz + 8,
            )) + 1.25;
            texts.push((
                [x + 8.0, label_y, z + 8.0],
                format!("BASE? {}%", chunk.score),
                0xFFE0A0FF,
            ));
            for (min, max) in [
                ([x, y_north, z], [x + 16.0, y_north + 0.12, z + 0.18]),
                ([x + 0.0, y_south, z + 15.82], [x + 16.0, y_south + 0.12, z + 16.0]),
                ([x, y_west, z], [x + 0.18, y_west + 0.12, z + 16.0]),
                ([x + 15.82, y_east, z], [x + 16.0, y_east + 0.12, z + 16.0]),
            ] {
                esp_fills.extend(g::cuboid_fill_quads(min, max).map(|q| (q, color)));
            }
        }
        if water {
            self.water_level_shapes(feet, &mut fills, &mut texts);
        }
        if heightmap {
            self.heightmap_shapes(eye, &mut fills);
        }
        if block_light || sky_light {
            light_level_texts(self, eye, block_light, sky_light, &mut texts);
        }
        if solid {
            self.solid_face_shapes(eye, &mut fills);
        }
        // Honey Block ESP: red translucent boxes, visible through terrain.
        for marker in honey_block_markers {
            let dx = f64::from(marker.x) + 0.5 - camera.x;
            let dy = f64::from(marker.y) + 0.5 - camera.y;
            let dz = f64::from(marker.z) + 0.5 - camera.z;
            if dx * dx + dy * dy + dz * dz > 64.0 * 64.0 {
                continue;
            }
            let x = f64::from(marker.x);
            let y = f64::from(marker.y);
            let z = f64::from(marker.z);
            esp_fills.extend(
                g::cuboid_fill_quads(
                    [x + 0.04, y + 0.04, z + 0.04],
                    [x + 0.96, y + 0.96, z + 0.96],
                )
                .map(|q| (q, 0xA0FF2020)),
            );
        }
        // Highlight detected grown clusters near the camera without requiring
        // the debug-renderer options screen to be enabled.
        for marker in amethyst_markers {
            let dx = f64::from(marker.x) + 0.5 - camera.x;
            let dy = f64::from(marker.y) + 0.5 - camera.y;
            let dz = f64::from(marker.z) + 0.5 - camera.z;
            if dx * dx + dy * dy + dz * dz > 48.0 * 48.0 {
                continue;
            }
            let x = f64::from(marker.x);
            let y = f64::from(marker.y);
            let z = f64::from(marker.z);
            esp_fills.extend(
                g::cuboid_fill_quads(
                    [x + 0.08, y + 0.08, z + 0.08],
                    [x + 0.92, y + 0.92, z + 0.92],
                )
                .map(|q| (q, AMETHYST_FILL)),
            );
        }
        // Block-entity ESP: containers and utility block entities are
        // highlighted through walls, with shulkers in a distinct color.
        for marker in block_entity_markers {
            // Trial Spawners and Vaults are structure-exclusion signals only.
            // Do not render them as player storage/utility ESP.
            if marker.kind == 3 {
                continue;
            }
            let dx = f64::from(marker.x) + 0.5 - camera.x;
            let dy = f64::from(marker.y) + 0.5 - camera.y;
            let dz = f64::from(marker.z) + 0.5 - camera.z;
            if dx * dx + dy * dy + dz * dz > 64.0 * 64.0 {
                continue;
            }
            let x = f64::from(marker.x);
            let y = f64::from(marker.y);
            let z = f64::from(marker.z);
            let color = match marker.kind {
                1 => 0x80D060FF, // shulker
                0 => 0x8060D0FF, // chest / barrel
                _ => 0x80FFD060, // furnace, hopper and other utility
            };
            esp_fills.extend(
                g::cuboid_fill_quads(
                    [x + 0.04, y + 0.04, z + 0.04],
                    [x + 0.96, y + 0.96, z + 0.96],
                )
                .map(|q| (q, color)),
            );
        }
        // Highlight selected artificial-light blocks with the same through-wall
        // translucent geometry used for amethyst clusters.
        for marker in artificial_light_markers {
            let dx = f64::from(marker.x) + 0.5 - camera.x;
            let dy = f64::from(marker.y) + 0.5 - camera.y;
            let dz = f64::from(marker.z) + 0.5 - camera.z;
            if dx * dx + dy * dy + dz * dz > 48.0 * 48.0 {
                continue;
            }
            let x = f64::from(marker.x);
            let y = f64::from(marker.y);
            let z = f64::from(marker.z);
            let (inset, color) = if marker.is_source {
                (0.08, ARTIFICIAL_LIGHT_FILL)
            } else {
                (0.18, 0x48FFB020)
            };
            esp_fills.extend(
                g::cuboid_fill_quads(
                    [x + inset, y + inset, z + inset],
                    [x + 1.0 - inset, y + 1.0 - inset, z + 1.0 - inset],
                )
                .map(|q| (q, color)),
            );
        }
        let light = vertex_light(0xFF);
        let mut white = HudBatch::new([1, 1]);
        let start = out.len();
        self.mark_translucent_entity(start, "");
        for (quad, color) in fills {
            white.vertices.clear();
            white.fill(&self.gui.atlas, 0.0, 0.0, 1.0, 1.0, abgr(color));
            let Some(v) = white.vertices.first().copied() else { continue };
            for corner in quad {
                out.push(Vertex {
                    normal: [0; 4],
                    pos: (glam::DVec3::from_array(corner) - camera).as_vec3().to_array(),
                    uv: v.uv, layer: 0, color: v.color, light, glint_uv: [0.0, 0.0],
                });
            }
        }
        // ESP surfaces and container boxes use the always-on-top vertex stream;
        // ordinary debug overlays above retain normal world depth testing.
        for (quad, color) in esp_fills {
            white.vertices.clear();
            white.fill(&self.gui.atlas, 0.0, 0.0, 1.0, 1.0, abgr(color));
            let Some(v) = white.vertices.first().copied() else { continue };
            for corner in quad {
                see_through.push(Vertex {
                    normal: [0; 4],
                    pos: (glam::DVec3::from_array(corner) - camera).as_vec3().to_array(),
                    uv: v.uv, layer: 0, color: v.color, light, glint_uv: [0.0, 0.0],
                });
            }
        }
        // `Gizmos.billboardText`: centred, without a background.
        let mut patches = Vec::new();
        for (at, text, color) in texts {
            let scale = if text.starts_with("BASE? ") {
                0.56
            } else {
                TEXT_SCALE
            };
            patches.extend(self.name_tag_quads(
                glam::DVec3::from_array(at) - camera,
                &[(Text::literal(text).with_color(color & 0xFF_FFFF), 0.0)],
                color >> 24,
                false,
                2,
                0,
                g::billboard_text_unit(scale),
                out,
            ));
        }
        self.map_patches.extend(patches);
    }

    /// `WaterDebugRenderer`: each water block within 10 of the feet filled
    /// to its height, with its amount over it.
    fn water_level_shapes(
        &self,
        feet: [i32; 3],
        fills: &mut Vec<([[f64; 3]; 4], u32)>,
        texts: &mut Vec<([f64; 3], String, u32)>,
    ) {
        use horizon_client::blocks::Fluid;
        for pos in cube(feet, 10) {
            let f = self
                .res
                .facts()
                .get(self.world.block(pos[0], pos[1], pos[2]));
            if f.fluid != Fluid::Water {
                continue;
            }
            let Some((height, _)) = self.fluid_height_of(pos[0], pos[1], pos[2], None) else {
                continue;
            };
            let [x, y, z] = pos.map(f64::from);
            let top = y + f64::from(height);
            let (lo, hi) = (f64::from(0.01f32), f64::from(0.99f32));
            fills.extend(
                g::cuboid_fill_quads([x + lo, y + lo, z + lo], [x + hi, top, z + hi])
                    .map(|q| (q, WATER_FILL)),
            );
            // FluidState.getAmount: a source holds 8, a flowing one its level.
            texts.push((
                [x + 0.5, top, z + 0.5],
                f.fluid_level.to_string(),
                0xFF00_0000,
            ));
        }
    }

    /// `HeightMapRenderer`: the client chunk's heightmaps of the 5x5 chunks
    /// around the camera, a small box per column, stacked by type.
    fn heightmap_shapes(&self, eye: [i32; 3], fills: &mut Vec<([[f64; 3]; 4], u32)>) {
        let (cx, cz) = (eye[0] >> 4, eye[2] >> 4);
        for chunk_x in cx - 2..=cx + 2 {
            for chunk_z in cz - 2..=cz + 2 {
                if !self.world.chunks.contains_key(&(chunk_x, chunk_z)) {
                    continue;
                }
                for kind in HeightmapType::ALL
                    .into_iter()
                    .filter(|k| k.send_to_client())
                {
                    let color = heightmap_color(kind);
                    for x in chunk_x * 16..chunk_x * 16 + 16 {
                        for z in chunk_z * 16..chunk_z * 16 + 16 {
                            let height = f64::from(self.column_height(kind, x, z))
                                + f64::from(kind as u8) * BOX_HEIGHT;
                            let (x, z) = (f64::from(x), f64::from(z));
                            fills.extend(
                                g::cuboid_fill_quads(
                                    [x + 0.25, height, z + 0.25],
                                    [x + 0.75, height + BOX_HEIGHT, z + 0.75],
                                )
                                .map(|q| (q, color)),
                            );
                        }
                    }
                }
            }
        }
    }

    /// `SolidFaceRenderer`: each outline box of the blocks within 6 of the
    /// camera, grown by 0.002, with its sides that are sturdy filled.
    fn solid_face_shapes(&self, eye: [i32; 3], fills: &mut Vec<([[f64; 3]; 4], u32)>) {
        // In SolidFaceRenderer's order (WEST, SOUTH, EAST, NORTH, DOWN, UP):
        // the Direction ordinal and the side's quad in `cuboid_fill_quads`.
        const SIDES: [(u8, usize); 6] = [(4, 1), (3, 3), (5, 0), (2, 2), (0, 5), (1, 4)];
        for pos in cube(eye, 6) {
            let state = self.world.block(pos[0], pos[1], pos[2]);
            let f = self.res.facts().get(state);
            if &*f.name == "minecraft:air" || f.sturdy_full == 0 {
                continue;
            }
            let at = pos.map(f64::from);
            for b in f.outline.iter() {
                let min = [at[0] + b[0], at[1] + b[1], at[2] + b[2]].map(|v| v - 0.002);
                let max = [at[0] + b[3], at[1] + b[4], at[2] + b[5]].map(|v| v + 0.002);
                let quads = g::cuboid_fill_quads(min, max);
                for (direction, side) in SIDES {
                    if f.sturdy_full & (1 << direction) != 0 {
                        fills.push((quads[side], SOLID_FACE_FILL));
                    }
                }
            }
        }
    }
}

/// `LightDebugRenderer`: within 10 of the camera, the sky light where it
/// isn't full and the block light where there is some (the light engine's
/// section data, drawn with them in vanilla, Horizon doesn't keep).
fn light_level_texts(
    game: &Game<'_>,
    eye: [i32; 3],
    block: bool,
    sky: bool,
    texts: &mut Vec<([f64; 3], String, u32)>,
) {
    for pos in cube(eye, 10) {
        let light = game.world.light(pos[0], pos[1], pos[2]);
        let (sky_light, block_light) = (light >> 4, light & 15);
        let [x, y, z] = pos.map(f64::from);
        if sky_light != 15 && sky {
            let color = srgb_lerp(f32::from(sky_light) / 15.0, 0xFF00_00FF, 0xFF00_FFFF);
            texts.push(([x + 0.5, y + 0.25, z + 0.5], sky_light.to_string(), color));
        }
        if block && block_light != 0 {
            let color = srgb_lerp(f32::from(block_light) / 15.0, 0xFFAA_0000, 0xFFFF_FF00);
            texts.push(([x + 0.5, y + 0.5, z + 0.5], block_light.to_string(), color));
        }
    }
}
