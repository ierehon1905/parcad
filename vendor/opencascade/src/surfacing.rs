//! Surface modelling: open shells built from wires, sewn, split, patched,
//! offset and thickened, and the counts that say what a shape is. Added for
//! parcad; see PARCAD-CHANGES.md.
//!
//! Face histories are `(input face, output face)` pairs numbered as
//! [`Shape::face_map`] numbers faces.

use crate::primitives::{Compound, Shape, Wire};
use cxx::UniquePtr;
use glam::DVec3;

/// What a shape is made of.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Census {
    pub solids: usize,
    /// Faces connected through shared edges, counted as one sheet each.
    pub shells: usize,
    pub faces: usize,
    /// Edges bordered by exactly one face, seams excluded.
    pub free_edges: usize,
    pub free_edge_length: f64,
    /// Edges shared by more than two faces.
    pub multiple_edges: usize,
    /// Faces that belong to no solid.
    pub loose_faces: usize,
    /// Free edges joined end to end: loops that close, and chains that do not.
    pub closed_loops: usize,
    pub open_chains: usize,
}

/// A sample of a face: where, which way it faces, and how it bends.
#[derive(Debug, Clone, Copy)]
pub struct FaceSample {
    pub face: usize,
    pub point: DVec3,
    /// The face's outward normal.
    pub normal: DVec3,
    /// The largest curvature bending the surface toward `normal`, in 1/mm;
    /// an offset of `d` along the normal folds where `d * toward` reaches 1.
    pub toward: f64,
    /// The most negative such curvature: an offset of `-d` folds where
    /// `d * -away` reaches 1.
    pub away: f64,
}

/// The nearest point of a shape.
#[derive(Debug, Clone, Copy)]
pub struct Nearest {
    pub distance: f64,
    pub foot: DVec3,
    /// The face's outward normal at the foot; zero when the foot is on an
    /// edge or a vertex.
    pub normal: DVec3,
    pub in_face: bool,
}

/// What [`Shape::fill_loops`] made.
#[derive(Debug, Clone, Copy, Default)]
pub struct FillReport {
    pub planar: usize,
    pub filled: usize,
    /// How far a filled face's boundary strays from the edges it fills.
    pub position_error: f64,
    /// The worst angle, in radians, between a filled face and the faces it
    /// was asked to be tangent to.
    pub tangent_error: f64,
    /// Chains of selected edges that do not close.
    pub open_chains: usize,
}

/// What [`Shape::sewn`] left.
#[derive(Debug, Clone, Copy, Default)]
pub struct SewReport {
    pub free_edges: usize,
    pub multiple_edges: usize,
}

/// What each input face became.
#[derive(Debug, Clone, Default)]
pub struct FaceHistory {
    /// `(input face, output face)`: the face itself, moved, split or offset.
    pub images: Vec<(usize, usize)>,
    /// `(input face, output face)`: faces made from the input face's free
    /// edges, such as a thickened surface's side walls.
    pub lateral: Vec<(usize, usize)>,
}

impl FaceHistory {
    /// Every output face of an input face, side walls included.
    pub fn all(&self) -> impl Iterator<Item = &(usize, usize)> {
        self.images.iter().chain(&self.lateral)
    }
}

fn pairs(flat: Vec<i32>) -> FaceHistory {
    let mut history = FaceHistory::default();
    for p in flat.chunks_exact(2) {
        if p[0] < 0 {
            history.lateral.push(((-1 - p[0]) as usize, p[1] as usize));
        } else {
            history.images.push((p[0] as usize, p[1] as usize));
        }
    }
    history
}

fn boxed(inner: Result<UniquePtr<opencascade_sys::ffi::TopoDS_Shape>, cxx::Exception>) -> Result<Shape, String> {
    inner.map(|inner| Shape { inner }).map_err(|e| e.what().to_string())
}

/// How a swept curve is carried along its spine; see
/// [`crate::sweep::SweepFrame`], whose numbering this shares.
pub fn pipe_surface(spine: &Wire, profile: &Wire, mode: i32) -> Result<Shape, String> {
    boxed(ffi::parcad_pipe_surface(&spine.inner, &profile.inner, mode))
}

