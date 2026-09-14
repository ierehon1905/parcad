//! Questions asked of the exact solid: is there material at this point, what
//! does this line cross, where is the part thinnest, where is each tag.
//!
//! Every answer here is measured on the B-rep the kernel built — a point
//! against `BRepClass3d`, a line against the surfaces themselves, a tag
//! against the faces its lineage says it owns. Nothing is read off a mesh or
//! a field, so a fillet is in what gets measured and a distance is the
//! distance, at a corner as on a face. The one place the tessellation is used
//! is to choose *where* to fire the thickness sweep's rays from: its nodes lie
//! on the surface, and a planar face's triangles give the middle of a face the
//! mesher never puts a node in.

use crate::backend::{face_key, BuiltPart, FaceKey, NamedFaces};
use crate::protocol::{
    FaceSummary, Perceive, Perceived, PointResult, PointWhere, RayHitResult, RayLine, RayResult,
    TagBounds, ThicknessResult, ThicknessSample, ThicknessSpec,
};
use anyhow::{bail, Result};
use glam::DVec3;
use opencascade::primitives::{Compound, Crossing, PointState, RayCaster, Shape};
use std::collections::{HashMap, HashSet};

/// How near a point must be to a face to count as on it, and the face-boundary
/// tolerance the ray intersector classifies hits with, in mm. A hit within this
/// of an edge is reported by both faces that share it, which the walk below
/// expects; smaller and a ray through an edge can be reported by neither.
const SURFACE_TOLERANCE_MM: f64 = 1e-4;

/// A hit nearer the ray's origin than this is the face the ray started on.
const OWN_FACE_MM: f64 = 1e-7;

/// How many distinct thin spots to report.
const MAX_THIN_SPOTS: usize = 8;

/// One finished body with the lookup every question needs: which tags each
/// face carries.
pub struct Body<'a> {
    pub name: Option<&'a str>,
    pub shape: &'a Shape,
    /// Per face, in the shape's own traversal order: its tags, innermost
    /// first. The innermost tag is the one authored nearest the primitive —
    /// `bore` rather than the `part` that was unioned around it.
    pub face_tags: Vec<Vec<String>>,
    /// The body's faces alone, for distances. `BRepExtrema` reads a solid as
    /// a volume, so a point inside one is 0 from it; the distance a probe
    /// reports is to the boundary.
    boundary: Shape,
}

impl<'a> Body<'a> {
    pub fn new(name: Option<&'a str>, shape: &'a Shape, names: &NamedFaces) -> Self {
        Self {
            name,
            shape,
            face_tags: face_tags(shape, names),
            boundary: Compound::from_shapes(shape.faces().map(Shape::from)).into(),
        }
    }

    fn tags_of(&self, face: usize) -> Vec<String> {
        self.face_tags.get(face).cloned().unwrap_or_default()
    }
}

/// The finished bodies of a part, one for a one-solid part.
pub fn bodies_of(part: &BuiltPart) -> Vec<Body<'_>> {
    if part.bodies.is_empty() {
        return vec![Body::new(None, &part.shape, &part.names[0])];
    }
    part.bodies
        .iter()
        .zip(&part.names)
        .map(|((name, shape), names)| Body::new(Some(name), shape, names))
        .collect()
}

/// What each face is, from the kernel's own report. Empty rather than fatal:
/// the geometry is the answer, and a description of it is an aid.
pub fn describe_faces(shape: &Shape) -> Vec<FaceSummary> {
    serde_json::from_str(&shape.faces_json()).unwrap_or_default()
}

/// Which tags each face of `shape` carries, innermost first.
///
/// The lineage's faces are the result's own sub-shapes, so the map lookup
/// answers for nearly all of them; a face the map does not know — one a
/// history handed back as a copy — is matched by its boundary geometry instead,
/// which is slower and never wrong.
pub fn face_tags(shape: &Shape, names: &NamedFaces) -> Vec<Vec<String>> {
    let map = shape.face_map();
    let mut tags: Vec<Vec<String>> = vec![Vec::new(); map.len()];
    let mut by_key: Option<HashMap<FaceKey, usize>> = None;
    for (tag, faces) in &names.tags {
        for face in faces {
            let index = map.index_of(face).or_else(|| {
                by_key
                    .get_or_insert_with(|| {
                        shape
                            .faces()
                            .enumerate()
                            .map(|(i, f)| (face_key(&f), i))
                            .collect()
                    })
                    .get(&face_key(face))
                    .copied()
            });
            if let Some(index) = index {
                if !tags[index].iter().any(|t| t == tag) {
                    tags[index].push(tag.clone());
                }
            }
        }
    }
    tags
}

