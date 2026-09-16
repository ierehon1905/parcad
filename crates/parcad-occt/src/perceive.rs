//! Questions asked of the exact solid: is there material at this point, what
//! does this line cross, where is the part thinnest, where is each tag.
//!
//! Every answer here is measured on the B-rep the kernel built — a point
//! against `BRepClass3d`, a line against the surfaces themselves, a wall as
//! the largest ball whose nearest boundary point is no nearer than its
//! radius, a tag against the faces its lineage says it owns. Nothing is read
//! off a mesh or a field, so a fillet is in what gets measured and a distance
//! is the distance, at a corner as on a face. The one place the tessellation
//! is used is to choose *where* the thickness sweep measures from: its nodes
//! lie near the surface and are projected onto it, and its triangles give the
//! middle of a face the mesher never puts a node in.

use crate::backend::{face_key, BuiltPart, FaceKey, NamedFaces};
use crate::protocol::{
    FaceSummary, Perceive, Perceived, PointResult, PointWhere, RayHitResult, RayLine, RayResult,
    TagBounds, ThicknessResult, ThicknessSample, ThicknessSpec, ThinKind,
};
use anyhow::{bail, Result};
use glam::{DVec2, DVec3};
use opencascade::mesh::Mesh;
use opencascade::primitives::{ClosePair, Compound, Crossing, EdgeWedge, NearestBoundary, PointState, RayCaster, Shape};
use std::collections::{BTreeMap, HashMap, HashSet};

/// How near a point must be to a face to count as on it, and the face-boundary
/// tolerance the ray intersector classifies hits with, in mm. A hit within this
/// of an edge is reported by both faces that share it, which the walk below
/// expects; smaller and a ray through an edge can be reported by neither.
const SURFACE_TOLERANCE_MM: f64 = 1e-4;

/// How many distinct thin spots to report, and how many edge readings after them.
const MAX_THIN_SPOTS: usize = 8;
const MAX_EDGE_SPOTS: usize = 3;

/// Faces that meet at this angle or steeper make an edge reading, not a feather:
/// the band thinner than a reading of `t` is at most `t / tan(60°)` ≈ 0.58 t wide.
const EDGE_MIN_WEDGE_DEG: f64 = 60.0;

/// Below this the two faces are parallel enough to be a wall rather than
/// material running out: a rod measured across itself, a floor under a pocket,
/// a moulded wall with draft on it.
const WALL_MAX_WEDGE_DEG: f64 = 5.0;

/// One finished body with the lookup every question needs: which tags each
/// face carries.
pub struct Body<'a> {
    pub name: Option<&'a str>,
    pub shape: &'a Shape,
    /// Per face, in the shape's own traversal order: its tags, nearest first.
    /// The nearest is the one authored nearest the node that produced the
    /// face — `bore` rather than the `part` that was unioned around it, and
    /// `right` rather than `left` on `left.mirror("x").tag("right")`.
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

