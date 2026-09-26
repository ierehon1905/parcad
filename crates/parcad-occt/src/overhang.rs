//! Overhang, per body, in the orientation it prints: which faces face down
//! at or under the threshold angle, how much of the body is unsupported,
//! which ceilings are bridges, and what a support prism to the bed would
//! hold. The open half of printability in docs/PERCEPTION.md §6, asked for
//! by docs/COIN_HOLDER_REVIEW.md (mechanical §2.1, workflow §3).
//!
//! Measured on the mesh the kernel just laid on the exact solid, face by
//! face: a plane's angle is one number and is exact; anything curved is
//! sampled on its triangles, to the mesher's deflection, and the reply says
//! so. Report the geometry, never a verdict about a printer.

use crate::perceive::Body;
use crate::protocol::{Bridge, FaceSummary, Overhang, OverhangFace};
use glam::DVec3;
use opencascade::mesh::Mesh;
use opencascade::adhoc::AdHocShape;
use opencascade::primitives::{BooleanShape, Crossing, Edge, Face, PointState, Shape};
use parcad_core::planar_supports;

/// Faces at or under this angle to the bed need support or a bridge: the
/// slicers' usual default. Reported, never judged: a face exactly here is
/// listed, not passed.
pub const THRESHOLD_DEG: f64 = 45.0;
/// A face is a ceiling below this angle: a candidate bridge.
const CEILING_DEG: f64 = 1.0;
/// How near the lowest point a vertex may sit and still be on the bed.
const BED_TOLERANCE_MM: f64 = 0.01;
/// How far past a ceiling's edge, and how far below it, a wall must reach to
/// hold that edge up.
const PROBE_MM: f64 = 0.3;
/// How far a support's sampled points may stray from its edge: the mesher's
/// own deflection, so a span is as good as every other number here.
const SUPPORT_DEFLECTION_MM: f64 = 0.01;
/// How many overhanging faces the reply lists; the rest are counted.
const LISTED: usize = 8;
/// Past this many overhanging faces the support prism is not built: one
/// prism, fuse and cut per face is a boolean the size of the part. Nor is it
/// built when a curved face overhangs: a prism under a NURBS flank is a
/// NURBS-bounded solid, and forty of them fused took a thread past its
/// budget (the corpus's M8 pairs). Report nothing rather than something.
const SUPPORT_FACES: usize = 48;

