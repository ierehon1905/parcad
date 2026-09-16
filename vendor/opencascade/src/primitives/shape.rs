use crate::{
    adhoc::AdHocShape,
    history::ffi as history,
    mesh::{Mesh, Mesher},
    primitives::{
        make_dir, make_point, make_vec, BooleanShape, Compound, Edge, EdgeIterator, Face,
        FaceIterator, ShapeType, Solid, Vertex, Wire,
    },
    Error,
};
use cxx::UniquePtr;
use glam::{dvec3, DVec3};
use opencascade_sys::ffi;
use std::path::Path;

pub struct Shape {
    pub(crate) inner: UniquePtr<ffi::TopoDS_Shape>,
}

/// Cheap: `TopoDS_Shape` is a handle onto a reference-counted `TShape`, so this
/// shares the underlying geometry rather than copying it. Needed because several
/// operations here take `self` by value while the caller still wants the
/// original — hollowing a solid, for instance, needs both the solid and a
/// shrunken copy of it.
impl Clone for Shape {
    fn clone(&self) -> Self {
        Self {
            inner: ffi::TopoDS_Shape_to_owned(&self.inner),
        }
    }
}

impl AsRef<Shape> for Shape {
    fn as_ref(&self) -> &Shape {
        self
    }
}

impl From<Vertex> for Shape {
    fn from(vertex: Vertex) -> Self {
        let shape = ffi::cast_vertex_to_shape(&vertex.inner);
        let inner = ffi::TopoDS_Shape_to_owned(shape);

        Shape { inner }
    }
}

impl From<Edge> for Shape {
    fn from(edge: Edge) -> Self {
        let shape = ffi::cast_edge_to_shape(&edge.inner);
        let inner = ffi::TopoDS_Shape_to_owned(shape);

        Shape { inner }
    }
}

impl From<Wire> for Shape {
    fn from(wire: Wire) -> Self {
        let shape = ffi::cast_wire_to_shape(&wire.inner);
        let inner = ffi::TopoDS_Shape_to_owned(shape);

        Shape { inner }
    }
}

impl From<Face> for Shape {
    fn from(face: Face) -> Self {
        let shape = ffi::cast_face_to_shape(&face.inner);
        let inner = ffi::TopoDS_Shape_to_owned(shape);

        Shape { inner }
    }
}

impl From<Solid> for Shape {
    fn from(solid: Solid) -> Self {
        let shape = ffi::cast_solid_to_shape(&solid.inner);
        let inner = ffi::TopoDS_Shape_to_owned(shape);

        Shape { inner }
    }
}

impl From<Compound> for Shape {
    fn from(compound: Compound) -> Self {
        let shape = ffi::cast_compound_to_shape(&compound.inner);
        let inner = ffi::TopoDS_Shape_to_owned(shape);

        Shape { inner }
    }
}

impl From<BooleanShape> for Shape {
    fn from(boolean_shape: BooleanShape) -> Self {
        boolean_shape.shape
    }
}

impl From<AdHocShape> for Shape {
    fn from(adhoc_shape: AdHocShape) -> Self {
        adhoc_shape.0
    }
}

impl Shape {
    // PARCAD: sweep a profile face along a spine wire (BRepOffsetAPI_MakePipe).
    // The spine must be G1-continuous — straight runs joined by tangent arcs —
    // which the caller is responsible for; a sharp corner makes the sweep's
    // frame ambiguous and OCCT resolves it however it likes.
    pub fn sweep_profile_along(profile: &Face, spine: &Wire) -> Self {
        let profile_shape = ffi::cast_face_to_shape(&profile.inner);
        let mut make_pipe = ffi::BRepOffsetAPI_MakePipe_ctor(&spine.inner, profile_shape);
        let shape = make_pipe.pin_mut().Shape();

        Self {
            inner: ffi::TopoDS_Shape_to_owned(shape),
        }
    }

    pub fn shape_type(&self) -> ShapeType {
        self.inner.ShapeType().into()
    }

    /// Run OpenCASCADE's shape-healing pass over this shape.
    ///
    /// Added for parcad; see PARCAD-CHANGES.md. `precision` and `max_tolerance`
    /// bound how far the repair may move geometry — the caller is expected to
    /// measure the result rather than trust it, because healing is free to
    /// change the part.
    pub fn healed(&self, precision: f64, max_tolerance: f64) -> Self {
        let fixed = ffi::ShapeFix_repair(&self.inner, precision, max_tolerance);
        Self {
            inner: ffi::TopoDS_Shape_to_owned(&fixed),
        }
    }