/// Which tags each face of `shape` carries, nearest first: node order, which
/// is innermost first, except that a tag outranked on that face (see
/// [`NamedFaces::outranked_by`]) follows the tags that outrank it.
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
    for carried in &mut tags {
        let outranks = |tag: &String| {
            names
                .outranked_by
                .get(tag)
                .map_or(0, |by| carried.iter().filter(|t| by.contains(*t)).count())
        };
        let ranks: HashMap<String, usize> = carried.iter().map(|t| (t.clone(), outranks(t))).collect();
        carried.sort_by_key(|t| ranks[t]);
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
        .map(|t| thickness(bodies, t, (hi - lo).length()));

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

/// How far the exact surface may lie from the mesh the sweep plans on, in mm:
/// the mesher's linear deflection.
const DEFLECTION_MM: f64 = 0.01;

/// Edge samples per part diagonal for the feather search, at least eight per
/// edge: the angle between two faces changes slowly along their edge.
const EDGE_SAMPLES_PER_DIAGONAL: f64 = 400.0;

/// The inscribed-ball thickness at every sampled surface point: the diameter of
/// the largest ball inside the material that touches the surface there, which
/// is the thickness a mould or casting check means by the word — and then the
/// two places thin material can hide between samples, found from the exact
/// geometry rather than hoped for (docs/PERCEPTION.md §5, "What is certain").
///
/// The sweep measures from points over every triangle and the tessellation's
/// nodes, evaluated on the surface at the triangle's own parameters, at the
/// finest spacing the budget allows: every point of every face is within that
/// spacing of a sample on the same face. A point on a face's boundary beside a
/// convex edge is moved a step into its face, since the neighbouring face cuts
/// every ball tangent on the edge.
///
/// Then: every edge is read along its length, and one whose faces enclose
/// less than [`EDGE_MIN_WEDGE_DEG`] of material is a feather, reported at
/// zero on its seam. Every pair of faces that do not share an edge but come
/// within reach of each other — reach being the threshold or the thinnest
/// sampled wall, whichever is larger — has its least distance settled on the
/// surfaces and the ball measured where it is attained. And the thinnest
/// sampled walls are followed downhill on their faces.
fn thickness(bodies: &[Body], spec: &ThicknessSpec, diagonal: f64) -> ThicknessResult {
    let mut samples: Vec<ThicknessSample> = Vec::new();
    let mut discarded = 0usize;
    // No ball in the part is wider than the part.
    let limit = diagonal * 0.5 + 1.0;
    let meshes: Vec<Mesh> = bodies.iter().map(|b| b.shape.mesh()).collect();
    let max = spec.max_samples.max(1);
    // About a hundred samples across the part's diagonal at the finest.
    let finest = (diagonal / 96.0).max(1e-3);
    let spacing = plan_spacing(&meshes, max, finest);
    let starts = surface_samples(&meshes, spacing);

    let faces: Vec<FacesOf> = bodies.iter().map(FacesOf::new).collect();
    let mut nearest: Vec<NearestBoundary> = bodies.iter().map(|b| b.shape.nearest_boundary()).collect();
    // Neighbouring samples on one face read alike, so the last reading there
    // is the first radius tried.
    let mut last_radius: HashMap<(usize, usize), f64> = HashMap::new();
    // How far into its face a sample on a convex edge is moved. Beside a
    // right-angled edge a ball there reads 1.1 times the threshold, so the
    // edge itself is not counted, and any wall thinner than the threshold
    // still is.
    let step = spec.threshold_mm.map_or(0.5 * spacing, |t| 0.55 * t);
    for Start { body: b, face, uv, into, may_be_outside, .. } in starts {
        let Some((p, outward)) = nearest[b].evaluate(face, uv, may_be_outside) else {
            // A lattice point in the sliver between a curved boundary and its
            // chord is not on the face at all; anything else is a failure.
            if !may_be_outside {
                discarded += 1;
            }
            continue;
        };
        // A first ball much wider than the wall asks every face near it.
        let guess = last_radius.get(&(b, face)).copied().unwrap_or(finest);
        // A point on the face's boundary is measured where it is only beside
        // a concave edge; beside a convex one no ball fits. The probe that
        // tells them apart can be fooled where a point lies on the line of
        // another edge, and a point on a boundary's chord is up to the
        // deflection inside the face, so a ball no wider than that moves too.
        let mut reading = None;
        if into.is_none() || !on_convex_edge(&mut nearest[b], p, -outward) {
            reading = measure(&bodies[b], &faces[b], &mut nearest[b], p, -outward, face, guess, limit);
        }
        if let Some(into) = into.filter(|_| reading.as_ref().is_none_or(|s| s.thickness_mm < 4.0 * DEFLECTION_MM)) {
            let along = (into - outward * into.dot(outward)).normalize_or_zero();
            reading = match nearest[b].project(face, p + along * step) {
                Some((q, outward)) if along != DVec3::ZERO => {
                    measure(&bodies[b], &faces[b], &mut nearest[b], q, -outward, face, guess, limit)
                }
                _ => continue,
            };
        }
        match reading {
            Some(sample) => {
                last_radius.insert((b, face), (0.5 * sample.thickness_mm).max(1e-3));
                samples.push(sample);
            }
            None => discarded += 1,
        }
    }

    // Readings beside a feather run down to nothing and the seam search
    // reports those exactly, so the reach is the thinnest sampled wall.
    let sampled_min = samples
        .iter()
        .filter(|s| s.kind == ThinKind::Wall)
        .map(|s| s.thickness_mm)
        .fold(None, |m: Option<f64>, t| Some(m.map_or(t, |m| m.min(t))));
    let reach = spec.threshold_mm.unwrap_or(0.0).max(sampled_min.unwrap_or(0.0));
    // The thinnest sampled wall between each pair of faces, where a least
    // distance that runs along a line is known to be reachable by a ball.
    let mut sampled_walls: HashMap<(usize, usize, usize), (f64, DVec3, DVec3)> = HashMap::new();
    for s in samples.iter().filter(|s| s.kind == ThinKind::Wall) {
        let b = bodies.iter().position(|body| body.name.map(str::to_owned) == s.body).unwrap_or(0);
        let (f, g) = s.faces;
        let (key, at) = if f < g {
            ((b, f, g), (DVec3::from_array(s.at), DVec3::from_array(s.opposite)))
        } else {
            ((b, g, f), (DVec3::from_array(s.opposite), DVec3::from_array(s.at)))
        };
        let entry = sampled_walls.entry(key).or_insert((f64::INFINITY, at.0, at.1));
        if s.thickness_mm < entry.0 {
            *entry = (s.thickness_mm, at.0, at.1);
        }
    }
    let mut edges_checked = 0;
    let mut face_pairs_checked = 0;
    for (b, body) in bodies.iter().enumerate() {
        let edge_spacing = (diagonal / EDGE_SAMPLES_PER_DIAGONAL).max(1e-3);
        let (seams, edges) = feathers(body, &mut nearest[b], edge_spacing);
        edges_checked += edges;
        samples.extend(seams);
        if reach <= 0.0 {
            continue;
        }
        let pairs = nearest[b].close_pairs(reach);
        face_pairs_checked += pairs.len();
        // A least distance on a boundary is only near a wall, and stepping back
        // from it finds a reading rather than the minimum: worth it for what a
        // threshold asks about, and for the thinnest when nothing is asked.
        let boundaries_below = spec.threshold_mm.unwrap_or(reach);
        for pair in pairs.into_iter().filter(|p| p.inside || p.distance < boundaries_below) {
            let sampled = sampled_walls.get(&(b, pair.faces.0, pair.faces.1)).map(|w| (w.1, w.2));
            samples.extend(wall_between(body, &faces[b], &mut nearest[b], &pair, sampled, limit));
        }
    }

    // The thinnest walls sampled, each followed downhill on its own face: a
    // sample lands somewhere on a wall, and the wall is thinnest nearby.
    let mut walls: Vec<&ThicknessSample> = samples.iter().filter(|s| s.kind == ThinKind::Wall).collect();
    walls.sort_by(|a, b| a.thickness_mm.total_cmp(&b.thickness_mm));
    let mut seeds: Vec<ThicknessSample> = Vec::new();
    for w in walls {
        if seeds.len() == POLISHED_WALLS {
            break;
        }
        let apart = |s: &ThicknessSample| (DVec3::from_array(s.at) - DVec3::from_array(w.at)).length() > 2.5 * spacing;
        if seeds.iter().all(apart) {
            seeds.push(w.clone());
        }
    }
    for seed in seeds {
        let b = bodies.iter().position(|body| body.name.map(str::to_owned) == seed.body).unwrap_or(0);
        if let Some(better) = downhill(&bodies[b], &faces[b], &mut nearest[b], &seed, spacing, limit) {
            samples.push(better);
        }
    }

    samples.sort_by(|a, b| a.thickness_mm.total_cmp(&b.thickness_mm));
    let (edges, rest): (Vec<&ThicknessSample>, Vec<&ThicknessSample>) =
        samples.iter().partition(|s| s.kind == ThinKind::Edge);
    let below = |group: &[&ThicknessSample]| {
        spec.threshold_mm
            .map(|t| group.iter().filter(|s| s.thickness_mm <= t).count())
            .unwrap_or(0)
    };
    let mut thin_spots = match spec.threshold_mm {
        Some(t) => {
            let link = spacing * 2.5;
            let mut found = places(&rest, t, link);
            found.truncate(MAX_THIN_SPOTS);
            let mut at_edges = places(&edges, t, link);
            at_edges.truncate(MAX_EDGE_SPOTS);
            found.extend(at_edges);
            found
        }
        None => distinct(&rest, diagonal),
    };
    // The thinnest reading that is not an edge beside itself; an edge only when
    // that is all the part has.
    let mut min = rest.first().or(edges.first()).map(|s| (*s).clone());
    for spot in thin_spots.iter_mut().chain(min.iter_mut()) {
        let b = bodies.iter().position(|body| body.name.map(str::to_owned) == spot.body).unwrap_or(0);
        spot.surface = Some(faces[b].describe(spot.faces.0));
        spot.opposite_surface = Some(faces[b].describe(spot.faces.1));
    }
    ThicknessResult {
        samples: samples.len(),
        discarded,
        min,
        below_threshold: below(&rest),
        below_threshold_at_edges: below(&edges),
        thin_spots,
        spacing_mm: spacing,
        edges_checked,
        face_pairs_checked,
    }
}

/// How many of the thinnest sampled walls, a few sample spacings apart, are
/// followed downhill.
const POLISHED_WALLS: usize = 4;

/// A compass search for a thinner wall on `seed`'s face, from a quarter of the
/// sample spacing down to a micron: the thinnest reading it reaches, if it is
/// thinner, or nothing.
fn downhill(
    body: &Body,
    faces: &FacesOf,
    nearest: &mut NearestBoundary,
    seed: &ThicknessSample,
    spacing: f64,
    limit: f64,
) -> Option<ThicknessSample> {
    const MAX_MOVES: usize = 64;
    let face = seed.faces.0;
    let mut best = seed.clone();
    let mut step = 0.25 * spacing;
    let mut moved = false;
    for _ in 0..MAX_MOVES {
        if step < 1e-3 {
            break;
        }
        let at = DVec3::from_array(best.at);
        let inward = DVec3::from_array(best.inward);
        let t1 = inward.any_orthonormal_vector();
        let t2 = inward.cross(t1);
        let mut improved = false;
        for dir in [t1, -t1, t2, -t2] {
            let Some((q, outward)) = nearest.project(face, at + dir * step) else {
                continue;
            };
            if on_convex_edge(nearest, q, -outward) {
                continue;
            }
            let guess = 0.5 * best.thickness_mm;
            if let Some(s) = measure(body, faces, nearest, q, -outward, face, guess, limit) {
                if s.kind == ThinKind::Wall && s.thickness_mm < best.thickness_mm - 1e-9 {
                    best = s;
                    improved = true;
                    moved = true;
                    break;
                }
            }
        }
        if !improved {
            step *= 0.5;
        }
    }
    moved.then_some(best)
}

/// The ball tangent at `p`, as a reading named for its faces.
#[allow(clippy::too_many_arguments)]
fn measure(
    body: &Body,
    faces: &FacesOf,
    nearest: &mut NearestBoundary,
    p: DVec3,
    inward: DVec3,
    face: usize,
    guess: f64,
    limit: f64,
) -> Option<ThicknessSample> {
    let ball = inscribed(nearest, p, inward, guess, limit)?;
    // Smaller than the probe that finds edges, or than the mesh's deflection
    // against a face this one meets: the point is on an edge, and a feather
    // there is the seam search's to report.
    let (mut kind, wedge_deg) = faces.classify(face, ball.face, p, ball.centre, ball.contact);
    if ball.radius < 2.0 * EDGE_PROBE_MM || (ball.radius < 2.0 * DEFLECTION_MM && faces.meet(face, ball.face)) {
        kind = ThinKind::Edge;
    }
    Some(ThicknessSample {
        thickness_mm: 2.0 * ball.radius,
        at: p.to_array(),
        opposite: ball.contact.to_array(),
        inward: inward.to_array(),
        tags: body.tags_of(face),
        opposite_tags: body.tags_of(ball.face),
        body: body.name.map(str::to_owned),
        kind,
        wedge_deg,
        surface: None,
        opposite_surface: None,
        faces: (face, ball.face),
        samples: 1,
        extent_mm: None,
        span: None,
    })
}

/// Every edge whose two faces enclose less than [`EDGE_MIN_WEDGE_DEG`] of
/// material anywhere along it, as one zero-thickness reading on its seam at
/// the sharpest point, spanning the stretch that is that sharp; and how many
/// edges were read.
fn feathers(body: &Body, nearest: &mut NearestBoundary, spacing: f64) -> (Vec<ThicknessSample>, usize) {
    let wedges = nearest.edge_wedges(spacing);
    let mut by_edge: BTreeMap<usize, Vec<&EdgeWedge>> = BTreeMap::new();
    for w in &wedges {
        by_edge.entry(w.edge).or_default().push(w);
    }
    let mut seams = Vec::new();
    for along in by_edge.values() {
        let sharp: Vec<&&EdgeWedge> = along.iter().filter(|w| w.angle_deg < EDGE_MIN_WEDGE_DEG).collect();
        let Some(sharpest) = sharp.iter().min_by(|a, b| a.angle_deg.total_cmp(&b.angle_deg)) else {
            continue;
        };
        let lo = sharp.iter().fold(DVec3::splat(f64::INFINITY), |m, w| m.min(w.point));
        let hi = sharp.iter().fold(DVec3::splat(f64::NEG_INFINITY), |m, w| m.max(w.point));
        let (a, b) = sharpest.faces;
        seams.push(ThicknessSample {
            thickness_mm: 0.0,
            at: sharpest.point.to_array(),
            opposite: sharpest.point.to_array(),
            inward: sharpest.into.to_array(),
            tags: body.tags_of(a),
            opposite_tags: body.tags_of(b),
            body: body.name.map(str::to_owned),
            kind: ThinKind::Feather,
            wedge_deg: Some(sharpest.angle_deg),
            surface: None,
            opposite_surface: None,
            faces: (a, b),
            samples: 1,
            extent_mm: None,
            span: Some((lo.to_array(), hi.to_array())),
        });
    }
    (seams, by_edge.len())
}

/// The thinnest wall between two faces that do not share an edge, measured as
/// a ball where their least distance `d` is attained. When that is square to
/// both faces the ball there spans the whole distance and is the answer. A
/// least distance along a line or over a patch — a hole beside a flat side —
/// may have been settled at an end of it, on a face's boundary where no ball
/// fits, or where a third face cuts the line; so the distance is settled again
/// from where the sweep found this wall and from points stepped along the
/// face, which moves along the line rather than off it, and the ball is taken
/// there. A least distance only on a boundary is near a wall, not across one:
/// see [`wall_near_boundary`]. Nothing when the faces face each other across a
/// void or look the same way across a step.
fn wall_between(
    body: &Body,
    faces: &FacesOf,
    nearest: &mut NearestBoundary,
    pair: &ClosePair,
    sampled: Option<(DVec3, DVec3)>,
    limit: f64,
) -> Option<ThicknessSample> {
    let d = pair.distance;
    // Steps sized by the wall alone, so that what is found does not depend on
    // the threshold asked about.
    let scale = d.max(4.0 * DEFLECTION_MM);
    let guess = (0.5 * d).max(1e-4);
    let (a, c) = pair.faces;
    let (pa, pc) = pair.points;
    let ends = [(a, nearest.project(a, pa)), (c, nearest.project(c, pc))];
    // A wall has each face's material towards the other: two faces that both
    // look the same way across a step, or away from each other across a gap,
    // are not one.
    if d > 1e-6 {
        for ((_, end), other) in ends.iter().zip([pc, pa]) {
            if end.is_some_and(|(q, outward)| (other - q).dot(-outward) <= 0.0) {
                return None;
            }
        }
    }
    if !pair.inside {
        return ends.into_iter().find_map(|(face, end)| {
            let (q, outward) = end?;
            wall_near_boundary(body, faces, nearest, face, q, outward, scale, guess, limit)
        });
    }
    let spans = |s: &ThicknessSample, across: f64| {
        s.kind != ThinKind::Edge && s.thickness_mm >= across * (1.0 - 1e-4) - 1e-6
    };
    for (face, end) in ends {
        let Some((q, outward)) = end else {
            continue;
        };
        if !on_convex_edge(nearest, q, -outward) {
            if let Some(s) = measure(body, faces, nearest, q, -outward, face, guess, limit) {
                if spans(&s, d) {
                    return Some(s);
                }
            }
        }
        let across = if face == a { pc - pa } else { pa - pc };
        let towards = faces.centroid(face).map(|c| c - q);
        let stepped = steps_into(nearest, face, q, outward, towards, scale)
            .into_iter()
            .map(|(at, _)| if face == a { (at, at + across) } else { (at + across, at) });
        let seeds: Vec<(DVec3, DVec3)> = sampled.filter(|_| face == a).into_iter().chain(stepped).collect();
        for near in seeds {
            let Some(settled) = nearest
                .settle_pair(a, c, near)
                .filter(|s| s.inside && s.distance <= d * (1.0 + 1e-4) + 1e-6)
            else {
                continue;
            };
            let on_face = if face == a { settled.points.0 } else { settled.points.1 };
            let Some((m, n)) = nearest.project(face, on_face) else {
                continue;
            };
            if on_convex_edge(nearest, m, -n) {
                continue;
            }
            if let Some(s) = measure(body, faces, nearest, m, -n, face, guess, limit) {
                if spans(&s, settled.distance) {
                    return Some(s);
                }
            }
        }
    }
    None
}

/// Beside a least distance `q` that lies on face `face`'s boundary: the first
/// reading stepped into the face that is not an edge, followed back towards
/// `q` by halving for as long as it stays a wall, keeping the thinnest.
#[allow(clippy::too_many_arguments)]
fn wall_near_boundary(
    body: &Body,
    faces: &FacesOf,
    nearest: &mut NearestBoundary,
    face: usize,
    q: DVec3,
    outward: DVec3,
    scale: f64,
    guess: f64,
    limit: f64,
) -> Option<ThicknessSample> {
    const HALVINGS: usize = 6;
    let towards = faces.centroid(face).map(|c| c - q);
    for (at, n) in steps_into(nearest, face, q, outward, towards, scale) {
        if on_convex_edge(nearest, at, -n) {
            continue;
        }
        let Some(first) = measure(body, faces, nearest, at, -n, face, guess, limit) else {
            continue;
        };
        if first.kind == ThinKind::Edge {
            continue;
        }
        let (mut wall, mut edge) = (at, q);
        let mut best = first;
        for _ in 0..HALVINGS {
            let mid = 0.5 * (wall + edge);
            let Some((m, n)) = nearest.project(face, mid) else {
                edge = mid;
                continue;
            };
            match measure(body, faces, nearest, m, -n, face, guess, limit) {
                Some(t) if t.kind != ThinKind::Edge && !on_convex_edge(nearest, m, -n) => {
                    wall = mid;
                    if t.thickness_mm < best.thickness_mm {
                        best = t;
                    }
                }
                _ => edge = mid,
            }
        }
        return Some(best);
    }
    None
}

/// Points of the face `step`, `2 step`, `4 step` away from `q` in each tangent direction
/// that stays on the face — the one nearest `towards` first, which from a
/// face's boundary is the way into it.
fn steps_into(
    nearest: &mut NearestBoundary,
    face: usize,
    q: DVec3,
    outward: DVec3,
    towards: Option<DVec3>,
    scale: f64,
) -> Vec<(DVec3, DVec3)> {
    const DIRECTIONS: usize = 8;
    let mut out = Vec::new();
    let aim = towards.map(|t| t - outward * t.dot(outward)).filter(|t| t.length() > 1e-9);
    let t1 = aim.map_or_else(|| outward.any_orthonormal_vector(), DVec3::normalize);
    let t2 = outward.cross(t1);
    let probe = (1e-3 * scale).max(1e-5);
    // Straight ahead, then alternately either side of it.
    for k in [0, 1, 7, 2, 6, 3, 5, 4] {
        let angle = std::f64::consts::TAU * k as f64 / DIRECTIONS as f64;
        let dir = t1 * angle.cos() + t2 * angle.sin();
        if nearest.project(face, q + dir * probe).is_none() {
            continue;
        }
        for step in [0.55, 1.1, 2.2] {
            if let Some(hit) = nearest.project(face, q + dir * step * scale) {
                out.push(hit);
            }
        }
    }
    out
}

/// A surface point to measure from.
struct Start {
    body: usize,
    face: usize,
    /// Where on the face's surface, in its own parameters.
    uv: DVec2,
    /// The same point on the mesh, for how far the mesh lies off the surface.
    chord: DVec3,
    /// For a point on the face's boundary, a direction into the face.
    into: Option<DVec3>,
    /// Whether the point may lie outside the face: a lattice point in a
    /// triangle on the boundary, which a curved boundary can leave outside.
    may_be_outside: bool,
}

/// Radius of the ball that tells a convex edge from a concave one, mm.
const EDGE_PROBE_MM: f64 = 1e-4;

/// How far, as a share of a ball's radius, boundary may reach into it and the
/// ball still fit. It is what makes a crease between two faces under about a
/// quarter of a degree smooth, as a ruled loft's seams between sections are,
/// rather than an edge a ball cannot sit on.
const BALL_SLACK: f64 = 1e-5;

/// Whether a ball this small, tangent at `p`, already has boundary inside it:
/// a neighbouring face turning towards the material.
fn on_convex_edge(nearest: &mut NearestBoundary, p: DVec3, inward: DVec3) -> bool {
    nearest
        .nearest_within(p + inward * EDGE_PROBE_MM, EDGE_PROBE_MM * (1.0 - BALL_SLACK))
        .is_some_and(|hit| (hit.point - p).dot(inward) > 0.0)
}

fn point_segment_distance(x: DVec3, a: DVec3, b: DVec3) -> f64 {
    let ab = b - a;
    let t = if ab.length_squared() > 0.0 { ((x - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    (x - (a + ab * t)).length()
}

/// Points over triangle `corners` no more than `spacing` apart, its corners
/// left out: rows parallel to its longest side, so a sliver gets a row along
/// its middle rather than a lattice as wide as it is long. Each point is its
/// weights on the three corners and, for a point on a side, the corner that
/// side is opposite.
fn triangle_points(corners: [DVec3; 3], spacing: f64) -> Vec<([f64; 3], Option<usize>)> {
    let side = |k: usize| (corners[(k + 2) % 3] - corners[(k + 1) % 3]).length();
    let apex = (0..3).max_by(|&i, &j| side(i).total_cmp(&side(j))).unwrap_or(0);
    let (a, b) = ((apex + 1) % 3, (apex + 2) % 3);
    let base = side(apex);
    let area = 0.5 * (corners[b] - corners[a]).cross(corners[apex] - corners[a]).length();
    let height = if base > 0.0 { 2.0 * area / base } else { 0.0 };
    let rows = ((height / spacing).ceil() as usize).max(1);
    let mut out = Vec::new();
    for j in 0..rows {
        let t = j as f64 / rows as f64;
        let across = ((base * (1.0 - t) / spacing).ceil() as usize).max(1);
        for k in 0..=across {
            if j == 0 && (k == 0 || k == across) {
                continue;
            }
            let along = k as f64 / across as f64;
            let mut w = [0.0; 3];
            w[a] = (1.0 - t) * (1.0 - along);
            w[b] = (1.0 - t) * along;
            w[apex] = t;
            let on = if j == 0 {
                Some(apex)
            } else if k == 0 {
                Some(b)
            } else if k == across {
                Some(a)
            } else {
                None
            };
            out.push((w, on));
        }
    }
    out
}

/// A grid cell, hashed for [`CellSet`].
type Cell = (u32, [i64; 3]);

/// Sets and maps over the mesh's own keys — cells, node indices, triangle
/// sides — with a hash that costs a multiply rather than SipHash's rounds:
/// the keys are ours, and there are hundreds of thousands.
type FastHash = std::hash::BuildHasherDefault<CellHasher>;
type CellSet = HashSet<Cell, FastHash>;

#[derive(Default)]
struct CellHasher(u64);

impl std::hash::Hasher for CellHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(b as u64);
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
    fn write_i64(&mut self, n: i64) {
        self.write_u64(n as u64);
    }
    fn write_u32(&mut self, n: u32) {
        self.write_u64(n as u64);
    }
    fn write_usize(&mut self, n: usize) {
        self.write_u64(n as u64);
    }
}

fn cell_of(face: usize, p: DVec3, cell: f64) -> Cell {
    (face as u32, (p / cell).floor().to_array().map(|c| c as i64))
}

/// How many samples [`surface_samples`] takes at `spacing`, without taking them.
fn sample_count(meshes: &[Mesh], spacing: f64) -> usize {
    let cell = spacing / 3f64.sqrt();
    let mut total = 0;
    for mesh in meshes {
        let mut taken = CellSet::default();
        let mut node_seen = vec![false; mesh.vertices.len()];
        for run in &mesh.faces {
            let triangles = &mesh.indices[run.start * 3..(run.start + run.count) * 3];
            for tri in triangles.chunks_exact(3) {
                let [p, q, r] = [mesh.vertices[tri[0]], mesh.vertices[tri[1]], mesh.vertices[tri[2]]];
                taken.insert(cell_of(run.face, (p + q + r) / 3.0, cell));
                for (w, _) in triangle_points([p, q, r], spacing) {
                    taken.insert(cell_of(run.face, p * w[0] + q * w[1] + r * w[2], cell));
                }
                for &k in tri {
                    if !std::mem::replace(&mut node_seen[k], true) {
                        taken.insert(cell_of(run.face, mesh.vertices[k], cell));
                    }
                }
            }
        }
        total += taken.len();
    }
    total
}

/// A spacing no finer than `finest` whose samples fit in `max`, and within a
/// few percent of the finest that does: samples go as the inverse square of
/// the spacing, so a few corrected guesses land there.
fn plan_spacing(meshes: &[Mesh], max: usize, finest: f64) -> f64 {
    // Cells a spacing / √3 across cover a surface of area A about 3A / s²
    // times, which is where to start counting.
    let area: f64 = meshes
        .iter()
        .flat_map(|m| {
            m.indices.chunks_exact(3).map(|t| {
                let [p, q, r] = [m.vertices[t[0]], m.vertices[t[1]], m.vertices[t[2]]];
                0.5 * (q - p).cross(r - p).length()
            })
        })
        .sum();
    let mut spacing = finest.max(0.95 * (3.0 * area / max as f64).sqrt());
    for _ in 0..8 {
        let count = sample_count(meshes, spacing);
        if count <= max && (spacing <= finest || count * 10 >= max * 9) {
            return spacing;
        }
        let next = spacing * (count as f64 / max as f64).sqrt() * if count > max { 1.02 } else { 0.99 };
        spacing = next.max(finest);
    }
    // Still over after the corrections: coarsen until it fits.
    while sample_count(meshes, spacing) > max && spacing < 1e6 {
        spacing *= 1.25;
    }
    spacing
}

/// Points to measure from on every face, no two in one cell of a grid whose
/// cells are `spacing / √3` across, so every candidate is within `spacing` of
/// a sample on its own face. The candidates are every triangle's middle and
/// points over it at `spacing`, then the tessellation's nodes. Each point carries its
/// surface parameters, so it is evaluated on the face rather than projected.
fn surface_samples(meshes: &[Mesh], spacing: f64) -> Vec<Start> {
    let cell = spacing / 3f64.sqrt();
    let mut starts: Vec<Start> = Vec::new();
    for (b, mesh) in meshes.iter().enumerate() {
        let at = |i: usize| (mesh.vertices[i], mesh.face_uvs[i]);
        for run in &mesh.faces {
            let triangles = &mesh.indices[run.start * 3..(run.start + run.count) * 3];
            // A triangle side used once is on the face's boundary.
            let mut sides: HashMap<(usize, usize), usize, FastHash> = HashMap::default();
            for tri in triangles.chunks_exact(3) {
                for (i, j) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                    *sides.entry((i.min(j), i.max(j))).or_default() += 1;
                }
            }
            let on_rim = |i: usize, j: usize| sides.get(&(i.min(j), i.max(j))) == Some(&1);
            // Off the boundary first, then on it, then the nodes: the first
            // candidate in a cell is the one measured.
            let mut inner: Vec<Start> = Vec::new();
            let mut boundary: Vec<Start> = Vec::new();
            let start = |chord: DVec3, uv: DVec2, into: Option<DVec3>, may_be_outside: bool| Start {
                body: b,
                face: run.face,
                uv,
                chord,
                into,
                may_be_outside,
            };
            for tri in triangles.chunks_exact(3) {
                let [(p, up), (q, uq), (r, ur)] = [at(tri[0]), at(tri[1]), at(tri[2])];
                let middle = (p + q + r) / 3.0;
                // Per side, opposite corners p, q, r in turn.
                let rim = [on_rim(tri[1], tri[2]), on_rim(tri[0], tri[2]), on_rim(tri[0], tri[1])];
                // Only a point this near a boundary side can be off the face:
                // the boundary is within the deflection of its chord.
                let off_face = |x: DVec3| {
                    [(q, r), (p, r), (p, q)]
                        .iter()
                        .zip(rim)
                        .any(|(&(a, c), on)| on && point_segment_distance(x, a, c) <= 2.0 * DEFLECTION_MM)
                };
                inner.push(start(middle, (up + uq + ur) / 3.0, None, off_face(middle)));
                for (w, on) in triangle_points([p, q, r], spacing) {
                    let chord = p * w[0] + q * w[1] + r * w[2];
                    let uv = up * w[0] + uq * w[1] + ur * w[2];
                    match on {
                        None => inner.push(start(chord, uv, None, off_face(chord))),
                        Some(side) => boundary.push(start(chord, uv, rim[side].then(|| middle - chord), off_face(chord))),
                    }
                }
            }
            // Into the face from a boundary node: towards the middles of the
            // triangles it is a corner of.
            let mut inwards: HashMap<usize, DVec3, FastHash> = HashMap::default();
            for tri in triangles.chunks_exact(3) {
                let middle = (mesh.vertices[tri[0]] + mesh.vertices[tri[1]] + mesh.vertices[tri[2]]) / 3.0;
                for (k, &i) in tri.iter().enumerate() {
                    if on_rim(i, tri[(k + 1) % 3]) || on_rim(i, tri[(k + 2) % 3]) {
                        *inwards.entry(i).or_default() += (middle - mesh.vertices[i]).normalize_or_zero();
                    }
                }
            }
            let mut seen: HashSet<usize, FastHash> = HashSet::default();
            let nodes = triangles
                .iter()
                .filter(|&&i| seen.insert(i))
                .map(|&i| start(at(i).0, at(i).1, inwards.get(&i).copied(), false));
            let mut taken = CellSet::default();
            for candidate in inner.into_iter().chain(boundary).chain(nodes.collect::<Vec<_>>()) {
                if taken.insert(cell_of(run.face, candidate.chord, cell)) {
                    starts.push(candidate);
                }
            }
        }
    }
    starts
}

/// The largest ball inside the material that touches the surface at a point.
struct Ball {
    radius: f64,
    centre: DVec3,
    /// Where else the ball touches the boundary, and a face that point is on.
    contact: DVec3,
    face: usize,
}

/// Radii tried per ball before settling for the largest that fitted. Measured:
/// a median of 3 to 7 and a maximum of 16 over the corpus and a 180 mm lamp.
const MAX_BALL_TESTS: usize = 80;

/// The largest ball tangent at `p` on the material's side that has no boundary
/// point inside it.
///
/// Balls tangent at one point on one side are nested, so the radii that fit
/// are an interval `[0, r*]`. A ball that does not fit has a boundary point
/// `q` strictly inside it, and the ball through `q` tangent at `p` is smaller
/// yet never smaller than `r*` — the shrinking-ball step (Ma, Bae, Choi & Rhee,
/// 2012). A radius reached that way which fits is therefore `r*` itself. The
/// guess may lie either side: one that fits is doubled until one does not.
fn inscribed(nearest: &mut NearestBoundary, p: DVec3, inward: DVec3, guess: f64, limit: f64) -> Option<Ball> {
    let slack = |r: f64| (r * BALL_SLACK).max(1e-7);
    // `lo` fits; `hi` is a shrink result, never below r*, not yet tested.
    let mut lo = 0.0f64;
    let mut hi = f64::INFINITY;
    let mut contact: Option<(DVec3, usize)> = None;
    let mut r = guess.clamp(1e-4, limit);
    for _ in 0..MAX_BALL_TESTS {
        // A boundary point on or behind the tangent plane is `p` itself to
        // rounding: no ball tangent there can contain it.
        let hit = nearest
            .nearest_within(p + inward * r, r - slack(r))
            .filter(|hit| (hit.point - p).dot(inward) > 0.0);
        match hit {
            None => {
                lo = r;
                if contact.is_none() {
                    if r >= limit {
                        return None;
                    }
                    r = (r * 2.0).min(limit);
                    continue;
                }
            }
            Some(hit) => {
                let d = hit.point - p;
                hi = (d.length_squared() / (2.0 * d.dot(inward))).min(r);
                contact = Some((hit.point, hit.face));
                // Converging slowly — a ball rolling into a round it
                // osculates — so halve the bracket as well.
                if hi > 0.9 * r && hi - lo > slack(hi) {
                    r = 0.5 * (lo + hi);
                    continue;
                }
            }
        }
        let (q, face) = contact?;
        if hi - lo <= slack(hi) {
            let radius = if lo > 0.0 { lo } else { hi };
            return Some(Ball { radius, centre: p + inward * radius, contact: q, face });
        }
        r = hi;
    }
    let (q, face) = contact?;
    (lo > 0.0).then(|| Ball { radius: lo, centre: p + inward * lo, contact: q, face })
}

/// One body's faces by traversal index, for whether two of them meet and a
/// name for one that no tag names.
struct FacesOf {
    summaries: Vec<FaceSummary>,
}

impl FacesOf {
    fn new(body: &Body) -> Self {
        Self {
            summaries: describe_faces(body.shape),
        }
    }

    fn meet(&self, a: usize, b: usize) -> bool {
        a == b || self.summaries.get(a).is_some_and(|s| s.adjacent.iter().any(|&f| f as usize == b))
    }

    fn centroid(&self, face: usize) -> Option<DVec3> {
        self.summaries.get(face).map(|s| DVec3::from_array(s.centroid))
    }

    /// Feather, wall or edge, from where the ball touches: 180° less the angle
    /// between its two contacts seen from its centre, which is the angle the
    /// two surfaces enclose there — parallel walls 0°, a box corner 90°. A ball
    /// wedged into a corner is an edge reading whether or not the corner's
    /// faces share an edge, since a round between them is still a corner; a
    /// feather is faces that meet, or one face wrapping onto itself.
    fn classify(&self, face: usize, opposite: usize, p: DVec3, centre: DVec3, q: DVec3) -> (ThinKind, Option<f64>) {
        let (a, b) = (p - centre, q - centre);
        if a.length_squared() < 1e-24 || b.length_squared() < 1e-24 {
            return (ThinKind::Wall, None);
        }
        let wedge = 180.0 - a.normalize().dot(b.normalize()).clamp(-1.0, 1.0).acos().to_degrees();
        let meet = face == opposite
            || self
                .summaries
                .get(face)
                .is_some_and(|s| s.adjacent.iter().any(|&f| f as usize == opposite));
        let kind = if wedge >= EDGE_MIN_WEDGE_DEG {
            ThinKind::Edge
        } else if meet && wedge > WALL_MAX_WEDGE_DEG {
            ThinKind::Feather
        } else {
            ThinKind::Wall
        };
        (kind, Some(wedge))
    }

    fn describe(&self, face: usize) -> String {
        let Some(s) = self.summaries.get(face) else {
            return "a face".to_owned();
        };
        let [x, y, z] = s.centroid;
        let near = format!("near ({x:.1}, {y:.1}, {z:.1})");
        let along = s.surface.direction.map(axis_name);
        match (s.surface.kind.as_str(), along, s.surface.radius) {
            ("plane", Some(d), _) => format!("plane facing {d} {near}"),
            (kind, Some(d), Some(r)) => format!("{kind} r {r:.2} along {d} {near}"),
            (kind, _, Some(r)) => format!("{kind} r {r:.2} {near}"),
            (kind, _, _) => format!("{kind} {near}"),
        }
    }
}

/// `+z` for a direction on an axis, the rounded vector otherwise.
fn axis_name(d: [f64; 3]) -> String {
    for (i, name) in ["x", "y", "z"].iter().enumerate() {
        if (d[i].abs() - 1.0).abs() < 1e-6 {
            return format!("{}{name}", if d[i] > 0.0 { "+" } else { "-" });
        }
    }
    format!("({:.2}, {:.2}, {:.2})", d[0], d[1], d[2])
}

/// Every sample at or below `threshold`, grouped with its neighbours within
/// `link` in the same body and of the same kind: one entry per group, its
/// thinnest sample carrying the group's size and extent, thinnest group first.
fn places(sorted: &[&ThicknessSample], threshold: f64, link: f64) -> Vec<ThicknessSample> {
    let thin: Vec<&ThicknessSample> = sorted.iter().copied().take_while(|s| s.thickness_mm <= threshold).collect();
    let cell = |p: [f64; 3]| -> [i64; 3] { p.map(|c| (c / link).floor() as i64) };
    let mut grid: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    for (i, s) in thin.iter().enumerate() {
        grid.entry(cell(s.at)).or_default().push(i);
    }

    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut parent: Vec<usize> = (0..thin.len()).collect();
    for (i, s) in thin.iter().enumerate() {
        let [cx, cy, cz] = cell(s.at);
        for near in (-1..=1).flat_map(|dx| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| [cx + dx, cy + dy, cz + dz]))) {
            let Some(indices) = grid.get(&near) else { continue };
            for &j in indices {
                let other = thin[j];
                if j <= i || other.body != s.body || other.kind != s.kind {
                    continue;
                }
                if (DVec3::from_array(other.at) - DVec3::from_array(s.at)).length() <= link {
                    let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                    // Samples are sorted thinnest first, so the lower index stays the root.
                    parent[a.max(b)] = a.min(b);
                }
            }
        }
    }

    // A feather or an edge runs along the seam of two faces, and is one place
    // however far apart the sweep happened to sample it.
    let mut first_on_seam: HashMap<(Option<&str>, ThinKind, usize, usize), usize> = HashMap::new();
    for (i, s) in thin.iter().enumerate() {
        if s.kind == ThinKind::Wall {
            continue;
        }
        let (a, b) = s.faces;
        let seam = (s.body.as_deref(), s.kind, a.min(b), a.max(b));
        let first = *first_on_seam.entry(seam).or_insert(i);
        let (ra, rb) = (root(&mut parent, first), root(&mut parent, i));
        parent[ra.max(rb)] = ra.min(rb);
    }

    let mut groups: Vec<(usize, usize, DVec3, DVec3)> = Vec::new();
    let mut group_of_root: HashMap<usize, usize> = HashMap::new();
    for (i, s) in thin.iter().enumerate() {
        let r = root(&mut parent, i);
        let p = DVec3::from_array(s.at);
        let (lo, hi) = s
            .span
            .map_or((p, p), |(lo, hi)| (DVec3::from_array(lo).min(p), DVec3::from_array(hi).max(p)));
        let g = *group_of_root.entry(r).or_insert_with(|| {
            groups.push((r, 0, lo, hi));
            groups.len() - 1
        });
        let group = &mut groups[g];
        group.1 += 1;
        group.2 = group.2.min(lo);
        group.3 = group.3.max(hi);
    }
    groups
        .into_iter()
        .map(|(r, count, lo, hi)| {
            let mut place = thin[r].clone();
            place.samples = count;
            place.extent_mm = Some((hi - lo).to_array());
            place
        })
        .collect()
}