/// Measure one body as it prints, `up` a unit vector.
pub fn overhang(
    body: &Body,
    mesh: &Mesh,
    faces: &[FaceSummary],
    up: DVec3,
    declared: bool,
    threshold_deg: f64,
) -> Overhang {
    let height = |p: DVec3| p.dot(up);
    let bed = mesh.vertices.iter().map(|v| height(*v)).fold(f64::INFINITY, f64::min);
    // A frame on the bed plane, for spans.
    let seed = if up.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u = up.cross(seed).normalize();
    let v = up.cross(u);

    struct Hit {
        face: usize,
        area: f64,
        angle: f64,
        exact: bool,
        moment: DVec3,
        lo: (f64, f64),
        hi: (f64, f64),
        top: f64,
        bottom: f64,
    }
    let mut hits: Vec<Hit> = Vec::new();
    let mut bed_area = 0.0;
    let mut foot_lo = (f64::INFINITY, f64::INFINITY);
    let mut foot_hi = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for vertex in &mesh.vertices {
        let (a, b) = (vertex.dot(u), vertex.dot(v));
        foot_lo = (foot_lo.0.min(a), foot_lo.1.min(b));
        foot_hi = (foot_hi.0.max(a), foot_hi.1.max(b));
    }
    for run in &mesh.faces {
        let exact = faces.get(run.face).is_some_and(|f| f.surface.kind == "plane");
        let mut hit: Option<Hit> = None;
        for t in run.start..run.start + run.count {
            let [i, j, k] = [mesh.indices[3 * t], mesh.indices[3 * t + 1], mesh.indices[3 * t + 2]];
            let (a, b, c) = (mesh.vertices[i], mesh.vertices[j], mesh.vertices[k]);
            let cross = (b - a).cross(c - a);
            let area = 0.5 * cross.length();
            if area <= 0.0 {
                continue;
            }
            let mut n = cross / (2.0 * area);
            // The mesher's own normals say which way is out.
            let out = mesh.normals[i] + mesh.normals[j] + mesh.normals[k];
            if n.dot(out) < 0.0 {
                n = -n;
            }
            let (ha, hb, hc) = (height(a), height(b), height(c));
            if ha - bed <= BED_TOLERANCE_MM && hb - bed <= BED_TOLERANCE_MM && hc - bed <= BED_TOLERANCE_MM {
                bed_area += area;
                continue;
            }
            let down = -n.dot(up);
            if down <= 1e-9 {
                continue;
            }
            let angle = down.clamp(0.0, 1.0).acos().to_degrees();
            if angle > threshold_deg + 1e-9 {
                continue;
            }
            let entry = hit.get_or_insert_with(|| Hit {
                face: run.face,
                area: 0.0,
                angle: f64::INFINITY,
                exact,
                moment: DVec3::ZERO,
                lo: (f64::INFINITY, f64::INFINITY),
                hi: (f64::NEG_INFINITY, f64::NEG_INFINITY),
                top: f64::NEG_INFINITY,
                bottom: f64::INFINITY,
            });
            entry.area += area;
            entry.angle = entry.angle.min(angle);
            entry.moment += (a + b + c) / 3.0 * area;
            for p in [a, b, c] {
                let (pa, pb) = (p.dot(u), p.dot(v));
                entry.lo = (entry.lo.0.min(pa), entry.lo.1.min(pb));
                entry.hi = (entry.hi.0.max(pa), entry.hi.1.max(pb));
                entry.top = entry.top.max(height(p));
                entry.bottom = entry.bottom.min(height(p));
            }
        }
        hits.extend(hit);
    }
    let footprint = (foot_hi.0 - foot_lo.0).max(0.0) * (foot_hi.1 - foot_lo.1).max(0.0);
    let unsupported: f64 = hits.iter().map(|h| h.area).sum::<f64>().max(0.0);
    let face_count = hits.len();
    hits.sort_by(|a, b| b.area.total_cmp(&a.area));

    // A ceiling is a bridge when a straight line through its middle meets
    // walls going down at both ends, with walls on every side of that middle.
    // The middle may sit in a hole in the ceiling, never in a bay beside it
    // or over material. A lip hangs from one wall, however curved its root;
    // three posts round the middle hold it, but no line through it reaches
    // two. The span is the shortest such line: a bar on two towers 40 apart
    // bridges 40, whatever its width.
    let body_faces: Vec<Face> = body.shape.faces().collect();
    let on_bed = |p: DVec3| [p.dot(u), p.dot(v)];
    let mut caster = body.shape.ray_caster(1e-4);
    let mut bridges = Vec::new();
    let mut listed = Vec::new();
    for hit in hits.iter() {
        let centre = hit.moment / hit.area;
        let span = (hit.hi.0 - hit.lo.0).max(hit.hi.1 - hit.lo.1).max(0.0);
        let summary = faces.get(hit.face);
        let tag = summary.and_then(|f| f.tags.first().cloned());
        let surface = summary.map(|f| f.surface.kind.clone()).unwrap_or_else(|| "face".into());
        let middle = on_bed(centre);
        let ceiling = body_faces.get(hit.face).filter(|face| {
            hit.exact
                && hit.angle <= CEILING_DEG
                && planar_supports::encloses(&segments(&laid_flat(face.outer_wire().edges(), on_bed)), middle)
                && body.shape.classify_point(centre - up * 1e-3, 1e-4) != PointState::Inside
        });
        if let Some(ceiling) = ceiling {
            let held = held_edges(body.shape, ceiling, up);
            let walls = laid_flat(held.iter().cloned(), on_bed);
            let hull = planar_supports::convex_hull(&walls.concat());
            let crossing = planar_supports::contains(&hull, middle, SUPPORT_DEFLECTION_MM)
                .then(|| planar_supports::shortest_crossing(middle, &segments(&walls)))
                .flatten();
            if let Some(span_mm) = crossing {
                let start = centre - up * 1e-3;
                let below = caster
                    .cast(start, -up)
                    .into_iter()
                    .filter(|h| h.distance > 1e-3 && h.crossing == Crossing::Entering)
                    .map(|h| h.distance + 1e-3)
                    .next();
                bridges.push(Bridge {
                    tag: tag.clone(),
                    span_mm,
                    drop_mm: below.unwrap_or(hit.bottom - bed),
                    at: centre.to_array(),
                    sides: held.len(),
                });
            }
        }
        if listed.len() < LISTED {
            listed.push(OverhangFace {
                tag,
                surface,
                angle_deg: hit.angle,
                exact: hit.exact,
                area_mm2: hit.area,
                at: centre.to_array(),
                span_mm: span,
                height_mm: hit.bottom - bed,
            });
        }
    }

    let all_planar = hits.iter().all(|h| h.exact);
    let support_mm3 = (face_count > 0 && face_count <= SUPPORT_FACES && all_planar)
        .then(|| support_volume(body.shape, &body_faces, &hits.iter().map(|h| (h.face, h.top)).collect::<Vec<_>>(), up, bed));

    Overhang {
        body: body.name.map(str::to_owned),
        up: up.to_array(),
        declared,
        threshold_deg,
        bed_mm2: bed_area,
        footprint_fraction: if footprint > 0.0 { bed_area / footprint } else { 0.0 },
        unsupported_mm2: unsupported,
        face_count,
        faces: listed,
        bridges,
        support_mm3,
        sampled: hits.iter().any(|h| !h.exact),
    }
}