    /// Ask OpenCASCADE whether this shape is valid, and if not, what is wrong.
    ///
    /// Added for parcad; see PARCAD-CHANGES.md. `Ok(())` means valid. This is a
    /// different question from the `IsDone()` an operation reports about itself:
    /// a fillet has been observed returning `IsDone() == true` alongside a solid
    /// whose surface will not close.
    ///
    /// `exact` enables per-point checking, which is slow and off by default in
    /// OpenCASCADE — a face carrying an unusable surface can pass without it.
    pub fn check_validity(&self, exact: bool) -> Result<(), String> {
        let report = ffi::BRepCheck_report(&self.inner, exact);
        if report.is_empty() {
            Ok(())
        } else {
            Err(report)
        }
    }

    /// Full topology dump: faces, wires, edges, vertices with geometry types
    /// and tolerances. Added for parcad as a diagnostic; see PARCAD-CHANGES.md.
    pub fn topology_report(&self) -> String {
        ffi::Shape_topology_report(&self.inner)
    }

    /// Write the native BREP format, which preserves exact topology.
    /// Added for parcad as a diagnostic; see PARCAD-CHANGES.md.
    pub fn write_brep(&self, path: &str) -> bool {
        ffi::BRepTools_write_brep(&self.inner, path.to_string())
    }

    /// Measured geometry of this shape as JSON: per-solid exact mass
    /// properties and every face's surface data, down to B-spline pole grids.
    /// Added for parcad, which uses it to read a foreign STEP export back
    /// into numbers a part can be authored from; the schema is what
    /// parcad's `protocol.rs` deserialises. See PARCAD-CHANGES.md.
    pub fn geometry_json(&self) -> String {
        ffi::Shape_geometry_json(&self.inner)
    }

    /// What each face of the first solid *is*, as JSON: kind, exact area,
    /// centroid, placement direction and radius, and the faces it shares an
    /// edge with.
    ///
    /// The compact companion to [`Self::geometry_json`], for a caller that
    /// wants to describe a part rather than recreate one. It writes no boundary
    /// wires and does not descend into a B-spline's poles, which is where
    /// almost all of the full report's cost is. Added for parcad; see
    /// PARCAD-CHANGES.md.
    pub fn faces_json(&self) -> String {
        ffi::Shape_faces_json(&self.inner)
    }

    pub fn fillet_edge(&mut self, radius: f64, edge: &Edge) {
        let mut make_fillet = ffi::BRepFilletAPI_MakeFillet_ctor(&self.inner);
        make_fillet.pin_mut().add_edge(radius, &edge.inner);

        let filleted_shape = make_fillet.pin_mut().Shape();

        self.inner = ffi::TopoDS_Shape_to_owned(filleted_shape);
    }

    pub fn chamfer_edge(&mut self, distance: f64, edge: &Edge) {
        let mut make_chamfer = ffi::BRepFilletAPI_MakeChamfer_ctor(&self.inner);
        make_chamfer.pin_mut().add_edge(distance, &edge.inner);

        let chamfered_shape = make_chamfer.pin_mut().Shape();

        self.inner = ffi::TopoDS_Shape_to_owned(chamfered_shape);
    }

    pub fn fillet_edges<T: AsRef<Edge>>(
        &mut self,
        radius: f64,
        edges: impl IntoIterator<Item = T>,
    ) {
        let mut make_fillet = ffi::BRepFilletAPI_MakeFillet_ctor(&self.inner);

        for edge in edges.into_iter() {
            make_fillet.pin_mut().add_edge(radius, &edge.as_ref().inner);
        }

        let filleted_shape = make_fillet.pin_mut().Shape();

        self.inner = ffi::TopoDS_Shape_to_owned(filleted_shape);
    }

    /// Fillet selected edges without touching `self`, and say why when the
    /// kernel cannot.
    ///
    /// The infallible forms above abort the process on an unbuildable radius:
    /// `Build` raises `Standard_Failure`, nothing on the plain bridge path
    /// catches it, and the uncaught exception calls `std::terminate`. This
    /// form catches it at the C++ boundary and returns the kernel's own
    /// words. Non-mutating so a caller can probe several radii against one
    /// input. Added for parcad; see PARCAD-CHANGES.md.
    pub fn filleted_edges<T: AsRef<Edge>>(
        &self,
        radius: f64,
        edges: impl IntoIterator<Item = T>,
    ) -> Result<Self, String> {
        self.treated_edges(radius, edges, false)
    }

    /// Chamfer selected edges without touching `self`; the fallible sibling of
    /// [`Self::filleted_edges`].
    pub fn chamfered_edges<T: AsRef<Edge>>(
        &self,
        distance: f64,
        edges: impl IntoIterator<Item = T>,
    ) -> Result<Self, String> {
        self.treated_edges(distance, edges, true)
    }

