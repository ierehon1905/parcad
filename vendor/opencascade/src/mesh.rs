use crate::primitives::{FaceOrientation, Shape};
use cxx::UniquePtr;
use glam::{dvec2, dvec3, DVec2, DVec3};
use opencascade_sys::ffi;

/// Which face of the shape a run of triangles came from.
///
/// `start` and `count` are in triangles, not indices, so a caller that has a
/// triangle number from a raycast can find its face without dividing by three.
#[derive(Debug, Clone, Copy)]
pub struct FaceRun {
    /// The face's position in the shape's own face traversal — the order
    /// `Shape::faces()` walks, which is the order everything else that counts
    /// faces uses too.
    ///
    /// Counted across the faces this mesher skips, not derived from the run's
    /// position in `Mesh::faces`. A face whose triangulation is missing
    /// contributes no run, so the two numberings part company on exactly the
    /// shapes where saying "face 7" wrongly would be hardest to notice: the
    /// count still looks plausible and every face after the gap is named as
    /// its neighbour.
    pub face: usize,
    pub start: usize,
    pub count: usize,
}

#[derive(Debug)]
pub struct Mesh {
    pub vertices: Vec<DVec3>,
    pub uvs: Vec<DVec2>,
    pub normals: Vec<DVec3>,
    pub indices: Vec<usize>,
    /// One run per *triangulated* face of the meshed shape, in traversal order.
    ///
    /// The mesher already triangulates face by face and concatenates the
    /// results; this records where each face's triangles landed instead of
    /// throwing that away. Without it a triangle in the finished buffer has no
    /// way back to the `TopoDS_Face` it came from, which is what a viewer needs
    /// to say "you are pointing at *this* face" rather than "you are pointing
    /// at the solid".
    ///
    /// Shorter than the shape's face count when a face carried no
    /// triangulation. Read `FaceRun::face` for which face a run is; the run's
    /// own position is not that number.
    pub faces: Vec<FaceRun>,
}

pub struct Mesher {
    inner: UniquePtr<ffi::BRepMesh_IncrementalMesh>,
}

impl Mesher {
    pub fn new(shape: &Shape) -> Self {
        let inner = ffi::BRepMesh_IncrementalMesh_ctor(&shape.inner, 0.01);

        if !inner.IsDone() {
            // TODO(bschwind) - Add proper Error type and return Result.
            panic!("Call to ffi::BRepMesh_IncrementalMesh_ctor failed");
        }

        Self { inner }
    }

    pub fn mesh(mut self) -> Mesh {
        let mut vertices = vec![];
        let mut uvs = vec![];
        let mut normals = vec![];
        let mut indices = vec![];
        let mut faces = vec![];

        let triangulated_shape = ffi::TopoDS_Shape_to_owned(self.inner.pin_mut().Shape());
        let triangulated_shape = Shape { inner: triangulated_shape };

        for (face_index, face) in triangulated_shape.faces().enumerate() {
            let mut location = ffi::TopLoc_Location_ctor();

            let triangulation_handle =
                ffi::BRep_Tool_Triangulation(&face.inner, location.pin_mut());

            let Ok(triangulation) = ffi::Handle_Poly_Triangulation_Get(&triangulation_handle)
            else {
                // TODO(bschwind) - Do better error handling, use Results.
                println!("Encountered a face with no triangulation");
                continue;
            };

            let index_offset = vertices.len();
            let face_point_count = triangulation.NbNodes();

            for i in 1..=face_point_count {
                let mut point = ffi::Poly_Triangulation_Node(triangulation, i);
                point.pin_mut().Transform(&ffi::TopLoc_Location_Transformation(&location));
                vertices.push(dvec3(point.X(), point.Y(), point.Z()));
            }

            let mut u_min = f64::INFINITY;
            let mut v_min = f64::INFINITY;

            let mut u_max = f64::NEG_INFINITY;
            let mut v_max = f64::NEG_INFINITY;

            for i in 1..=(face_point_count) {
                let uv = ffi::Poly_Triangulation_UV(triangulation, i);
                let (u, v) = (uv.X(), uv.Y());

                u_min = u_min.min(u);
                v_min = v_min.min(v);

                u_max = u_max.max(u);
                v_max = v_max.max(v);

                uvs.push(dvec2(u, v));
            }

            // Normalize the newly added UV coordinates.
            for uv in &mut uvs[index_offset..(index_offset + face_point_count as usize)] {
                uv.x = (uv.x - u_min) / (u_max - u_min);
                uv.y = (uv.y - v_min) / (v_max - v_min);

                if face.orientation() != FaceOrientation::Forward {
                    uv.x = 1.0 - uv.x;
                }
            }

            // Add in the normals.
            // TODO(bschwind) - Use `location` to transform the normals.
            let normal_array = ffi::TColgp_Array1OfDir_ctor(0, face_point_count);

            ffi::compute_normals(&face.inner, &triangulation_handle);

            // TODO(bschwind) - Why do we start at 1 here?
            for i in 1..(normal_array.Length() as usize) {
                let normal = ffi::Poly_Triangulation_Normal(triangulation, i as i32);
                normals.push(dvec3(normal.X(), normal.Y(), normal.Z()));
            }

            let first_triangle = indices.len() / 3;
            for i in 1..=triangulation.NbTriangles() {
                let triangle = triangulation.Triangle(i);

                if face.orientation() == FaceOrientation::Forward {
                    indices.push(index_offset + triangle.Value(1) as usize - 1);
                    indices.push(index_offset + triangle.Value(2) as usize - 1);
                    indices.push(index_offset + triangle.Value(3) as usize - 1);
                } else {
                    indices.push(index_offset + triangle.Value(3) as usize - 1);
                    indices.push(index_offset + triangle.Value(2) as usize - 1);
                    indices.push(index_offset + triangle.Value(1) as usize - 1);
                }
            }
            faces.push(FaceRun {
                face: face_index,
                start: first_triangle,
                count: indices.len() / 3 - first_triangle,
            });
        }

        Mesh { vertices, uvs, normals, indices, faces }
    }
}
