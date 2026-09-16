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
use glam::DVec3;
use opencascade::primitives::{Compound, Crossing, NearestBoundary, PointState, RayCaster, Shape};
use std::collections::{HashMap, HashSet};

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
    /// A surface body: faces with no inside.
    pub surface: bool,
}

impl<'a> Body<'a> {
    pub fn new(name: Option<&'a str>, shape: &'a Shape, names: &NamedFaces) -> Self {
        Self {
            name,
            shape,
            face_tags: face_tags(shape, names),
            boundary: Compound::from_shapes(shape.faces().map(Shape::from)).into(),
            surface: crate::backend::kind_of(shape).is_ok_and(|k| k == crate::backend::Kind::Surface),
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

    if spec.thickness.is_some() && bodies.iter().all(|b| b.surface) {
        bail!(
            "the part is a surface, which has no material to be thick. Thicken it into a solid first — .thicken(t) — and the thickened part's wall is what this measures; the thickness thicken built is already in the evaluation as thickened_mm"
        );
    }
    let thickness = spec.thickness.as_ref().map(|t| {
        let mut result = thickness(bodies, t, (hi - lo).length());
        result.surfaces_skipped = bodies
            .iter()
            .filter(|b| b.surface)
            .map(|b| b.name.unwrap_or("part").to_owned())
            .collect();
        result
    });

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
    for body in bodies.iter().filter(|b| !b.surface) {
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
    for body in bodies.iter().filter(|b| b.surface) {
        if let Some((distance, at)) = body.boundary.distance_to_point(point) {
            if nearest.is_none_or(|(d, _, _)| distance < d) {
                nearest = Some((distance, at, body.name));
            }
        }
    }
    let (distance, at, near_body) = nearest.unwrap_or((0.0, point, None));
    let inside_any = bodies.iter().any(|b| {
        !b.surface && b.shape.classify_point(point, SURFACE_TOLERANCE_MM) == PointState::Inside
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
    if body.surface {
        return cross_surface(body, caster, origin, dir, max);
    }
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
            surface: false,
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

/// A surface body's crossings along a line: every place the line passes
/// through it, once each, and no material.
fn cross_surface(body: &Body, caster: &mut RayCaster, origin: DVec3, dir: DVec3, max: f64) -> Walk {
    let mut hits: Vec<RayHitResult> = Vec::new();
    for hit in caster.cast(origin, dir) {
        if hit.distance < -SURFACE_TOLERANCE_MM || hit.distance > max || hit.crossing == Crossing::Tangent {
            continue;
        }
        // An edge two faces share is reported by both.
        if hits.last().is_some_and(|h| (h.distance - hit.distance).abs() <= SURFACE_TOLERANCE_MM) {
            continue;
        }
        hits.push(RayHitResult {
            distance: hit.distance.max(0.0),
            point: hit.point.to_array(),
            entering: false,
            surface: true,
            tags: body.tags_of(hit.face),
            body: body.name.map(str::to_owned),
        });
    }
    Walk { starts_inside: false, ends_inside: false, hits, solid_mm: 0.0, first_solid_mm: None }
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

/// The inscribed-ball thickness at every sampled surface point: the diameter of
/// the largest ball inside the material that touches the surface there, which
/// is the thickness a mould or casting check means by the word.
///
/// The samples are the exact tessellation's nodes and a grid over every
/// triangle at a spacing tied to the part's size. The grid is what finds a
/// wall in the middle of a face: a plane is meshed as two triangles with no
/// node inside its boundary. Every sample is projected onto its own face for
/// the exact point and normal, since the ball has to be tangent to the surface
/// itself, not to a chord of it. A node on a convex edge is moved a step into
/// its face, since the neighbouring face cuts every ball tangent on the edge;
/// a node on a concave edge is measured where it is. Either is often the only
/// sample a narrow face has, and the material beside an edge is where a cut
/// that grazed another feature leaves its sliver.
fn thickness(bodies: &[Body], spec: &ThicknessSpec, diagonal: f64) -> ThicknessResult {
    let mut samples: Vec<ThicknessSample> = Vec::new();
    let mut discarded = 0usize;
    // No ball in the part is wider than the part.
    let limit = diagonal * 0.5 + 1.0;
    // About a hundred samples across the part's diagonal, before decimation.
    let spacing = (diagonal / 96.0).max(1e-3);

    let mut starts = surface_samples(bodies, spacing);
    // Decimate evenly rather than truncating, so every face keeps a share.
    let max = spec.max_samples.max(1);
    if starts.len() > max {
        let stride = starts.len().div_ceil(max);
        starts = starts.into_iter().step_by(stride).collect();
    }

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
    for Start { body: b, at, normal, face, into } in starts {
        let body = &bodies[b];
        let Some((p, outward)) = nearest[b].project(face, at) else {
            discarded += 1;
            continue;
        };
        // A projection that lands off the sample or turns the normal round has
        // found another sheet of the same surface, not the point sampled.
        if (p - at).length() > 20.0 * DEFLECTION_MM || outward.dot(normal.normalize_or_zero()) < 0.5 {
            discarded += 1;
            continue;
        }
        let (p, inward) = match into {
            Some(into) if on_convex_edge(&mut nearest[b], p, -outward) => {
                let along = (into - outward * into.dot(outward)).normalize_or_zero();
                match nearest[b].project(face, p + along * step) {
                    Some((q, outward)) if along != DVec3::ZERO => (q, -outward),
                    _ => continue,
                }
            }
            _ => (p, -outward),
        };
        let guess = last_radius.get(&(b, face)).copied().unwrap_or(spacing);
        let Some(ball) = inscribed(&mut nearest[b], p, inward, guess, limit) else {
            discarded += 1;
            continue;
        };
        last_radius.insert((b, face), ball.radius.max(1e-3));
        let (kind, wedge_deg) = faces[b].classify(face, ball.face, p, ball.centre, ball.contact);
        samples.push(ThicknessSample {
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
        });
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
        surfaces_skipped: Vec::new(),
    }
}

/// A surface point to measure from, with the mesher's normal there.
struct Start {
    body: usize,
    at: DVec3,
    normal: DVec3,
    face: usize,
    /// For a node on the face's boundary, a direction into the face.
    into: Option<DVec3>,
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

/// Every tessellation node, a barycentric grid inside every triangle, and a
/// face's triangle middles where it has nothing inside its boundary.
fn surface_samples(bodies: &[Body], spacing: f64) -> Vec<Start> {
    let mut starts: Vec<Start> = Vec::new();
    for (b, body) in bodies.iter().enumerate().filter(|(_, body)| !body.surface) {
        let mesh = body.shape.mesh();
        for run in &mesh.faces {
            let triangles = &mesh.indices[run.start * 3..(run.start + run.count) * 3];
            // A triangle side used once is on the face's boundary.
            let mut sides: HashMap<(usize, usize), usize> = HashMap::new();
            for tri in triangles.chunks_exact(3) {
                for (i, j) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                    *sides.entry((i.min(j), i.max(j))).or_default() += 1;
                }
            }
            let mut rim: HashSet<usize> = HashSet::new();
            for (&(i, j), &uses) in &sides {
                if uses == 1 {
                    rim.insert(i);
                    rim.insert(j);
                }
            }
            let first = starts.len();
            // Into the face from a boundary node: towards the middles of the
            // triangles it is a corner of.
            let mut inwards: HashMap<usize, DVec3> = HashMap::new();
            for tri in triangles.chunks_exact(3) {
                let middle = (mesh.vertices[tri[0]] + mesh.vertices[tri[1]] + mesh.vertices[tri[2]]) / 3.0;
                for &i in tri {
                    if rim.contains(&i) {
                        *inwards.entry(i).or_default() += (middle - mesh.vertices[i]).normalize_or_zero();
                    }
                }
            }
            let mut seen: HashSet<usize> = HashSet::new();
            let mut push = |at: DVec3, normal: DVec3, into: Option<DVec3>| {
                starts.push(Start { body: b, at, normal, face: run.face, into })
            };
            for tri in triangles.chunks_exact(3) {
                for &i in tri {
                    if seen.insert(i) {
                        push(mesh.vertices[i], mesh.normals[i], inwards.get(&i).copied());
                    }
                }
                let (p, q, r) = (mesh.vertices[tri[0]], mesh.vertices[tri[1]], mesh.vertices[tri[2]]);
                let (np, nq, nr) = (mesh.normals[tri[0]], mesh.normals[tri[1]], mesh.normals[tri[2]]);
                let longest = (q - p).length().max((r - q).length()).max((p - r).length());
                let n = (longest / spacing).ceil() as usize;
                // Barycentric grid strictly inside the triangle; its corners
                // and edges are the nodes and their neighbours' grids.
                for i in 1..n {
                    for j in 1..n - i {
                        let (u, v) = (i as f64 / n as f64, j as f64 / n as f64);
                        let w = 1.0 - u - v;
                        push(p * w + q * u + r * v, np * w + nq * u + nr * v, None);
                    }
                }
            }
            if starts[first..].iter().all(|s| s.into.is_some()) {
                for tri in triangles.chunks_exact(3) {
                    let [p, q, r] = [tri[0], tri[1], tri[2]];
                    starts.push(Start {
                        body: b,
                        at: (mesh.vertices[p] + mesh.vertices[q] + mesh.vertices[r]) / 3.0,
                        normal: mesh.normals[p] + mesh.normals[q] + mesh.normals[r],
                        face: run.face,
                        into: None,
                    });
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
        let g = *group_of_root.entry(r).or_insert_with(|| {
            groups.push((r, 0, p, p));
            groups.len() - 1
        });
        let group = &mut groups[g];
        group.1 += 1;
        group.2 = group.2.min(p);
        group.3 = group.3.max(p);
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
        assert!(min.thickness_mm < 0.5, "{min:?}");
        let wedge = min.wedge_deg.expect("a feather knows its angle");
        assert!((wedge - 15.0).abs() < 1.0, "wedge {wedge}, expected about 15");
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
        // Exact for its own sample point, y off the port's -X generator: a ball
        // tangent to the plane there and to the Ø10 port 10 away satisfies
        // (10 − r)² + y² = (5 + r)², so it is 5 + y²/15 across — a sampled
        // minimum, a micron or so over the 5.000 at the generator.
        let y = min.at[1];
        assert!((min.thickness_mm - (5.0 + y * y / 15.0)).abs() < 1e-4, "{min:?}");
        assert!((min.thickness_mm - 5.0).abs() < 3e-3, "{min:?}");
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