    fn treated_edges<T: AsRef<Edge>>(
        &self,
        distance: f64,
        edges: impl IntoIterator<Item = T>,
        chamfer: bool,
    ) -> Result<Self, String> {
        let mut treatment = if chamfer {
            history::parcad_chamfer_with_history(&self.inner)
        } else {
            history::parcad_fillet_with_history(&self.inner)
        };
        for edge in edges {
            treatment.pin_mut().add(distance, &edge.as_ref().inner);
        }
        if !treatment.pin_mut().build() {
            return Err(treatment_failure(&treatment));
        }
        Ok(Self {
            inner: ffi::TopoDS_Shape_to_owned(treatment.pin_mut().result()),
        })
    }

    /// Fillet selected edges and retain the result shapes generated from them.
    ///
    /// The generated shapes are exact OCCT history, suitable for transient
    /// feature inspection. They are not stable entity IDs and must not escape
    /// the current evaluation.
    pub fn fillet_edges_with_history<T: AsRef<Edge>>(
        &mut self,
        radius: f64,
        edges: impl IntoIterator<Item = T>,
    ) -> Result<Treatment, String> {
        self.treat_edges_with_history(radius, edges, false)
    }

    pub fn chamfer_edges<T: AsRef<Edge>>(
        &mut self,
        distance: f64,
        edges: impl IntoIterator<Item = T>,
    ) {
        let mut make_chamfer = ffi::BRepFilletAPI_MakeChamfer_ctor(&self.inner);

        for edge in edges.into_iter() {
            make_chamfer
                .pin_mut()
                .add_edge(distance, &edge.as_ref().inner);
        }

        let chamfered_shape = make_chamfer.pin_mut().Shape();

        self.inner = ffi::TopoDS_Shape_to_owned(chamfered_shape);
    }

    /// Chamfer selected edges and retain the result shapes generated from them.
    pub fn chamfer_edges_with_history<T: AsRef<Edge>>(
        &mut self,
        distance: f64,
        edges: impl IntoIterator<Item = T>,
    ) -> Result<Treatment, String> {
        self.treat_edges_with_history(distance, edges, true)
    }

    fn treat_edges_with_history<T: AsRef<Edge>>(
        &mut self,
        distance: f64,
        edges: impl IntoIterator<Item = T>,
        chamfer: bool,
    ) -> Result<Treatment, String> {
        let edges: Vec<Edge> = edges
            .into_iter()
            .map(|edge| edge.as_ref().clone())
            .collect();
        let mut treatment = if chamfer {
            history::parcad_chamfer_with_history(&self.inner)
        } else {
            history::parcad_fillet_with_history(&self.inner)
        };
        for edge in &edges {
            treatment.pin_mut().add(distance, &edge.inner);
        }
        if !treatment.pin_mut().build() {
            return Err(treatment_failure(&treatment));
        }

        let mut generated = Vec::new();
        for edge in edges {
            let shapes = treatment.pin_mut().generated(&edge.inner);
            let made: Vec<Self> = shapes
                .iter()
                .map(|shape| Self {
                    inner: ffi::TopoDS_Shape_to_owned(shape),
                })
                .collect();
            generated.push((edge, made));
        }
        self.inner = ffi::TopoDS_Shape_to_owned(treatment.pin_mut().result());
        Ok(Treatment {
            generated,
            history: treatment,
        })
    }

    /// Performs fillet of `radius` on all edges of the shape
    pub fn fillet(&mut self, radius: f64) {
        self.fillet_edges(radius, self.edges());
    }

    /// Performs chamfer of `distance` on all edges of the shape
    pub fn chamfer(&mut self, distance: f64) {
        self.chamfer_edges(distance, self.edges());
    }

    pub fn subtract(&self, other: &Shape) -> BooleanShape {
        BooleanShape::cut(self, other)
    }

    pub fn read_step(path: impl AsRef<Path>) -> Result<Self, Error> {
        let mut reader = ffi::STEPControl_Reader_ctor();

        let status = ffi::read_step(
            reader.pin_mut(),
            path.as_ref().to_string_lossy().to_string(),
        );

        if status != ffi::IFSelect_ReturnStatus::IFSelect_RetDone {
            return Err(Error::StepReadFailed);
        }

        reader
            .pin_mut()
            .TransferRoots(&ffi::Message_ProgressRange_ctor());

        let inner = ffi::one_shape(&reader);

        Ok(Self { inner })
    }

    pub fn write_step(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let mut writer = ffi::STEPControl_Writer_ctor();

        let status = ffi::transfer_shape(writer.pin_mut(), &self.inner);

        if status != ffi::IFSelect_ReturnStatus::IFSelect_RetDone {
            return Err(Error::StepWriteFailed);
        }

        let status = ffi::write_step(
            writer.pin_mut(),
            path.as_ref().to_string_lossy().to_string(),
        );

        if status != ffi::IFSelect_ReturnStatus::IFSelect_RetDone {
            return Err(Error::StepWriteFailed);
        }

        Ok(())
    }