/// Each edge sampled to the support deflection and laid flat on the bed.
fn laid_flat(edges: impl Iterator<Item = Edge>, on_bed: impl Fn(DVec3) -> [f64; 2]) -> Vec<Vec<[f64; 2]>> {
    edges.map(|edge| edge.approximation_within(SUPPORT_DEFLECTION_MM).map(&on_bed).collect()).collect()
}

fn segments(lines: &[Vec<[f64; 2]>]) -> Vec<[[f64; 2]; 2]> {
    lines.iter().flat_map(|line| line.windows(2).map(|w| [w[0], w[1]])).collect()
}

/// The ceiling's boundary edges with a wall going down beside them: material
/// just past the edge's middle and just below the ceiling. One probe per
/// B-rep edge, however finely the mesh cut it, classified against one loaded
/// solid.
fn held_edges(shape: &Shape, ceiling: &Face, up: DVec3) -> Vec<Edge> {
    let flat = |p: DVec3| p - up * p.dot(up);
    let (edges, probes): (Vec<Edge>, Vec<DVec3>) = ceiling
        .edges()
        .filter_map(|edge| {
            let (at, derivative) = edge.middle();
            let along = if edge.is_reversed() { -derivative } else { derivative };
            // The face's normal across its direction of travel points into the
            // face, as it does for `SelectableEdge::classify`.
            let away = flat(-ceiling.normal_at(at).cross(along));
            (away.length() > 1e-9).then(|| {
                let probe = at + away.normalize() * PROBE_MM - up * PROBE_MM;
                (edge, probe)
            })
        })
        .unzip();
    edges
        .into_iter()
        .zip(shape.classify_points(&probes, 1e-4))
        .filter_map(|(edge, state)| (state == PointState::Inside).then_some(edge))
        .collect()
}