/// Where every tag is: the exact bounds of the faces of the finished part
/// that carry it, over every body, each face counted once. A tag no face
/// carries is unlocated.
///
/// Each face is bounded once and its box folded into every tag it carries,
/// rather than a compound bounded per tag: a face carries every enclosing
/// tag, so bounding per tag measured the same blend surfaces once per name.
pub fn tag_extents(part: &BuiltPart, bodies: &[Body]) -> (Vec<TagBounds>, Vec<String>) {
    let mut found: Vec<TagBounds> = Vec::new();
    let mut unlocated: Vec<String> = Vec::new();
    // Tag order is node order, and it is the same on every body.
    let Some(first) = part.names.first() else {
        return (found, unlocated);
    };
    let mut owned: HashMap<&str, (usize, DVec3, DVec3)> = HashMap::new();
    for body in bodies {
        for (i, face) in body.shape.faces().enumerate() {
            let Some(tags) = body.face_tags.get(i).filter(|t| !t.is_empty()) else {
                continue;
            };
            let Some((lo, hi)) = Shape::from(face.clone()).bounds_optimal() else {
                continue;
            };
            for tag in tags {
                let entry = owned
                    .entry(tag.as_str())
                    .or_insert((0, DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)));
                entry.0 += 1;
                entry.1 = entry.1.min(lo);
                entry.2 = entry.2.max(hi);
            }
        }
    }
    for (tag, _) in &first.tags {
        match owned.get(tag.as_str()) {
            Some((faces, lo, hi)) => found.push(TagBounds {
                tag: tag.clone(),
                min: lo.to_array(),
                max: hi.to_array(),
                faces: *faces,
            }),
            None => unlocated.push(tag.clone()),
        }
    }
    (found, unlocated)
}

/// Answer every question in `spec` against these bodies.
pub fn perceive(bodies: &[Body], spec: &Perceive) -> Result<Perceived> {
    let extent = bodies
        .iter()
        .filter_map(|b| b.shape.bounds_optimal())
        .fold(None, |acc: Option<(DVec3, DVec3)>, (lo, hi)| {
            Some(match acc {
                None => (lo, hi),
                Some((a, b)) => (a.min(lo), b.max(hi)),
            })
        });
    let Some((lo, hi)) = extent else {
        bail!("the part has no extent to measure");
    };
    let centre = (lo + hi) * 0.5;
    let radius = (hi - lo).length() * 0.5;

    let points = spec
        .points
        .iter()
        .map(|p| classify(bodies, DVec3::from_array(*p)))
        .collect();

    let mut casters: Vec<RayCaster> = bodies
        .iter()
        .map(|b| b.shape.ray_caster(SURFACE_TOLERANCE_MM))
        .collect();
    let rays = spec
        .rays
        .iter()
        .map(|line| {
            let origin = DVec3::from_array(line.origin);
            // Far enough to leave the part from wherever the ray starts. A
            // caller that gave no length meant "all the way through".
            let reach = ((origin - centre).length() + radius) * 1.05 + 1.0;
            cast(bodies, &mut casters, line, line.max_distance.unwrap_or(reach))
        })
        .collect::<Result<Vec<_>>>()?;

    let thickness = spec
        .thickness
        .as_ref()
        .map(|t| thickness(bodies, &mut casters, t, (hi - lo).length()));

    Ok(Perceived {
        points,
        rays,
        thickness,
    })
}