    pub fn union(&self, other: &Shape) -> BooleanShape {
        BooleanShape::fuse(self, other)
    }

    /// Write the shape as STL, meshed to `deflection` where it is not already
    /// meshed at least that finely. A shape the caller has already meshed at
    /// this tolerance is written as it stands; a finer request re-meshes it.
    pub fn write_stl<P: AsRef<Path>>(&self, path: P, deflection: f64) -> Result<(), Error> {
        let mut stl_writer = ffi::StlAPI_Writer_ctor();
        let triangulation = ffi::BRepMesh_IncrementalMesh_ctor(&self.inner, deflection);
        let success = ffi::write_stl(
            stl_writer.pin_mut(),
            triangulation.Shape(),
            path.as_ref().to_string_lossy().to_string(),
        );

        if success {
            Ok(())
        } else {
            Err(Error::StlWriteFailed)
        }
    }

    pub fn clean(&mut self) {
        // A boolean can leave an edge that was a primitive's seam carrying
        // both seam pcurves while bordering its face only on one side. The
        // dead half blocks the unifier from merging that edge with a
        // collinear neighbour, so drop it first. Representation data only;
        // geometry is untouched.
        ffi::Shape_drop_unused_seam_pcurves(&self.inner);
        let mut upgrader = ffi::ShapeUpgrade_UnifySameDomain_ctor(&self.inner, true, true, true);
        upgrader.pin_mut().AllowInternalEdges(false);
        // The default merge tolerances (1e-7 mm, 1e-12 rad) only ever weld
        // exactly coincident geometry — fine for boolean imprints, whose split
        // pieces share exact curves, but blind to fragments downstream of an
        // approximated rebuild. A fillet corner rebuilt through approximation
        // places its vertices only to the vertex tolerance (1e-4 mm), so two
        // collinear pieces of one tangent line come back ~1e-7 mm and ~1e-7 rad
        // apart and the pass refuses them. These bounds cover that vertex
        // tolerance with margin while staying orders of magnitude below any
        // designed angle or offset.
        upgrader.pin_mut().SetLinearTolerance(1.0e-4);
        upgrader.pin_mut().SetAngularTolerance(1.0e-4);
        upgrader.pin_mut().Build();

        // Faces the mesher cannot read, rewritten; see `parcad_tidy_faces`.
        self.inner = history::parcad_tidy_faces_of(upgrader.Shape());
    }

    /// The least distance to `other`, and the two points it is measured
    /// between; `None` when the search fails. Zero when the shapes touch or
    /// overlap. Added for parcad; see PARCAD-CHANGES.md.
    pub fn least_distance_to(&self, other: &Shape) -> Option<(f64, DVec3, DVec3)> {
        let mut on_self = make_point(DVec3::ZERO);
        let mut on_other = make_point(DVec3::ZERO);
        let distance = ffi::BRepExtrema_least_distance(
            &self.inner,
            &other.inner,
            on_self.pin_mut(),
            on_other.pin_mut(),
        );
        (distance >= 0.0).then(|| {
            (
                distance,
                dvec3(on_self.X(), on_self.Y(), on_self.Z()),
                dvec3(on_other.X(), on_other.Y(), on_other.Z()),
            )
        })
    }

    /// `clean()` with its history kept: the unified shape, and what every
    /// face and edge of this one became. Added for parcad; see
    /// PARCAD-CHANGES.md.
    pub fn into_unified(self) -> Unification {
        ffi::Shape_drop_unused_seam_pcurves(&self.inner);
        let history = history::parcad_unify_with_history(&self.inner);
        let shape = Shape {
            inner: ffi::TopoDS_Shape_to_owned(history.result()),
        };
        Unification { shape, history }
    }

    /// Unwrap a compound that contains exactly one solid.
    ///
    /// Several OCCT builders — `BRepFilletAPI_MakeFillet` among them — hand back
    /// a `TopoDS_Compound` wrapping a single solid rather than the solid itself.
    /// That distinction is invisible until you feed the result to a boolean,
    /// which can then quietly produce nothing. Returns `None` when the shape is
    /// not a compound of exactly one solid, so callers can tell "unwrapped" from
    /// "left alone".
    pub fn single_solid(&self) -> Option<Self> {
        let mut explorer =
            ffi::TopExp_Explorer_ctor(&self.inner, ffi::TopAbs_ShapeEnum::TopAbs_SOLID);

        let mut found: Option<UniquePtr<ffi::TopoDS_Shape>> = None;
        while explorer.More() {
            if found.is_some() {
                // More than one: unwrapping would silently discard a body.
                return None;
            }
            let solid = ffi::TopoDS_cast_to_solid(explorer.Current());
            found = Some(ffi::TopoDS_Shape_to_owned(ffi::cast_solid_to_shape(solid)));
            explorer.pin_mut().Next();
        }

        found.map(|inner| Self { inner })
    }