/// The material a support prism under every overhanging face would hold:
/// each face extruded down to the bed, fused, the body cut out, clipped to
/// above the bed. One boolean per face plus three; exact.
fn support_volume(shape: &Shape, all: &[Face], faces: &[(usize, f64)], up: DVec3, bed: f64) -> f64 {
    let prisms: Vec<Shape> = faces
        .iter()
        .filter_map(|&(index, top)| {
            let face = all.get(index)?;
            let depth = top - bed;
            (depth > BED_TOLERANCE_MM).then(|| Shape::from(face.extrude(-up * depth)))
        })
        .collect();
    let Some((first, rest)) = prisms.split_first() else { return 0.0 };
    let fused = if rest.is_empty() { first.clone() } else { BooleanShape::fuse_all(first, rest).shape };
    let cut = fused.subtract(shape).shape;
    // Clip to the print volume: nothing below the bed counts.
    let Some((lo, hi)) = cut.bounds_optimal() else { return 0.0 };
    let reach = (hi - lo).length() + 1.0;
    let centre = (lo + hi) * 0.5;
    let bed_centre = centre - up * (centre.dot(up) - bed);
    let slab = AdHocShape::make_box_point_point(
        bed_centre - DVec3::splat(reach),
        bed_centre + DVec3::splat(reach),
    )
    .0
    .translated(up * reach);
    let mut clipped = AdHocShape(cut);
    clipped.intersect(&slab);
    clipped.0.signed_volume().abs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::build_part;
    use parcad_core::graph::Doc;

    /// A tee — a 20 x 20 x 2 plate on a 4 x 4 x 8 post — drawn post down.
    /// As drawn the plate's underside overhangs, 400 - 16 = 384 mm², and the
    /// post's foot is the bed, 16 mm². Declared to print up -z the same body
    /// is a plate on the bed with a post standing on it: nothing overhangs,
    /// and 400 mm² is the bed. The face is reported in the body's
    /// orientation, not the part's.
    #[test]
    fn an_overhang_is_measured_in_the_bodys_own_orientation() {
        let doc = |up: Option<[f64; 3]>| -> Doc {
            let body = match up {
                Some([x, y, z]) => format!(r#"{{"name":"tee","child":3,"printed_up":{{"x":{x},"y":{y},"z":{z}}}}}"#),
                None => r#"{"name":"tee","child":3}"#.to_string(),
            };
            serde_json::from_str(&format!(r#"{{"units":"mm","root":4,"nodes":[
                {{"op":"cuboid","size":{{"x":20,"y":20,"z":2}},"tag":"plate"}},
                {{"op":"translate","child":0,"by":{{"x":0,"y":0,"z":9}}}},
                {{"op":"cuboid","size":{{"x":4,"y":4,"z":8}},"tag":"post"}},
                {{"op":"union","children":[1,5],"blend":0}},
                {{"op":"bodies","bodies":[{body}]}},
                {{"op":"translate","child":2,"by":{{"x":0,"y":0,"z":4}}}}]}}"#))
            .unwrap()
        };
        let measure = |doc: &Doc| {
            let part = build_part(doc).unwrap();
            let body = &part.bodies[0];
            let body = Body::new(Some(&body.name), &body.shape, &part.names[0]);
            let mesh = body.shape.mesh();
            let mut faces = crate::perceive::describe_faces(body.shape);
            for (face, tags) in faces.iter_mut().zip(&body.face_tags) {
                face.tags = tags.clone();
            }
            let up = doc.bodies().unwrap()[0].up();
            let declared = doc.bodies().unwrap()[0].printed_up.is_some();
            overhang(&body, &mesh, &faces, DVec3::new(up.x, up.y, up.z), declared, THRESHOLD_DEG)
        };
        let drawn = measure(&doc(None));
        assert!(!drawn.declared);
        assert!((drawn.unsupported_mm2 - 384.0).abs() < 1e-6, "{}", drawn.unsupported_mm2);
        assert!((drawn.bed_mm2 - 16.0).abs() < 1e-6, "{}", drawn.bed_mm2);
        assert_eq!(drawn.face_count, 1);
        let face = &drawn.faces[0];
        assert_eq!((face.tag.as_deref(), face.exact, face.angle_deg), (Some("plate"), true, 0.0));
        assert!((face.span_mm - 20.0).abs() < 1e-6 && (face.height_mm - 8.0).abs() < 1e-6, "{face:?}");
        // A prism under the plate to the bed, less the post: 400 x 8 - 16 x 8.
        assert!((drawn.support_mm3.unwrap() - (384.0 * 8.0)).abs() < 1e-6, "{:?}", drawn.support_mm3);
        assert!(drawn.bridges.is_empty(), "a plate on one post is held on no side");

        let flipped = measure(&doc(Some([0.0, 0.0, -1.0])));
        assert!(flipped.declared);
        assert_eq!((flipped.unsupported_mm2, flipped.face_count, flipped.support_mm3), (0.0, 0, None));
        assert!((flipped.bed_mm2 - 400.0).abs() < 1e-6, "{}", flipped.bed_mm2);
        assert!((flipped.footprint_fraction - 1.0).abs() < 1e-6);
    }

    /// A slot's ceiling is held on both sides and is a bridge; a lip on one
    /// wall is not.
    #[test]
    fn a_ceiling_on_two_walls_is_a_bridge_and_a_lip_is_not() {
        let doc: Doc = serde_json::from_str(r#"{"units":"mm","root":6,"nodes":[
            {"op":"cuboid","size":{"x":30,"y":20,"z":10},"tag":"block"},
            {"op":"translate","child":0,"by":{"x":0,"y":0,"z":5}},
            {"op":"cuboid","size":{"x":12,"y":30,"z":6},"tag":"slot"},
            {"op":"translate","child":2,"by":{"x":0,"y":0,"z":2}},
            {"op":"cuboid","size":{"x":4,"y":20,"z":2},"tag":"lip"},
            {"op":"translate","child":4,"by":{"x":17,"y":0,"z":9}},
            {"op":"union","children":[7,5],"blend":0},
            {"op":"difference","base":1,"tools":[3],"blend":0}]}"#)
        .unwrap();
        let part = build_part(&doc).unwrap();
        let body = Body::new(None, &part.shape, &part.names[0]);
        let mesh = body.shape.mesh();
        let mut faces = crate::perceive::describe_faces(body.shape);
        for (face, tags) in faces.iter_mut().zip(&body.face_tags) {
            face.tags = tags.clone();
        }
        let o = overhang(&body, &mesh, &faces, DVec3::Z, false, THRESHOLD_DEG);
        // The slot's ceiling, 12 x 20 at z 5, and the lip's underside, 4 x 20 at z 8.
        assert!((o.unsupported_mm2 - 320.0).abs() < 1e-6, "{}", o.unsupported_mm2);
        assert_eq!(o.bridges.len(), 1, "{:?}", o.bridges);
        let bridge = &o.bridges[0];
        assert_eq!(bridge.tag.as_deref(), Some("slot"));
        assert!((bridge.span_mm - 12.0).abs() < 1e-6 && (bridge.drop_mm - 5.0).abs() < 1e-6, "{bridge:?}");
        assert_eq!(bridge.sides, 2, "the slot's two walls");
        let lip = o.faces.iter().find(|f| f.tag.as_deref() == Some("lip")).expect("the lip overhangs");
        assert!((lip.area_mm2 - 80.0).abs() < 1e-6 && (lip.height_mm - 8.0).abs() < 1e-6, "{lip:?}");
    }

    fn measure_as_drawn(json: &str) -> Overhang {
        let doc: Doc = serde_json::from_str(json).unwrap();
        let part = build_part(&doc).unwrap();
        let body = Body::new(None, &part.shape, &part.names[0]);
        let mesh = body.shape.mesh();
        let mut faces = crate::perceive::describe_faces(body.shape);
        for (face, tags) in faces.iter_mut().zip(&body.face_tags) {
            face.tags = tags.clone();
        }
        overhang(&body, &mesh, &faces, DVec3::Z, false, THRESHOLD_DEG)
    }

    /// A lip inside a cup's bore hangs from the one wall its root is merged
    /// into. That root is an arc, which the mesh cuts into many pieces whose
    /// ends lie either side of the lip's middle; it is still one wall.
    #[test]
    fn a_lip_on_a_curved_wall_is_not_a_bridge() {
        let o = measure_as_drawn(r#"{"units":"mm","root":6,"nodes":[
            {"op":"cylinder","r":14,"h":14,"tag":"cup"},
            {"op":"cylinder","r":12,"h":15,"tag":"bore"},
            {"op":"translate","child":1,"by":{"x":0,"y":0,"z":1}},
            {"op":"difference","base":0,"tools":[2],"blend":0},
            {"op":"cuboid","size":{"x":2,"y":6,"z":2},"tag":"lip"},
            {"op":"translate","child":4,"by":{"x":11.5,"y":0,"z":6}},
            {"op":"union","children":[3,5],"blend":0}]}"#);
        assert_eq!(o.face_count, 1, "{:?}", o.faces);
        assert_eq!(o.faces[0].tag.as_deref(), Some("lip"));
        assert!(o.bridges.is_empty(), "{:?}", o.bridges);
    }

    /// The ceiling of a hole bored up from below is held all round by the
    /// bore, one closed edge: a bridge as wide as the hole, over the drop to
    /// the bed it opens onto.
    #[test]
    fn a_round_ceiling_held_all_round_is_bridged_across_its_diameter() {
        let o = measure_as_drawn(r#"{"units":"mm","root":4,"nodes":[
            {"op":"cuboid","size":{"x":20,"y":20,"z":10},"tag":"block"},
            {"op":"translate","child":0,"by":{"x":0,"y":0,"z":5}},
            {"op":"cylinder","r":4,"h":6,"tag":"hole"},
            {"op":"translate","child":2,"by":{"x":0,"y":0,"z":2}},
            {"op":"difference","base":1,"tools":[3],"blend":0}]}"#);
        assert_eq!(o.bridges.len(), 1, "{:?}", o.bridges);
        let bridge = &o.bridges[0];
        assert_eq!((bridge.tag.as_deref(), bridge.sides), (Some("hole"), 1), "{bridge:?}");
        assert!((bridge.span_mm - 8.0).abs() <= 2.0 * SUPPORT_DEFLECTION_MM, "{bridge:?}");
        assert!((bridge.drop_mm - 5.0).abs() < 1e-6, "{bridge:?}");
    }

    /// A bar 8 wide laid on two towers 40 apart bridges 40: the gap between
    /// the walls that hold it, not the bar's own width.
    #[test]
    fn a_bar_on_two_towers_bridges_the_gap_between_them() {
        let o = measure_as_drawn(r#"{"units":"mm","root":5,"nodes":[
            {"op":"cuboid","size":{"x":8,"y":8,"z":10},"tag":"tower"},
            {"op":"translate","child":0,"by":{"x":-24,"y":0,"z":5}},
            {"op":"translate","child":0,"by":{"x":24,"y":0,"z":5}},
            {"op":"cuboid","size":{"x":56,"y":8,"z":2},"tag":"bar"},
            {"op":"translate","child":3,"by":{"x":0,"y":0,"z":11}},
            {"op":"union","children":[1,2,4],"blend":0}]}"#);
        assert_eq!(o.bridges.len(), 1, "{:?}", o.bridges);
        let bridge = &o.bridges[0];
        assert_eq!((bridge.tag.as_deref(), bridge.sides), (Some("bar"), 2), "{bridge:?}");
        assert!((bridge.span_mm - 40.0).abs() < 1e-6 && (bridge.drop_mm - 10.0).abs() < 1e-6, "{bridge:?}");
    }

    /// A screw hole through the middle of that bar leaves its middle in the
    /// hole: still inside the ceiling's outline, still a 40 mm bridge.
    #[test]
    fn a_hole_in_the_middle_of_a_bridge_leaves_it_a_bridge() {
        let o = measure_as_drawn(r#"{"units":"mm","root":8,"nodes":[
            {"op":"cuboid","size":{"x":8,"y":8,"z":10},"tag":"tower"},
            {"op":"translate","child":0,"by":{"x":-24,"y":0,"z":5}},
            {"op":"translate","child":0,"by":{"x":24,"y":0,"z":5}},
            {"op":"cuboid","size":{"x":56,"y":8,"z":2},"tag":"bar"},
            {"op":"translate","child":3,"by":{"x":0,"y":0,"z":11}},
            {"op":"union","children":[1,2,4],"blend":0},
            {"op":"cylinder","r":1.6,"h":4},
            {"op":"translate","child":6,"by":{"x":0,"y":0,"z":11}},
            {"op":"difference","base":5,"tools":[7],"blend":0}]}"#);
        assert_eq!(o.bridges.len(), 1, "{:?}", o.bridges);
        let bridge = &o.bridges[0];
        assert_eq!((bridge.tag.as_deref(), bridge.sides), (Some("bar"), 2), "{bridge:?}");
        assert!((bridge.span_mm - 40.0).abs() < 1e-6, "{bridge:?}");
    }

    /// Three posts round a plate's middle hold it on every side, but a line
    /// through the middle that reaches one post leaves between the other two:
    /// nothing bridges it, and the plate's underside stays an overhang.
    #[test]
    fn a_plate_on_three_posts_round_its_middle_is_not_bridged_through_it() {
        let o = measure_as_drawn(r#"{"units":"mm","root":6,"nodes":[
            {"op":"cuboid","size":{"x":4,"y":4,"z":10},"tag":"post"},
            {"op":"translate","child":0,"by":{"x":15,"y":0,"z":5}},
            {"op":"translate","child":0,"by":{"x":-7.5,"y":12.990381,"z":5}},
            {"op":"translate","child":0,"by":{"x":-7.5,"y":-12.990381,"z":5}},
            {"op":"cylinder","r":22,"h":2,"tag":"plate"},
            {"op":"translate","child":4,"by":{"x":0,"y":0,"z":11}},
            {"op":"union","children":[1,2,3,5],"blend":0}]}"#);
        let plate = o.faces.iter().find(|f| f.tag.as_deref() == Some("plate")).expect("the plate overhangs");
        assert_eq!((plate.exact, plate.angle_deg), (true, 0.0), "{plate:?}");
        assert!(o.bridges.is_empty(), "{:?}", o.bridges);
    }

    /// A plate on two posts is held by the posts' four sides each, inside
    /// its own outline, and bridges the 30 between their facing sides.
    #[test]
    fn a_plate_on_two_posts_bridges_between_their_facing_sides() {
        let o = measure_as_drawn(r#"{"units":"mm","root":5,"nodes":[
            {"op":"cuboid","size":{"x":4,"y":4,"z":10},"tag":"post"},
            {"op":"translate","child":0,"by":{"x":-17,"y":0,"z":5}},
            {"op":"translate","child":0,"by":{"x":17,"y":0,"z":5}},
            {"op":"cuboid","size":{"x":50,"y":10,"z":2},"tag":"plate"},
            {"op":"translate","child":3,"by":{"x":0,"y":0,"z":11}},
            {"op":"union","children":[1,2,4],"blend":0}]}"#);
        assert_eq!(o.bridges.len(), 1, "{:?}", o.bridges);
        let bridge = &o.bridges[0];
        assert_eq!((bridge.tag.as_deref(), bridge.sides), (Some("plate"), 8), "{bridge:?}");
        assert!((bridge.span_mm - 30.0).abs() < 1e-6 && (bridge.drop_mm - 10.0).abs() < 1e-6, "{bridge:?}");
    }

    /// A shelf 2 deep round half a bore is held by one wall all along its
    /// curve; its middle is out in the bore, off the shelf, so it is a ledge
    /// however much of the circle the wall covers.
    #[test]
    fn a_half_ring_shelf_round_a_bore_is_not_a_bridge() {
        let o = measure_as_drawn(r#"{"units":"mm","root":11,"nodes":[
            {"op":"cylinder","r":16,"h":14,"tag":"cup"},
            {"op":"cylinder","r":12,"h":15,"tag":"bore"},
            {"op":"translate","child":1,"by":{"x":0,"y":0,"z":1}},
            {"op":"difference","base":0,"tools":[2],"blend":0},
            {"op":"cylinder","r":12.5,"h":2},
            {"op":"cylinder","r":10,"h":3},
            {"op":"difference","base":4,"tools":[5],"blend":0},
            {"op":"cuboid","size":{"x":30,"y":15,"z":4}},
            {"op":"translate","child":7,"by":{"x":0,"y":7.5,"z":0}},
            {"op":"intersection","children":[6,8],"blend":0,"tag":"shelf"},
            {"op":"translate","child":9,"by":{"x":0,"y":0,"z":4}},
            {"op":"union","children":[3,10],"blend":0}]}"#);
        let shelf = o.faces.iter().find(|f| f.tag.as_deref() == Some("shelf"));
        let shelf = shelf.unwrap_or_else(|| panic!("the shelf overhangs: {:?}", o.faces));
        assert_eq!((shelf.exact, shelf.angle_deg), (true, 0.0), "{shelf:?}");
        assert!(o.bridges.is_empty(), "{:?}", o.bridges);
    }
}