/// A B-spline surface in bands, as [`crate::skin::SkinSurface`] describes one,
/// cut at `v_breaks` and each band into `u_pieces` equal stretches of u, sewn;
/// and the exact box of the whole surface over those breaks.
pub fn bspline_bands(
    surface: &crate::skin::SkinSurface,
    v_breaks: &[f64],
    u_pieces: usize,
) -> Result<(Shape, (DVec3, DVec3)), String> {
    let flat: Vec<f64> = surface.poles.iter().flat_map(|p| [p.x, p.y, p.z]).collect();
    let mut bounds = Vec::new();
    let shape = boxed(ffi::parcad_bspline_bands(
        surface.nu as i32,
        surface.nv as i32,
        &flat,
        surface.uknots,
        surface.umults,
        surface.udegree as i32,
        surface.vknots,
        surface.vmults,
        surface.vdegree as i32,
        v_breaks,
        u_pieces as i32,
        &mut bounds,
    ))?;
    Ok((shape, (DVec3::new(bounds[0], bounds[1], bounds[2]), DVec3::new(bounds[3], bounds[4], bounds[5]))))
}

/// A face as large as `half` either way on the plane through `point`.
pub fn plane_face(point: DVec3, normal: DVec3, half: f64) -> Result<Shape, String> {
    boxed(ffi::parcad_plane_face(point.x, point.y, point.z, normal.x, normal.y, normal.z, half))
}

impl Shape {
    pub fn census(&self) -> Result<Census, String> {
        let v = ffi::parcad_census(&self.inner).map_err(|e| e.what().to_string())?;
        Ok(Census {
            solids: v[0] as usize,
            shells: v[1] as usize,
            faces: v[2] as usize,
            free_edges: v[3] as usize,
            free_edge_length: v[4],
            multiple_edges: v[5] as usize,
            loose_faces: v[6] as usize,
            closed_loops: v[7] as usize,
            open_chains: v[8] as usize,
        })
    }

    /// The enclosed volume, integrated by Gauss–Kronrod over every knot span
    /// of every face to relative error `eps`, and the integrator's own
    /// estimate of its error.
    pub fn volume_by_spans(&self, eps: f64) -> Result<(f64, f64), String> {
        let v = ffi::parcad_volume_by_spans(&self.inner, eps).map_err(|e| e.what().to_string())?;
        Ok((v[0], v[1]))
    }

    pub fn volume_fixed(&self) -> f64 {
        ffi::parcad_volume_fixed(&self.inner)
    }

    /// Faces the current triangulation does not cover, as `(face, meshed
    /// area, boundary area)` in the face's parameter plane, the boundary being
    /// the polygon the face's edges were discretised into: every face whose
    /// two differ by more than `rel` of the boundary's, or that carries no
    /// mesh.
    pub fn uncovered_faces(&self, rel: f64) -> Result<Vec<(usize, f64, f64)>, String> {
        let v = ffi::parcad_uncovered_faces(&self.inner, rel).map_err(|e| e.what().to_string())?;
        Ok(v.chunks_exact(3).map(|c| (c[0] as usize, c[1], c[2])).collect())
    }

    /// Every free edge, as a compound.
    pub fn free_edges(&self) -> Result<Shape, String> {
        boxed(ffi::parcad_free_edges(&self.inner))
    }

    /// Every edge between two faces cut from one surface: a split in the
    /// representation rather than an edge of the shape.
    pub fn split_edges(&self) -> Result<Shape, String> {
        boxed(ffi::parcad_split_edges(&self.inner))
    }

    /// The same shape with every face turned over.
    pub fn reversed(&self) -> Shape {
        Shape { inner: ffi::parcad_reversed(&self.inner) }
    }

    /// A wire swept along `by`: one face per edge, open or closed as the wire is.
    pub fn prism_of(wire: &Wire, by: DVec3) -> Result<Shape, String> {
        boxed(ffi::parcad_prism(opencascade_sys::ffi::cast_wire_to_shape(&wire.inner), by.x, by.y, by.z))
    }