    /// How many sealed internal voids the shape's solids enclose.
    ///
    /// A solid is bounded by one outer shell; every further shell bounds a
    /// cavity with no path to the outside. Counted per solid, so a compound of
    /// several bodies reports the voids inside them, not the bodies.
    pub fn internal_void_count(&self) -> usize {
        let mut voids = 0;
        let mut solids =
            ffi::TopExp_Explorer_ctor(&self.inner, ffi::TopAbs_ShapeEnum::TopAbs_SOLID);
        while solids.More() {
            let mut shells =
                ffi::TopExp_Explorer_ctor(solids.Current(), ffi::TopAbs_ShapeEnum::TopAbs_SHELL);
            let mut bounded_by = 0usize;
            while shells.More() {
                bounded_by += 1;
                shells.pin_mut().Next();
            }
            voids += bounded_by.saturating_sub(1);
            solids.pin_mut().Next();
        }
        voids
    }

    /// Apply an arbitrary rigid transform, returning a new shape.
    ///
    /// The general escape hatch the rest of the transforms are built on. Unlike
    /// [`Self::set_global_translation`], which overwrites a shape's location
    /// and therefore cannot be nested, this bakes the transform into the
    /// geometry and composes.
    pub fn transformed(&self, transform: &ffi::gp_Trsf) -> Self {
        let mut builder = ffi::BRepBuilderAPI_Transform_ctor(&self.inner, transform, true);
        let inner = ffi::TopoDS_Shape_to_owned(builder.pin_mut().Shape());

        Self { inner }
    }

    /// Rotate by `radians` about the line through `origin` along `axis`,
    /// right-handed.
    pub fn rotated(&self, origin: DVec3, axis: DVec3, radians: f64) -> Self {
        let point = make_point(origin);
        let dir = make_dir(axis);
        let line = ffi::gp_Ax1_ctor(&point, &dir);

        let mut transform = ffi::new_transform();
        transform.pin_mut().SetRotation(&line, radians);

        self.transformed(&transform)
    }

    /// Scale about the origin by a different factor on each axis, or `None` when
    /// OCCT's general transform fails. Every surface comes back as its exact
    /// B-spline conversion — an ellipsoid is a rational B-spline, not an
    /// approximation of one. Added for parcad; see PARCAD-CHANGES.md.
    pub fn scaled_axes(&self, by: DVec3) -> Option<Self> {
        let scaled = ffi::Shape_scaled_axes(&self.inner, by.x, by.y, by.z);
        let shape = Self {
            inner: ffi::TopoDS_Shape_to_owned(&scaled),
        };
        (shape.faces().count() > 0).then_some(shape)
    }

    /// Scale about `origin` by one factor in every direction.
    ///
    /// Uniform only, because `gp_Trsf` is a similarity transform and cannot
    /// represent anything else. Non-uniform scaling would need `gp_GTrsf` and
    /// `BRepBuilderAPI_GTransform`, which `opencascade-sys` does not bind — and
    /// it is not merely a bigger matrix: stretching one axis turns a cylinder
    /// into an elliptical one and a fillet's arc into an ellipse, so the exact
    /// surfaces have to change type, not just move.
    pub fn scaled_uniform(&self, origin: DVec3, factor: f64) -> Self {
        let point = make_point(origin);

        let mut transform = ffi::new_transform();
        transform.pin_mut().SetScale(&point, factor);

        self.transformed(&transform)
    }

    /// Translate, composably.
    pub fn translated(&self, by: DVec3) -> Self {
        let mut transform = ffi::new_transform();
        let vec = make_vec(by);
        transform.pin_mut().set_translation_vec(&vec);

        self.transformed(&transform)
    }

    pub fn set_global_translation(&mut self, translation: DVec3) {
        let mut transform = ffi::new_transform();
        let translation_vec = make_vec(translation);
        transform.pin_mut().set_translation_vec(&translation_vec);

        let location = ffi::TopLoc_Location_from_transform(&transform);

        self.inner
            .pin_mut()
            .set_global_translation(&location, false);
    }

    pub fn mesh(&self) -> Mesh {
        let mesher = Mesher::new(self);
        mesher.mesh()
    }