/// Which side of the boundary a point is on, and how far from it.
fn classify(bodies: &[Body], point: DVec3) -> PointResult {
    let mut nearest: Option<(f64, DVec3, Option<&str>)> = None;
    let mut inside: Option<&str> = None;
    let mut on_boundary = false;
    for body in bodies {
        match body.shape.classify_point(point, SURFACE_TOLERANCE_MM) {
            PointState::Inside => inside = inside.or(body.name),
            PointState::OnBoundary => on_boundary = true,
            PointState::Outside | PointState::Unknown => {}
        }
        if let Some((distance, at)) = body.boundary.distance_to_point(point) {
            if nearest.is_none_or(|(d, _, _)| distance < d) {
                nearest = Some((distance, at, body.name));
            }
        }
    }
    let (distance, at, near_body) = nearest.unwrap_or((0.0, point, None));
    let inside_any = bodies.iter().any(|b| {
        b.shape.classify_point(point, SURFACE_TOLERANCE_MM) == PointState::Inside
    });
    let (where_, signed) = if inside_any {
        (PointWhere::Inside, -distance)
    } else if on_boundary || distance <= SURFACE_TOLERANCE_MM {
        (PointWhere::OnBoundary, 0.0)
    } else {
        (PointWhere::Outside, distance)
    };
    PointResult {
        point: point.to_array(),
        state: where_,
        distance_mm: signed,
        nearest: at.to_array(),
        body: if bodies.len() > 1 {
            inside.or(near_body).map(str::to_owned)
        } else {
            None
        },
    }
}

/// One body's crossings along a line, walked so that entering and leaving
/// alternate: a hit the intersector reports twice — once per face sharing the
/// edge it went through — or a grazing contact is dropped rather than doubled.
struct Walk {
    starts_inside: bool,
    ends_inside: bool,
    hits: Vec<RayHitResult>,
    solid_mm: f64,
    first_solid_mm: Option<f64>,
}

fn walk(body: &Body, caster: &mut RayCaster, origin: DVec3, dir: DVec3, max: f64) -> Walk {
    let starts_inside = body.shape.classify_point(origin, SURFACE_TOLERANCE_MM) == PointState::Inside;
    let mut inside = starts_inside;
    let mut hits = Vec::new();
    for hit in caster.cast(origin, dir) {
        if hit.distance < -SURFACE_TOLERANCE_MM || hit.distance > max {
            continue;
        }
        let entering = match hit.crossing {
            Crossing::Entering => true,
            Crossing::Leaving => false,
            Crossing::Tangent => continue,
        };
        if entering == inside {
            continue;
        }
        inside = entering;
        hits.push(RayHitResult {
            distance: hit.distance.max(0.0),
            point: hit.point.to_array(),
            entering,
            tags: body.tags_of(hit.face),
            body: body.name.map(str::to_owned),
        });
    }

    // Pair the crossings into runs of material. A run that had already begun
    // when the ray started counts towards `solid_mm` and never towards
    // `first_solid_mm`, which is a measurement of a wall.
    let mut solid_mm = 0.0;
    let mut first_solid_mm = None;
    let mut span_start = starts_inside.then_some(0.0);
    let mut span_is_complete = !starts_inside;
    for hit in &hits {
        if hit.entering {
            span_start = Some(hit.distance);
            span_is_complete = true;
        } else if let Some(start) = span_start.take() {
            let length = hit.distance - start;
            solid_mm += length;
            if span_is_complete && first_solid_mm.is_none() {
                first_solid_mm = Some(length);
            }
        }
    }
    if let Some(start) = span_start {
        solid_mm += max - start;
    }
    Walk {
        starts_inside,
        ends_inside: inside,
        hits,
        solid_mm,
        first_solid_mm,
    }
}