/// The worst samples with near-duplicates suppressed, so the list names
/// distinct features rather than a hundred neighbouring points of one.
fn distinct(sorted: &[&ThicknessSample], diagonal: f64) -> Vec<ThicknessSample> {
    let apart = diagonal * 0.05;
    let mut kept: Vec<ThicknessSample> = Vec::new();
    for s in sorted {
        if kept.len() >= MAX_THIN_SPOTS {
            break;
        }
        let near = kept.iter().any(|k| {
            (DVec3::from_array(k.at) - DVec3::from_array(s.at)).length() < apart
        });
        if !near {
            kept.push((*s).clone());
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

    fn thickness_of(part: &BuiltPart, threshold_mm: Option<f64>) -> ThicknessResult {
        ask(
            part,
            Perceive {
                thickness: Some(ThicknessSpec { max_samples: 20000, threshold_mm }),
                ..Default::default()
            },
        )
        .thickness
        .expect("a thickness sweep was asked for")
    }

    /// A cut whose floor meets the plate's top face at 15° leaves material
    /// tapering to nothing along that seam — the shape that shipped a 0.013 mm
    /// sliver under a phone slot. It is a feather, it is what `thinnest` names,
    /// and the angle is the one the two planes enclose.
    #[test]
    fn a_cut_that_grazes_a_face_reads_as_a_feather_at_the_angle_the_two_faces_enclose() {
        // Two cuts converging on one line, as a cable channel converged on a
        // leaning slot floor: everything above a plane through the origin
        // turned 15° about X goes, and so does everything below z = 0. What is
        // left runs out to nothing along y = 0.
        let part = built(
            r#"{"units":"mm","root":6,"nodes":[
            {"op":"cuboid","size":{"x":40,"y":40,"z":10},"tag":"plate"},
            {"op":"cuboid","size":{"x":80,"y":80,"z":20},"tag":"ramp"},
            {"op":"rotate","child":1,"axis":{"x":1,"y":0,"z":0},"degrees":15},
            {"op":"translate","child":2,"by":{"x":0,"y":-2.5882,"z":9.6593}},
            {"op":"cuboid","size":{"x":80,"y":80,"z":20},"tag":"floor"},
            {"op":"translate","child":4,"by":{"x":0,"y":0,"z":-10}},
            {"op":"difference","base":0,"tools":[3,5],"blend":0}]}"#,
        );
        let report = thickness_of(&part, Some(1.2));
        let min = report.min.clone().expect("the taper is measurable");
        assert_eq!(min.kind, ThinKind::Feather, "{min:?}");
        // Read off the edge itself: it runs to nothing, on the line y = 0 at
        // z = 0 where the two cuts meet.
        assert_eq!(min.thickness_mm, 0.0, "{min:?}");
        assert!(min.at[1].abs() < 1e-3 && min.at[2].abs() < 1e-3, "{min:?}");
        let wedge = min.wedge_deg.expect("a feather knows its angle");
        assert!((wedge - 15.0).abs() < 0.01, "wedge {wedge}, expected 15");
        // The seam is one place, however far apart the sweep sampled it, and it
        // is named by the tags of both faces.
        let feathers: Vec<_> = report
            .thin_spots
            .iter()
            .filter(|s| s.kind == ThinKind::Feather)
            .collect();
        assert_eq!(feathers.len(), 1, "{:?}", report.thin_spots);
        assert!(feathers[0].samples > 1, "{:?}", feathers[0]);
        let mut named = [
            feathers[0].tags.first().map(String::as_str).unwrap_or(""),
            feathers[0].opposite_tags.first().map(String::as_str).unwrap_or(""),
        ];
        named.sort();
        assert_eq!(named, ["floor", "ramp"], "{:?}", feathers[0]);
    }

    /// A cone whose side meets its base at 75° is thin beside that rim and
    /// nowhere else. Every sharp edge is, so those readings are counted apart
    /// and never become the part's thinnest wall.
    #[test]
    fn a_sharp_rim_is_counted_as_an_edge_rather_than_as_thin_material() {
        let part = built(
            r#"{"units":"mm","root":0,"nodes":[
            {"op":"revolve","profile":[[0,-14.93],[10,-14.93],[2,14.93],[0,14.93]],"tag":"cone"}]}"#,
        );
        let report = thickness_of(&part, Some(1.0));
        assert_eq!(report.below_threshold, 0, "{:?}", report.thin_spots);
        assert!(report.below_threshold_at_edges > 0, "{report:?}");
        let edge = report
            .thin_spots
            .iter()
            .find(|s| s.kind == ThinKind::Edge)
            .expect("the rim is listed, after anything that is really thin");
        let wedge = edge.wedge_deg.expect("an edge knows its angle");
        assert!((wedge - 75.0).abs() < 2.0, "wedge {wedge}, expected about 75");
        let min = report.min.expect("the cone has material to measure");
        assert_ne!(min.kind, ThinKind::Edge, "{min:?}");
    }

    /// A face with no tag is still named, by what it is and where.
    #[test]
    fn an_untagged_face_is_named_by_its_own_geometry() {
        let part = built(PLATE);
        let report = thickness_of(&part, None);
        let spot = report.thin_spots.first().expect("a plate has walls");
        let surface = spot.surface.clone().unwrap_or_default();
        assert!(
            surface.starts_with("plane facing") || surface.starts_with("cylinder r"),
            "{surface}"
        );
    }

    /// A 30 × 30 × 8 plate with its top edges rounded at r = 2. A ray up from
    /// the underside at x = 14 leaves through the round at z = 2 + √3, so the
    /// material straight up there is 4 + 2 + √3 = 7.732 mm — and a ray sweep
    /// reported walls down to 7.08 from such slanting exits. The plate is 8
    /// thick: the largest ball that fits touches top and bottom, and under the
    /// round it is the corner, not the wall, that stops a ball growing. On the
    /// round itself a ball reads 2r = 4 and is wedged, so it is an edge.
    #[test]
    fn a_plate_with_rounded_edges_is_as_thick_as_the_plate() {
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
                thickness: Some(ThicknessSpec { max_samples: 6000, threshold_mm: Some(7.9) }),
                ..Default::default()
            },
        );
        assert!((answer.rays[0].first_solid_mm.unwrap() - 8.0).abs() < 1e-6);
        let under = answer.rays[1].first_solid_mm.unwrap();
        let expected = 6.0 + 3.0f64.sqrt();
        assert!((under - expected).abs() < 1e-6, "under the fillet: {under}, expected {expected}");

        let thickness = answer.thickness.unwrap();
        let min = thickness.min.clone().unwrap();
        assert_eq!(min.kind, ThinKind::Wall, "{min:?}");
        assert!((min.thickness_mm - 8.0).abs() < 1e-6, "{min:?}");
        assert_eq!(thickness.below_threshold, 0, "{:?}", thickness.thin_spots);
        assert!(thickness.below_threshold_at_edges > 0, "{thickness:?}");
        assert!(thickness.samples > 500, "{}", thickness.samples);

        // Halfway round the +Y edge's round, whose axis is at y = 13, z = 2.
        let bodies = bodies_of(&part);
        let mut nearest = bodies[0].shape.nearest_boundary();
        let out = DVec3::new(0.0, 1.0, 1.0).normalize();
        let p = DVec3::new(0.0, 13.0, 2.0) + out * 2.0;
        let ball = inscribed(&mut nearest, p, -out, 3.0, 100.0).expect("a ball fits on the round");
        assert!((ball.radius - 2.0).abs() < 1e-4, "radius {}", ball.radius);
        let faces = FacesOf::new(&bodies[0]);
        let (kind, _) = faces.classify(ball.face, ball.face, p, ball.centre, ball.contact);
        assert_eq!(kind, ThinKind::Edge);
    }

    /// A slab 2 thick between its faces, turned 30° about Y: straight down it
    /// is 2 / cos 30° of material, and the wall is 2.
    #[test]
    fn a_slanted_wall_is_measured_across_its_faces_not_along_a_line() {
        let part = built(
            r#"{"units":"mm","root":1,"nodes":[
            {"op":"cuboid","size":{"x":40,"y":30,"z":2},"tag":"slab"},
            {"op":"rotate","child":0,"axis":{"x":0,"y":1,"z":0},"degrees":30}]}"#,
        );
        let mut spec = ray([0.0, 0.0, 50.0], [0.0, 0.0, -1.0]);
        spec.thickness = Some(ThicknessSpec { max_samples: 6000, threshold_mm: None });
        let answer = ask(&part, spec);
        let down = answer.rays[0].first_solid_mm.unwrap();
        assert!((down - 2.0 / 30f64.to_radians().cos()).abs() < 1e-6, "{down}");
        let min = answer.thickness.unwrap().min.unwrap();
        assert!((min.thickness_mm - 2.0).abs() < 1e-6, "{min:?}");
        assert!(min.wedge_deg.unwrap() < 1e-3, "{min:?}");
        // The two contacts face each other across the wall.
        let across = DVec3::from_array(min.opposite) - DVec3::from_array(min.at);
        assert!(across.normalize().dot(DVec3::from_array(min.inward)) > 1.0 - 1e-9, "{min:?}");
    }

    /// A Ø20 tube with its Ø16 bore 1 mm off axis: the wall is 1 mm where the
    /// circles are nearest, between the tube and the bore.
    #[test]
    fn an_eccentric_bore_leaves_the_wall_thinnest_where_the_circles_are_nearest() {
        let part = built(
            r#"{"units":"mm","root":3,"nodes":[
            {"op":"cylinder","r":10,"h":30,"tag":"tube"},
            {"op":"cylinder","r":8,"h":40},
            {"op":"translate","child":1,"by":{"x":1,"y":0,"z":0},"tag":"bore"},
            {"op":"difference","base":0,"tools":[2],"blend":0}]}"#,
        );
        let report = thickness_of(&part, Some(1.2));
        let min = report.min.expect("a tube has a wall");
        assert_eq!(min.kind, ThinKind::Wall, "{min:?}");
        // Sampled: exact at its own point, which is within a sample spacing of
        // the nearest pair, where the wall grows as 1 + 1 - cos θ.
        assert!(min.thickness_mm >= 1.0 - 1e-6 && min.thickness_mm < 1.001, "{min:?}");
        assert!(min.at[0] > 8.9 && min.at[1].abs() < 0.5, "{min:?}");
        let mut between = [min.tags[0].as_str(), min.opposite_tags[0].as_str()];
        between.sort();
        assert_eq!(between, ["bore", "tube"]);
        for spot in report.thin_spots.iter().filter(|s| s.kind == ThinKind::Wall) {
            assert!(spot.at[0] > 8.0, "only the +X side is under 1.2: {spot:?}");
        }
    }

    /// A ball fits nowhere on an edge, so no sample may sit on one: a box's
    /// sweep reads its half-thickness walls and edges beside them, never zero.
    #[test]
    fn no_sample_sits_on_an_edge() {
        let part = built(PLATE);
        let report = thickness_of(&part, Some(1.0));
        assert!(report.samples > 1000, "{report:?}");
        let min = report.min.clone().unwrap();
        assert!((min.thickness_mm - 6.0).abs() < 1e-6, "{min:?}");
        assert_eq!(report.discarded, 0, "{report:?}");
        for spot in &report.thin_spots {
            assert!(spot.thickness_mm > 1e-3, "{spot:?}");
        }
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
        // On the port's -X generator, which the face-pair search settles on: a
        // sample y off it reads 5 + y²/15, where a ball tangent to the plane
        // meets the Ø10 port, and the gallery cuts the generator below z = 4,
        // so the ball is taken where it fits above.
        assert!((min.thickness_mm - 5.0).abs() < 1e-6, "{min:?}");
        assert!(min.at[1].abs() < 1e-6, "{min:?}");
        let mut between = [min.tags[0].as_str(), min.opposite_tags[0].as_str()];
        between.sort();
        assert_eq!(between, ["block", "port"]);
        assert!(t.below_threshold > 0);
    }

    /// The control box's plate defect in closed form: a Ø2 hole and a Ø6
    /// recess 1 mm deep, 4.25 apart, leave 0.25 mm between them. Found and
    /// measured exactly at any sample count, one included.
    #[test]
    fn a_thin_wall_between_two_features_is_found_whatever_the_sample_count() {
        let part = built(
            r#"{"units":"mm","root":6,"nodes":[
            {"op":"cuboid","size":{"x":20,"y":20,"z":2}},
            {"op":"translate","child":0,"by":{"x":0,"y":0,"z":1},"tag":"plate"},
            {"op":"cylinder","r":1,"h":4},
            {"op":"translate","child":2,"by":{"x":0,"y":0,"z":1},"tag":"hole"},
            {"op":"cylinder","r":3,"h":2},
            {"op":"translate","child":4,"by":{"x":4.25,"y":0,"z":0},"tag":"recess"},
            {"op":"difference","base":1,"tools":[3,5],"blend":0}]}"#,
        );
        for max_samples in [1, 200, 20000] {
            let report = ask(
                &part,
                Perceive {
                    thickness: Some(ThicknessSpec { max_samples, threshold_mm: Some(1.2) }),
                    ..Default::default()
                },
            )
            .thickness
            .unwrap();
            let min = report.min.clone().expect("the plate has walls");
            assert!((min.thickness_mm - 0.25).abs() < 1e-6, "{max_samples}: {min:?}");
            assert_eq!(min.kind, ThinKind::Wall, "{min:?}");
            let mut between = [min.tags[0].as_str(), min.opposite_tags[0].as_str()];
            between.sort();
            assert_eq!(between, ["hole", "recess"]);
            assert!(report.below_threshold > 0);
            assert!(report.face_pairs_checked > 0 && report.edges_checked > 0, "{report:?}");
        }
    }

    /// The control box's grille through its screw boss, in closed form: where
    /// circles of radius 3.5 and 1.5, 2√2 apart, cross, the material between
    /// them closes at acos((3.5² + 1.5² − 8) / (2 · 3.5 · 1.5)).
    #[test]
    fn a_hole_through_a_boss_side_is_a_feather_at_the_angle_the_circles_cross() {
        let part = built(
            r#"{"units":"mm","root":3,"nodes":[
            {"op":"cylinder","r":3.5,"h":10,"tag":"boss"},
            {"op":"cylinder","r":1.5,"h":20},
            {"op":"translate","child":1,"by":{"x":2,"y":2,"z":0},"tag":"hole"},
            {"op":"difference","base":0,"tools":[2],"blend":0}]}"#,
        );
        let report = thickness_of(&part, Some(1.2));
        let min = report.min.expect("the boss has material");
        assert_eq!((min.kind, min.thickness_mm), (ThinKind::Feather, 0.0), "{min:?}");
        let expected = ((3.5f64 * 3.5 + 1.5 * 1.5 - 8.0) / (2.0 * 3.5 * 1.5)).acos().to_degrees();
        let wedge = min.wedge_deg.unwrap();
        assert!((wedge - expected).abs() < 1e-3, "wedge {wedge}, expected {expected}");
        // On a crossing line: 3.5 from the boss's axis and 1.5 from the hole's.
        let at = DVec3::from_array(min.at);
        assert!((at.truncate().length() - 3.5).abs() < 1e-6, "{min:?}");
        assert!(((at.truncate() - glam::DVec2::new(2.0, 2.0)).length() - 1.5).abs() < 1e-6, "{min:?}");
    }

    /// A box with a pocket: its own edges and the pocket's rim are convex
    /// right angles, the pocket's floor and inside corners concave ones, and
    /// the rule for which side of an edge a face lies on never needed the
    /// classifier's correction.
    #[test]
    fn every_edge_reads_the_angle_its_material_encloses() {
        let part = built(
            r#"{"units":"mm","root":3,"nodes":[
            {"op":"cuboid","size":{"x":20,"y":20,"z":10}},
            {"op":"cuboid","size":{"x":10,"y":10,"z":10}},
            {"op":"translate","child":1,"by":{"x":0,"y":0,"z":5}},
            {"op":"difference","base":0,"tools":[2],"blend":0}]}"#,
        );
        let bodies = bodies_of(&part);
        let wedges = bodies[0].shape.nearest_boundary().edge_wedges(0.5);
        let mut by_edge: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
        for w in &wedges {
            assert_eq!(w.corrected, 0, "{w:?}");
            let e = by_edge.entry(w.edge).or_insert((f64::INFINITY, f64::NEG_INFINITY));
            e.0 = e.0.min(w.angle_deg);
            e.1 = e.1.max(w.angle_deg);
        }
        assert_eq!(by_edge.len(), 24, "{by_edge:?}");
        let convex = by_edge.values().filter(|(lo, hi)| (lo - 90.0).abs() < 1e-9 && (hi - 90.0).abs() < 1e-9).count();
        let concave = by_edge.values().filter(|(lo, hi)| (lo - 270.0).abs() < 1e-9 && (hi - 270.0).abs() < 1e-9).count();
        assert_eq!((convex, concave), (16, 8), "{by_edge:?}");
        let report = thickness_of(&part, Some(1.0));
        assert!(report.thin_spots.iter().all(|s| s.kind != ThinKind::Feather), "{:?}", report.thin_spots);
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

    /// A mirrored copy is named for itself, and a feature inside the
    /// original is still named for the feature on the original.
    #[test]
    fn a_tagged_copy_outranks_the_tags_of_what_it_copied() {
        let part = built(
            r#"{"units":"mm","root":5,"nodes":[
            {"op":"cuboid","size":{"x":10,"y":10,"z":10}},
            {"op":"cylinder","r":2,"h":20,"tag":"bore"},
            {"op":"difference","base":0,"tools":[1],"blend":0,"tag":"left"},
            {"op":"translate","child":2,"by":{"x":-10,"y":0,"z":0}},
            {"op":"mirror","child":3,"normal":{"x":1,"y":0,"z":0},"tag":"right"},
            {"op":"bodies","bodies":[{"name":"left","child":3},{"name":"right","child":4}]}]}"#,
        );
        let bodies = bodies_of(&part);
        let owners = |body: &Body| -> Vec<(String, String)> {
            describe_faces(body.shape)
                .iter()
                .zip(&body.face_tags)
                .map(|(f, tags)| (f.surface.kind.clone(), tags.first().cloned().unwrap_or_default()))
                .collect()
        };
        let left = owners(&bodies[0]);
        let right = owners(&bodies[1]);
        assert_eq!(left.len(), 7, "{left:?}");
        assert_eq!(right.len(), 7, "{right:?}");
        for (kind, owner) in &left {
            assert_eq!(owner, if kind == "cylinder" { "bore" } else { "left" }, "{left:?}");
        }
        assert!(right.iter().all(|(_, owner)| owner == "right"), "{right:?}");
        // Every name is still carried; only the order moved.
        assert!(bodies[1].face_tags.iter().all(|t| t.contains(&"left".to_string())));
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