    pub fn edges(&self) -> EdgeIterator {
        let explorer = ffi::TopExp_Explorer_ctor(&self.inner, ffi::TopAbs_ShapeEnum::TopAbs_EDGE);

        EdgeIterator { explorer }
    }

    pub fn faces(&self) -> FaceIterator {
        let explorer = ffi::TopExp_Explorer_ctor(&self.inner, ffi::TopAbs_ShapeEnum::TopAbs_FACE);

        FaceIterator { explorer }
    }

    // TODO(bschwind) - Convert the return type to an iterator.
    pub fn faces_along_ray(&self, ray_start: DVec3, ray_dir: DVec3) -> Vec<(Face, DVec3)> {
        let mut intersector = ffi::BRepIntCurveSurface_Inter_ctor();
        let tolerance = 0.0001;
        intersector.pin_mut().Init(
            &self.inner,
            &ffi::gp_Lin_ctor(&make_point(ray_start), &make_dir(ray_dir)),
            tolerance,
        );

        let mut results = vec![];

        while intersector.More() {
            let face = ffi::BRepIntCurveSurface_Inter_face(&intersector);
            let point = ffi::BRepIntCurveSurface_Inter_point(&intersector);

            let face = Face {
                inner: ffi::TopoDS_Face_to_owned(&face),
            };

            results.push((face, dvec3(point.X(), point.Y(), point.Z())));

            intersector.pin_mut().Next();
        }

        results
    }

    pub fn hollow<T: AsRef<Face>>(
        self,
        offset: f64,
        faces_to_remove: impl IntoIterator<Item = T>,
    ) -> Self {
        let mut faces_list = ffi::new_list_of_shape();

        for face in faces_to_remove.into_iter() {
            ffi::shape_list_append_face(faces_list.pin_mut(), &face.as_ref().inner);
        }

        let mut solid_maker = ffi::BRepOffsetAPI_MakeThickSolid_ctor();
        ffi::MakeThickSolidByJoin(
            solid_maker.pin_mut(),
            &self.inner,
            &faces_list,
            offset,
            0.001,
        );

        let hollowed_shape = solid_maker.pin_mut().Shape();
        let inner = ffi::TopoDS_Shape_to_owned(hollowed_shape);

        Self { inner }
    }

    pub fn offset_surface(self, offset: f64) -> Self {
        let faces_to_remove: [Face; 0] = [];
        self.hollow(offset, faces_to_remove)
    }

    /// The same solid with its faces turned to point outward, when it is a
    /// single closed solid; anything else comes back unchanged. Added for
    /// parcad; see PARCAD-CHANGES.md.
    pub fn oriented_outward(&self) -> Self {
        let fixed = ffi::BRepLib_orient_closed_solid(&self.inner);
        Self {
            inner: ffi::TopoDS_Shape_to_owned(&fixed),
        }
    }

    /// The enclosed volume with its sign: negative when the shape's faces are
    /// oriented inward, which is what an inside-out solid looks like to every
    /// later boolean. Added for parcad; see PARCAD-CHANGES.md.
    pub fn signed_volume(&self) -> f64 {
        let mut props = ffi::GProp_GProps_ctor();
        // Adaptive to 1e-7 relative: the fixed-order default is not exact on
        // B-spline faces, and every caller compares this against a closed form.
        ffi::BRepGProp_VolumeProperties_eps(&self.inner, props.pin_mut(), 1e-7);
        props.Mass()
    }

    /// Which side of the solid's boundary a point is on, `BRepClass3d`. Added
    /// for parcad; see PARCAD-CHANGES.md.
    pub fn classify_point(&self, point: DVec3, tolerance: f64) -> PointState {
        match ffi::BRepClass3d_classify(&self.inner, point.x, point.y, point.z, tolerance) {
            0 => PointState::Inside,
            1 => PointState::Outside,
            2 => PointState::OnBoundary,
            _ => PointState::Unknown,
        }
    }

    /// The least distance from a point to this shape's boundary, and the
    /// point on the boundary it is measured to. Unsigned: pair it with
    /// [`Shape::classify_point`] for a sign. Added for parcad.
    pub fn distance_to_point(&self, point: DVec3) -> Option<(f64, DVec3)> {
        let probe: Shape = Vertex::new(point).into();
        probe
            .least_distance_to(self)
            .map(|(distance, _, on_shape)| (distance, on_shape))
    }

    /// Load this shape for repeated ray casts. Added for parcad.
    pub fn ray_caster(&self, tolerance: f64) -> RayCaster {
        let mut inner = ffi::BRepIntCurveSurface_Inter_ctor();
        ffi::BRepIntCurveSurface_Inter_load(inner.pin_mut(), &self.inner, tolerance);
        RayCaster {
            inner,
            faces: self.face_map(),
        }
    }