fn cast(bodies: &[Body], casters: &mut [RayCaster], line: &RayLine, max: f64) -> Result<RayResult> {
    let origin = DVec3::from_array(line.origin);
    let direction = DVec3::from_array(line.direction);
    let len = direction.length();
    if !len.is_finite() || len < 1e-9 {
        bail!("the ray direction has no length; give a direction such as [0, 0, 1]");
    }
    if !(max > 0.0) || !max.is_finite() {
        bail!("max_distance must be a positive length in mm, not {max}");
    }
    let dir = direction / len;

    let walks: Vec<Walk> = bodies
        .iter()
        .zip(casters.iter_mut())
        .map(|(body, caster)| walk(body, caster, origin, dir, max))
        .collect();

    let mut hits: Vec<RayHitResult> = walks.iter().flat_map(|w| w.hits.iter().cloned()).collect();
    hits.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    Ok(RayResult {
        origin: origin.to_array(),
        direction: dir.to_array(),
        max_distance: max,
        starts_inside: walks.iter().any(|w| w.starts_inside),
        ends_inside: walks.iter().any(|w| w.ends_inside),
        hits,
        solid_mm: walks.iter().map(|w| w.solid_mm).sum(),
        // The first complete run over every body: the one that begins nearest
        // the origin.
        first_solid_mm: walks
            .iter()
            .filter_map(|w| {
                let start = w.hits.iter().find(|h| h.entering)?.distance;
                w.first_solid_mm.map(|t| (start, t))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, t)| t),
    })
}

/// How far a thickness sample may sit off the exact surface, in mm: the
/// mesher's deflection. A grid point inside a curved triangle lies on the
/// chord, within this of the surface on the material's side or the void's.
const DEFLECTION_MM: f64 = 0.01;

/// A ray from every sampled surface point, back along its own outward normal.
///
/// The samples are the exact tessellation's nodes — they lie on the surface,
/// and the mesher's normal at each is the surface's — and a grid over every
/// triangle at a spacing tied to the part's size, normals interpolated. The
/// grid is what finds a wall in the middle of a face: a plane is meshed as
/// two triangles with no node inside its boundary, and a bore's wall as two
/// rings, so the nodes alone never fire a ray from the middle of either. A
/// grid point on a curved face is on the chord, within the deflection of the
/// surface; a ray that starts that far into the void enters material at once
/// and is measured from there.
fn thickness(
    bodies: &[Body],
    casters: &mut [RayCaster],
    spec: &ThicknessSpec,
    diagonal: f64,
) -> ThicknessResult {
    let mut samples: Vec<ThicknessSample> = Vec::new();
    let mut discarded = 0usize;
    let reach = diagonal * 1.05 + 1.0;
    // About a hundred samples across the part's diagonal, before decimation:
    // the density the old render-driven sweep had at its default 96 px.
    let spacing = (diagonal / 96.0).max(1e-3);

    let mut starts: Vec<(usize, DVec3, DVec3, usize)> = Vec::new();
    for (b, body) in bodies.iter().enumerate() {
        let mesh = body.shape.mesh();
        for run in &mesh.faces {
            let mut seen: HashSet<usize> = HashSet::new();
            for tri in mesh.indices[run.start * 3..(run.start + run.count) * 3].chunks_exact(3) {
                for &i in tri {
                    if seen.insert(i) {
                        starts.push((b, mesh.vertices[i], mesh.normals[i], run.face));
                    }
                }
                let (p, q, r) = (mesh.vertices[tri[0]], mesh.vertices[tri[1]], mesh.vertices[tri[2]]);
                let (np, nq, nr) = (mesh.normals[tri[0]], mesh.normals[tri[1]], mesh.normals[tri[2]]);
                let longest = (q - p).length().max((r - q).length()).max((p - r).length());
                let n = (longest / spacing).ceil() as usize;
                if n < 2 {
                    continue;
                }
                // Barycentric grid strictly inside the triangle; its corners
                // and edges are the nodes and their neighbours' grids.
                for i in 1..n {
                    for j in 1..n - i {
                        let (u, v) = (i as f64 / n as f64, j as f64 / n as f64);
                        let w = 1.0 - u - v;
                        starts.push((
                            b,
                            p * w + q * u + r * v,
                            np * w + nq * u + nr * v,
                            run.face,
                        ));
                    }
                }
            }
        }
    }
    // Decimate evenly rather than truncating, so every face keeps a share.
    let max = spec.max_samples.max(1);
    if starts.len() > max {
        let stride = starts.len().div_ceil(max);
        starts = starts.into_iter().step_by(stride).collect();
    }

    for (b, at, normal, face) in starts {
        let len = normal.length();
        if !len.is_finite() || len < 0.5 {
            discarded += 1;
            continue;
        }
        let inward = -normal / len;
        let body = &bodies[b];
        let hits: Vec<_> = casters[b]
            .cast(at, inward)
            .into_iter()
            .filter(|h| h.distance.abs() > OWN_FACE_MM && h.crossing != Crossing::Tangent)
            .collect();
        // A grid point on a curved triangle is on the chord, within the
        // deflection of the surface on one side or the other. The wall is
        // measured from where the line actually enters the material: just
        // ahead when the start is in the void, just behind when it is
        // already inside. A node starts on the surface and needs neither.
        let from = hits
            .iter()
            .filter(|h| h.crossing == Crossing::Entering && h.distance.abs() <= 2.0 * DEFLECTION_MM)
            .map(|h| h.distance)
            .min_by(|a, b| a.abs().total_cmp(&b.abs()))
            .unwrap_or(0.0);
        let first = hits.into_iter().find(|h| h.distance > from + OWN_FACE_MM);
        match first {
            Some(hit) if hit.crossing == Crossing::Leaving && hit.distance <= reach => {
                samples.push(ThicknessSample {
                    thickness_mm: hit.distance - from,
                    at: (at + inward * from).to_array(),
                    opposite: hit.point.to_array(),
                    inward: inward.to_array(),
                    tags: body.tags_of(face),
                    opposite_tags: body.tags_of(hit.face),
                    body: body.name.map(str::to_owned),
                });
            }
            _ => discarded += 1,
        }
    }

    samples.sort_by(|a, b| a.thickness_mm.total_cmp(&b.thickness_mm));
    let below_threshold = spec
        .threshold_mm
        .map(|t| samples.iter().filter(|s| s.thickness_mm <= t).count())
        .unwrap_or(0);
    ThicknessResult {
        samples: samples.len(),
        discarded,
        min: samples.first().cloned(),
        below_threshold,
        thin_spots: distinct(&samples, spec.threshold_mm, diagonal),
    }
}