    /// A wire revolved about +Z by `degrees`.
    pub fn revolution_of(wire: &Wire, degrees: f64) -> Result<Shape, String> {
        boxed(ffi::parcad_revolve(opencascade_sys::ffi::cast_wire_to_shape(&wire.inner), degrees))
    }

    /// A shell through `wires` in order, their pairing taken as given.
    pub fn loft_surface(wires: &[Wire], ruled: bool) -> Result<Shape, String> {
        let compound: Shape = Compound::from_shapes(wires.iter().map(Wire::to_shape)).into();
        boxed(ffi::parcad_thru_sections(&compound.inner, ruled))
    }

    /// Every shape in this compound sewn at `tolerance`, with the history of
    /// every face, numbered as this compound's [`Shape::face_map`] numbers them.
    pub fn sewn(&self, tolerance: f64) -> Result<(Shape, FaceHistory, SewReport), String> {
        let compound = self;
        let mut history = Vec::new();
        let mut stats = Vec::new();
        let shape = boxed(ffi::parcad_sew(&compound.inner, tolerance, &mut history, &mut stats))?;
        Ok((
            shape,
            pairs(history),
            SewReport { free_edges: stats[0] as usize, multiple_edges: stats[1] as usize },
        ))
    }

    /// The solid this closed shell bounds, facing out and valid.
    pub fn closed_solid(&self) -> Result<Shape, String> {
        boxed(ffi::parcad_solid_from_shell(&self.inner))
    }

    /// This shape cut along everything `tool` touches.
    pub fn split_by(&self, tool: &Shape) -> Result<(Shape, FaceHistory), String> {
        let mut history = Vec::new();
        let shape = boxed(ffi::parcad_split(&self.inner, &tool.inner, &mut history))?;
        Ok((shape, pairs(history)))
    }

    /// A point inside each face and its outward normal there, in face order;
    /// `None` for a face no sample landed in.
    pub fn face_inside_points(&self) -> Result<Vec<Option<(DVec3, DVec3)>>, String> {
        let v = ffi::parcad_face_inside_points(&self.inner).map_err(|e| e.what().to_string())?;
        Ok(v.chunks_exact(6)
            .map(|c| {
                c[0].is_finite()
                    .then(|| (DVec3::new(c[0], c[1], c[2]), DVec3::new(c[3], c[4], c[5])))
            })
            .collect())
    }

    /// A `per` by `per` grid inside every face.
    pub fn face_samples(&self, per: usize) -> Result<Vec<FaceSample>, String> {
        let v = ffi::parcad_face_samples(&self.inner, per as i32).map_err(|e| e.what().to_string())?;
        Ok(v.chunks_exact(9)
            .map(|c| FaceSample {
                face: c[0] as usize,
                point: DVec3::new(c[1], c[2], c[3]),
                normal: DVec3::new(c[4], c[5], c[6]),
                toward: c[7],
                away: c[8],
            })
            .collect())
    }

    pub fn nearest_on(&self, point: DVec3) -> Result<Nearest, String> {
        let v = ffi::parcad_nearest_on(&self.inner, point.x, point.y, point.z).map_err(|e| e.what().to_string())?;
        Ok(Nearest {
            distance: v[0],
            foot: DVec3::new(v[1], v[2], v[3]),
            normal: DVec3::new(v[4], v[5], v[6]),
            in_face: v[7] > 0.5,
        })
    }

    /// Faces filling each closed loop `edges` (edges of this shape) make.
    pub fn fill_loops(&self, edges: &Shape, tangent: bool) -> Result<(Shape, FillReport), String> {
        let mut stats = Vec::new();
        let shape = boxed(ffi::parcad_fill(&self.inner, &edges.inner, tangent, &mut stats))?;
        Ok((
            shape,
            FillReport {
                planar: stats[0] as usize,
                filled: stats[1] as usize,
                position_error: stats[2],
                tangent_error: stats[3],
                open_chains: stats[4] as usize,
            },
        ))
    }

    /// Every shell offset by `distance` along its normals, or, with
    /// `thicken`, the solid between each shell and that offset.
    pub fn offset_shells(&self, distance: f64, thicken: bool) -> Result<(Shape, FaceHistory), String> {
        let mut history = Vec::new();
        let shape = boxed(ffi::parcad_offset(&self.inner, distance, thicken, &mut history))?;
        Ok((shape, pairs(history)))
    }
}