    /// Load this shape for repeated nearest-boundary-point questions. Added
    /// for parcad; see PARCAD-CHANGES.md.
    pub fn nearest_boundary(&self) -> NearestBoundary {
        NearestBoundary {
            inner: ffi::NearestBoundary_new(&self.inner),
        }
    }

    /// Every face of this shape, numbered in traversal order — the numbering
    /// `Mesh::faces` and `faces_json` use. Added for parcad.
    pub fn face_map(&self) -> FaceMap {
        let mut inner = ffi::new_indexed_map_of_shape();
        ffi::map_shapes(
            &self.inner,
            ffi::TopAbs_ShapeEnum::TopAbs_FACE,
            inner.pin_mut(),
        );
        FaceMap { inner }
    }

    /// Tight bounds from the exact geometry, `BRepBndLib::AddOptimal` with no
    /// triangulation and no tolerance gap. `None` for a shape with no extent.
    /// Added for parcad.
    pub fn bounds_optimal(&self) -> Option<(DVec3, DVec3)> {
        let (mut x0, mut y0, mut z0, mut x1, mut y1, mut z1) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        ffi::Shape_bounds_optimal(
            &self.inner,
            &mut x0,
            &mut y0,
            &mut z0,
            &mut x1,
            &mut y1,
            &mut z1,
        )
        .then(|| (dvec3(x0, y0, z0), dvec3(x1, y1, z1)))
    }
}

/// Where a point lies relative to a solid. Added for parcad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointState {
    Inside,
    Outside,
    /// Within the classification tolerance of a face.
    OnBoundary,
    Unknown,
}

/// Which way a line crosses the material at a hit. Added for parcad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crossing {
    Entering,
    Leaving,
    /// The line grazes the surface without passing through it.
    Tangent,
}

/// A shape's faces by traversal number. Added for parcad.
pub struct FaceMap {
    inner: UniquePtr<ffi::IndexedMapOfShape>,
}

impl FaceMap {
    /// The face's 0-based traversal number, or `None` for a face that is not
    /// a sub-shape of the mapped shape (a copy, however exactly placed).
    pub fn index_of(&self, face: &Face) -> Option<usize> {
        let found = ffi::IndexedMapOfShape_find_index(&self.inner, ffi::cast_face_to_shape(&face.inner));
        (found > 0).then(|| found as usize - 1)
    }