/// The worst samples with near-duplicates suppressed, so the list names
/// distinct features rather than a hundred neighbouring points of one.
fn distinct(sorted: &[ThicknessSample], threshold: Option<f64>, diagonal: f64) -> Vec<ThicknessSample> {
    let apart = diagonal * 0.05;
    let mut kept: Vec<ThicknessSample> = Vec::new();
    for s in sorted {
        if kept.len() >= MAX_THIN_SPOTS {
            break;
        }
        if threshold.is_some_and(|t| s.thickness_mm > t) {
            break;
        }
        let near = kept.iter().any(|k| {
            (DVec3::from_array(k.at) - DVec3::from_array(s.at)).length() < apart
        });
        if !near {
            kept.push(s.clone());
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::build_part;
    use parcad_core::graph::Doc;

    fn built(json: &str) -> BuiltPart {
        let doc: Doc = serde_json::from_str(json).unwrap();
        build_part(&doc).unwrap()
    }

    fn ask(part: &BuiltPart, spec: Perceive) -> Perceived {
        perceive(&bodies_of(part), &spec).unwrap()
    }

    fn ray(origin: [f64; 3], direction: [f64; 3]) -> Perceive {
        Perceive {
            rays: vec![RayLine {
                origin,
                direction,
                max_distance: None,
            }],
            ..Default::default()
        }
    }

    /// A 40 mm plate, 6 thick, with a Ø12 bore through its middle.
    const PLATE: &str = r#"{"units":"mm","root":3,"nodes":[
        {"op":"cuboid","size":{"x":40,"y":40,"z":6},"tag":"plate"},
        {"op":"cylinder","r":6,"h":20,"tag":"bore"},
        {"op":"translate","child":1,"by":{"x":0,"y":0,"z":0}},
        {"op":"difference","base":0,"tools":[2],"blend":0}]}"#;

    /// The number the tool exists for: from x = -100 the ray crosses the
    /// plate at -20, -6, 6 and 20, so the wall beside the bore is 14 mm and
    /// there are 28 mm of material in all — closed forms, exact to the micron.
    #[test]
    fn a_ray_across_the_plate_measures_the_wall_beside_the_bore() {
        let part = built(PLATE);
        let answer = ask(&part, ray([-100.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
        let r = &answer.rays[0];
        let at: Vec<f64> = r.hits.iter().map(|h| h.point[0]).collect();
        assert_eq!(r.hits.len(), 4, "{at:?}");
        for (got, want) in at.iter().zip([-20.0, -6.0, 6.0, 20.0]) {
            assert!((got - want).abs() < 1e-6, "crossing at {got}, expected {want}");
        }
        let entering: Vec<bool> = r.hits.iter().map(|h| h.entering).collect();
        assert_eq!(entering, [true, false, true, false]);
        assert!((r.first_solid_mm.unwrap() - 14.0).abs() < 1e-6);
        assert!((r.solid_mm - 28.0).abs() < 1e-6);
        assert!(!r.starts_inside && !r.ends_inside);
        assert!(r.max_distance > 100.0);
        // Named: the faces read plate, bore, bore, plate.
        let named: Vec<&str> = r.hits.iter().map(|h| h.tags[0].as_str()).collect();
        assert_eq!(named, ["plate", "bore", "bore", "plate"]);
    }

    #[test]
    fn a_ray_down_the_bore_finds_no_material_and_a_point_in_it_is_void() {
        let part = built(PLATE);
        let mut spec = ray([0.0, 0.0, -100.0], [0.0, 0.0, 1.0]);
        spec.points = vec![[0.0, 0.0, 0.0], [19.0, 0.0, 0.0], [0.0, 0.0, 100.0]];
        let answer = ask(&part, spec);
        assert!(answer.rays[0].hits.is_empty());
        assert_eq!(answer.rays[0].solid_mm, 0.0);

        // The centre of the bore is exactly 6 from its wall; a point 1 mm in
        // from the +X face is exactly 1 mm inside, with the sign to say so.
        let [centre, wall, far] = answer.points.as_slice() else { panic!() };
        assert_eq!(centre.state, PointWhere::Outside);
        assert!((centre.distance_mm - 6.0).abs() < 1e-6, "{}", centre.distance_mm);
        assert_eq!(wall.state, PointWhere::Inside);
        assert!((wall.distance_mm + 1.0).abs() < 1e-6, "{}", wall.distance_mm);
        assert!((wall.nearest[0] - 20.0).abs() < 1e-6);
        // Above the bore's axis the nearest boundary is the bore's rim, not
        // the top face, which has a hole where the point looks down: 97 up
        // and 6 across.
        assert_eq!(far.state, PointWhere::Outside);
        let rim = (97.0f64 * 97.0 + 36.0).sqrt();
        assert!((far.distance_mm - rim).abs() < 1e-6, "{}", far.distance_mm);
    }

    #[test]
    fn a_ray_starting_inside_material_says_so() {
        let part = built(PLATE);
        let answer = ask(&part, ray([-18.0, 0.0, 0.0], [-1.0, 0.0, 0.0]));
        let r = &answer.rays[0];
        assert!(r.starts_inside && !r.ends_inside);
        assert_eq!(r.hits.len(), 1);
        assert!(!r.hits[0].entering);
        assert!((r.hits[0].distance - 2.0).abs() < 1e-6);
        assert!((r.solid_mm - 2.0).abs() < 1e-6);
        assert_eq!(r.first_solid_mm, None);
    }

    /// A ray through an edge is reported by both faces sharing it; the walk
    /// must count it once. Along the top face's diagonal, at the plate's
    /// exact top, both the top face and each side face see every crossing.
    #[test]
    fn a_ray_along_an_edge_counts_each_crossing_once() {
        let part = built(PLATE);
        let answer = ask(&part, ray([-100.0, -100.0, 3.0], [1.0, 1.0, 0.0]));
        let r = &answer.rays[0];
        let entering: Vec<bool> = r.hits.iter().map(|h| h.entering).collect();
        assert!(
            entering.windows(2).all(|w| w[0] != w[1]),
            "crossings must alternate: {entering:?}"
        );
    }

    /// The whole reason for the move: a fillet is in what gets measured.
    ///
    /// A 30 × 30 × 8 plate with its top edges rounded at r = 2 is 8 mm thick
    /// from its top face, and less from its bottom face under the round: a
    /// ray up from the underside at x = 14 leaves through the fillet at
    /// z = 2 + √(4 − 1²), so the wall there is 4 + 2 + √3 = 7.732 mm, and
    /// it thins to 6 at the side wall. The old field measured the sharp
    /// corner — 8 everywhere — and called that an upper bound; this measures
    /// the number.
    #[test]
    fn the_thinnest_wall_under_a_fillet_is_measured_through_the_fillet() {
        let part = built(
            r#"{"units":"mm","root":1,"nodes":[
            {"op":"cuboid","size":{"x":30,"y":30,"z":8},"tag":"body"},
            {"op":"fillet","child":0,"radius":2,"selector":">Z"}]}"#,
        );
        let answer = ask(
            &part,
            Perceive {
                rays: vec![
                    RayLine { origin: [0.0, 0.0, 100.0], direction: [0.0, 0.0, -1.0], max_distance: None },
                    RayLine { origin: [14.0, 0.0, -100.0], direction: [0.0, 0.0, 1.0], max_distance: None },
                ],
                thickness: Some(ThicknessSpec { max_samples: 6000, threshold_mm: None }),
                ..Default::default()
            },
        );
        assert!((answer.rays[0].first_solid_mm.unwrap() - 8.0).abs() < 1e-6);
        let under = answer.rays[1].first_solid_mm.unwrap();
        let expected = 6.0 + 3.0f64.sqrt();
        assert!((under - expected).abs() < 1e-6, "under the fillet: {under}, expected {expected}");

        // The sweep's minimum is below 8 — the fillet is in what it measured
        // — and no ray can measure less than the 6 mm at the side wall.
        let thickness = answer.thickness.unwrap();
        let min = thickness.min.unwrap();
        assert!(min.thickness_mm < 7.5, "{min:?}");
        assert!(min.thickness_mm > 6.0 - 1e-6, "{min:?}");
        // It is the underside that found it, at the wall.
        assert!((min.at[2] + 4.0).abs() < 1e-6 && min.at[0].abs().max(min.at[1].abs()) > 13.0, "{min:?}");
        assert!(thickness.samples > 500, "{}", thickness.samples);
    }

    /// The manifold of docs/PERCEPTION.md §3: a blind port down Z meeting a
    /// gallery along X. Across the part at the gallery's height the void is
    /// bounded by the *port*, which is what says they meet.
    #[test]
    fn crossings_tell_two_voids_that_meet_apart() {
        let part = built(
            r#"{"units":"mm","root":5,"nodes":[
            {"op":"cuboid","size":{"x":60,"y":30,"z":30},"tag":"block"},
            {"op":"cylinder","r":5,"h":25},
            {"op":"translate","child":1,"by":{"x":-20,"y":0,"z":7.5},"tag":"port"},
            {"op":"cylinder","r":4,"h":80},
            {"op":"rotate","child":3,"axis":{"x":0,"y":1,"z":0},"degrees":90,"tag":"gallery"},
            {"op":"difference","base":0,"tools":[2,4],"blend":0}]}"#,
        );
        let answer = ask(&part, ray([-20.0, -40.0, 0.0], [0.0, 1.0, 0.0]));
        let r = &answer.rays[0];
        let named: Vec<(&str, bool)> = r.hits.iter().map(|h| (h.tags[0].as_str(), h.entering)).collect();
        assert_eq!(
            named,
            [("block", true), ("port", false), ("port", true), ("block", false)],
            "{:?}",
            r.hits
        );
        let void = r.hits[2].distance - r.hits[1].distance;
        assert!((void - 10.0).abs() < 1e-6, "got {void}");

        // And the sweep finds the 5 mm left outboard of the port without
        // being asked where: the block runs to x = -30, the port to -25.
        let answer = ask(
            &part,
            Perceive {
                thickness: Some(ThicknessSpec { max_samples: 4000, threshold_mm: Some(6.0) }),
                ..Default::default()
            },
        );
        let t = answer.thickness.unwrap();
        let min = t.min.unwrap();
        // Exact for its own sample point, which lies a fraction of a degree
        // off the port's -X generator, where the normal tilts and the wall
        // along it is a few microns longer than the 5.000 at the generator
        // itself — a sampled minimum, not a rounded one.
        assert!((min.thickness_mm - 5.0).abs() < 1e-3, "{min:?}");
        let mut between = [min.tags[0].as_str(), min.opposite_tags[0].as_str()];
        between.sort();
        assert_eq!(between, ["block", "port"]);
        assert!(t.below_threshold > 0);
    }

    /// Two bodies, each answered on its own and the answers merged.
    #[test]
    fn a_part_in_two_bodies_is_probed_per_body() {
        let part = built(
            r#"{"units":"mm","root":4,"nodes":[
            {"op":"cuboid","size":{"x":10,"y":10,"z":10},"tag":"a"},
            {"op":"cuboid","size":{"x":10,"y":10,"z":10},"tag":"b"},
            {"op":"translate","child":1,"by":{"x":20,"y":0,"z":0}},
            {"op":"translate","child":0,"by":{"x":0,"y":0,"z":0}},
            {"op":"bodies","bodies":[{"name":"left","child":3},{"name":"right","child":2}]}]}"#,
        );
        let mut spec = ray([-50.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        spec.points = vec![[20.0, 0.0, 0.0], [10.0, 0.0, 0.0]];
        let answer = ask(&part, spec);
        let r = &answer.rays[0];
        let bodies: Vec<&str> = r.hits.iter().map(|h| h.body.as_deref().unwrap()).collect();
        assert_eq!(bodies, ["left", "left", "right", "right"]);
        assert!((r.solid_mm - 20.0).abs() < 1e-6);
        assert!((r.first_solid_mm.unwrap() - 10.0).abs() < 1e-6);
        assert_eq!(answer.points[0].body.as_deref(), Some("right"));
        assert_eq!(answer.points[0].state, PointWhere::Inside);
        // Halfway between them: outside both, 5 from each, named for one.
        assert_eq!(answer.points[1].state, PointWhere::Outside);
        assert!((answer.points[1].distance_mm - 5.0).abs() < 1e-6);

        let (extents, unlocated) = tag_extents(&part, &bodies_of(&part));
        assert!(unlocated.is_empty());
        let b = extents.iter().find(|e| e.tag == "b").unwrap();
        assert_eq!((b.min[0], b.max[0]), (15.0, 25.0));
        assert_eq!(b.faces, 6);
    }

    /// The extents are exact: the bore's wall spans x 4..16 to the micron,
    /// where the sampled field reported it short by the mesh spacing, and a
    /// tag nothing of survives is named rather than dropped.
    #[test]
    fn a_tag_is_bounded_where_its_own_faces_are() {
        let part = built(
            r#"{"units":"mm","root":5,"nodes":[
            {"op":"cuboid","size":{"x":40,"y":40,"z":40},"tag":"body"},
            {"op":"cylinder","r":6,"h":60},
            {"op":"translate","child":1,"by":{"x":10,"y":0,"z":0},"tag":"bore"},
            {"op":"difference","base":0,"tools":[2],"blend":0},
            {"op":"sphere","r":3,"tag":"core"},
            {"op":"union","children":[3,4],"blend":0}]}"#,
        );
        let (extents, unlocated) = tag_extents(&part, &bodies_of(&part));
        assert_eq!(unlocated, ["core"]);
        let bore = extents.iter().find(|e| e.tag == "bore").unwrap();
        for (got, want) in bore.min.iter().chain(&bore.max).zip([4.0, -6.0, -20.0, 16.0, 6.0, 20.0]) {
            assert!((got - want).abs() < 1e-6, "bore extent {:?}..{:?}", bore.min, bore.max);
        }
        let body = extents.iter().find(|e| e.tag == "body").unwrap();
        assert_eq!((body.min[0], body.max[0]), (-20.0, 20.0));
    }
}