#[cxx::bridge]
pub(crate) mod ffi {
    unsafe extern "C++" {
        include!("include/surfacing.hxx");

        type TopoDS_Shape = opencascade_sys::ffi::TopoDS_Shape;
        type TopoDS_Wire = opencascade_sys::ffi::TopoDS_Wire;

        fn parcad_census(shape: &TopoDS_Shape) -> Result<Vec<f64>>;
        fn parcad_volume_by_spans(shape: &TopoDS_Shape, eps: f64) -> Result<Vec<f64>>;
        fn parcad_volume_fixed(shape: &TopoDS_Shape) -> f64;
        fn parcad_uncovered_faces(shape: &TopoDS_Shape, rel: f64) -> Result<Vec<f64>>;
        fn parcad_free_edges(shape: &TopoDS_Shape) -> Result<UniquePtr<TopoDS_Shape>>;
        fn parcad_split_edges(shape: &TopoDS_Shape) -> Result<UniquePtr<TopoDS_Shape>>;
        fn parcad_reversed(shape: &TopoDS_Shape) -> UniquePtr<TopoDS_Shape>;
        fn parcad_prism(wire: &TopoDS_Shape, dx: f64, dy: f64, dz: f64) -> Result<UniquePtr<TopoDS_Shape>>;
        fn parcad_revolve(wire: &TopoDS_Shape, degrees: f64) -> Result<UniquePtr<TopoDS_Shape>>;
        fn parcad_thru_sections(wires: &TopoDS_Shape, ruled: bool) -> Result<UniquePtr<TopoDS_Shape>>;
        fn parcad_pipe_surface(
            spine: &TopoDS_Wire,
            profile: &TopoDS_Wire,
            mode: i32,
        ) -> Result<UniquePtr<TopoDS_Shape>>;
        #[allow(clippy::too_many_arguments)]
        fn parcad_bspline_bands(
            nu: i32,
            nv: i32,
            poles: &[f64],
            uknots: &[f64],
            umults: &[i32],
            udeg: i32,
            vknots: &[f64],
            vmults: &[i32],
            vdeg: i32,
            v_breaks: &[f64],
            u_pieces: i32,
            bounds: &mut Vec<f64>,
        ) -> Result<UniquePtr<TopoDS_Shape>>;
        fn parcad_sew(
            shapes: &TopoDS_Shape,
            tolerance: f64,
            history: &mut Vec<i32>,
            stats: &mut Vec<f64>,
        ) -> Result<UniquePtr<TopoDS_Shape>>;
        fn parcad_solid_from_shell(shape: &TopoDS_Shape) -> Result<UniquePtr<TopoDS_Shape>>;
        fn parcad_plane_face(
            px: f64,
            py: f64,
            pz: f64,
            nx: f64,
            ny: f64,
            nz: f64,
            half: f64,
        ) -> Result<UniquePtr<TopoDS_Shape>>;
        fn parcad_split(
            object: &TopoDS_Shape,
            tool: &TopoDS_Shape,
            history: &mut Vec<i32>,
        ) -> Result<UniquePtr<TopoDS_Shape>>;
        fn parcad_face_inside_points(shape: &TopoDS_Shape) -> Result<Vec<f64>>;
        fn parcad_face_samples(shape: &TopoDS_Shape, per: i32) -> Result<Vec<f64>>;
        fn parcad_nearest_on(shape: &TopoDS_Shape, x: f64, y: f64, z: f64) -> Result<Vec<f64>>;
        fn parcad_fill(
            shape: &TopoDS_Shape,
            edges: &TopoDS_Shape,
            tangent: bool,
            stats: &mut Vec<f64>,
        ) -> Result<UniquePtr<TopoDS_Shape>>;
        fn parcad_offset(
            shape: &TopoDS_Shape,
            offset: f64,
            thicken: bool,
            history: &mut Vec<i32>,
        ) -> Result<UniquePtr<TopoDS_Shape>>;
    }
}