    pub fn len(&self) -> usize {
        self.inner.Extent() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One place a line meets a shape's boundary. Added for parcad.
pub struct RayHit {
    /// Parameter along the line from its origin, in the direction's units —
    /// millimetres for a unit direction. Negative behind the origin.
    pub distance: f64,
    pub point: DVec3,
    /// Traversal number of the face hit, in the shape's own face order.
    pub face: usize,
    pub crossing: Crossing,
    /// The hit lies on the face's boundary rather than inside it, which is
    /// where a neighbouring face reports the same hit again.
    pub on_boundary: bool,
}

/// A shape held ready for many lines, `BRepIntCurveSurface_Inter` loaded
/// once. Added for parcad; see PARCAD-CHANGES.md.
pub struct RayCaster {
    inner: UniquePtr<ffi::BRepIntCurveSurface_Inter>,
    faces: FaceMap,
}

impl RayCaster {
    /// Every hit along the infinite line through `origin` in `direction`,
    /// sorted by parameter, hits behind the origin included.
    pub fn cast(&mut self, origin: DVec3, direction: DVec3) -> Vec<RayHit> {
        let line = ffi::gp_Lin_ctor(&make_point(origin), &make_dir(direction));
        ffi::BRepIntCurveSurface_Inter_init_line(self.inner.pin_mut(), &line);

        let mut hits = Vec::new();
        while self.inner.More() {
            let point = ffi::BRepIntCurveSurface_Inter_point(&self.inner);
            let face = ffi::BRepIntCurveSurface_Inter_face(&self.inner);
            let face = Face {
                inner: ffi::TopoDS_Face_to_owned(&face),
            };
            hits.push(RayHit {
                distance: ffi::BRepIntCurveSurface_Inter_w(&self.inner),
                point: dvec3(point.X(), point.Y(), point.Z()),
                face: self.faces.index_of(&face).unwrap_or(usize::MAX),
                crossing: match ffi::BRepIntCurveSurface_Inter_transition(&self.inner) {
                    0 => Crossing::Entering,
                    1 => Crossing::Leaving,
                    _ => Crossing::Tangent,
                },
                on_boundary: ffi::BRepIntCurveSurface_Inter_state(&self.inner) == 1,
            });
            self.inner.pin_mut().Next();
        }
        hits.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        hits
    }
}

/// A shape held ready for many nearest-point questions, every face's and
/// edge's projector built once. Added for parcad; see PARCAD-CHANGES.md.
pub struct NearestBoundary {
    inner: UniquePtr<ffi::NearestBoundary>,
}

/// A boundary point, and the traversal number of a face it lies on.
#[derive(Debug, Clone, Copy)]
pub struct BoundaryPoint {
    pub distance: f64,
    pub point: DVec3,
    pub face: usize,
}

impl NearestBoundary {
    /// The boundary point nearest `point`, if one is nearer than `within`. A
    /// point on an edge names one of the faces that share it.
    pub fn nearest_within(&mut self, point: DVec3, within: f64) -> Option<BoundaryPoint> {
        let mut at = make_point(DVec3::ZERO);
        let mut face = -1;
        let distance = ffi::NearestBoundary_nearest(
            self.inner.pin_mut(),
            point.x,
            point.y,
            point.z,
            within,
            at.pin_mut(),
            &mut face,
        );
        (distance >= 0.0 && face >= 0).then(|| BoundaryPoint {
            distance,
            point: dvec3(at.X(), at.Y(), at.Z()),
            face: face as usize,
        })
    }

    /// The point of face `face`'s surface nearest `point`, and the face's
    /// outward unit normal there. `None` where the surface has no normal, or
    /// where that point is outside the face.
    pub fn project(&mut self, face: usize, point: DVec3) -> Option<(DVec3, DVec3)> {
        let mut at = make_point(DVec3::ZERO);
        let mut normal = ffi::new_vec(0.0, 0.0, 0.0);
        let ok = ffi::NearestBoundary_project(
            self.inner.pin_mut(),
            face as i32,
            point.x,
            point.y,
            point.z,
            at.pin_mut(),
            normal.pin_mut(),
        );
        let n = dvec3(normal.X(), normal.Y(), normal.Z());
        let len = n.length();
        (ok && len.is_finite() && len > 1e-12).then(|| (dvec3(at.X(), at.Y(), at.Z()), n / len))
    }
}

/// A fillet or chamfer that built, with its history still alive. Added for
/// parcad; see PARCAD-CHANGES.md. `generated` is what each treated edge became
/// (its new faces, usually one); the methods answer what any input face or
/// edge became, which is what lets a name survive the treatment.
pub struct Treatment {
    pub generated: Vec<(Edge, Vec<Shape>)>,
    history: UniquePtr<history::ParcadEdgeTreatment>,
}

impl Treatment {
    pub fn modified_edge(&mut self, edge: &Edge) -> Vec<Edge> {
        super::boolean_shape::edges(
            self.history
                .pin_mut()
                .modified(ffi::cast_edge_to_shape(&edge.inner)),
        )
    }

    pub fn is_deleted_edge(&mut self, edge: &Edge) -> bool {
        self.history
            .pin_mut()
            .is_deleted(ffi::cast_edge_to_shape(&edge.inner))
    }

    pub fn modified_face(&mut self, face: &Face) -> Vec<Face> {
        super::boolean_shape::faces(
            self.history
                .pin_mut()
                .modified(ffi::cast_face_to_shape(&face.inner)),
        )
    }

    pub fn is_deleted_face(&mut self, face: &Face) -> bool {
        self.history
            .pin_mut()
            .is_deleted(ffi::cast_face_to_shape(&face.inner))
    }
}

/// A same-domain unify pass that built, with its history alive: the merged
/// shape, and the answer to what any input face or edge became.
pub struct Unification {
    pub shape: Shape,
    history: UniquePtr<history::ParcadUnify>,
}

impl Unification {
    pub fn modified_edge(&self, edge: &Edge) -> Vec<Edge> {
        super::boolean_shape::edges(self.history.modified(ffi::cast_edge_to_shape(&edge.inner)))
    }

    pub fn is_deleted_edge(&self, edge: &Edge) -> bool {
        self.history.is_deleted(ffi::cast_edge_to_shape(&edge.inner))
    }

    pub fn modified_face(&self, face: &Face) -> Vec<Face> {
        super::boolean_shape::faces(self.history.modified(ffi::cast_face_to_shape(&face.inner)))
    }

    pub fn is_deleted_face(&self, face: &Face) -> bool {
        self.history.is_deleted(ffi::cast_face_to_shape(&face.inner))
    }
}

/// The kernel's own words for a treatment that did not build: what `Build`
/// raised, or the quiet not-done when it raised nothing.
fn treatment_failure(treatment: &UniquePtr<history::ParcadEdgeTreatment>) -> String {
    let raised = treatment.failure();
    if raised.is_empty() {
        "the builder reported the command not done and raised nothing".into()
    } else {
        raised
    }
}
