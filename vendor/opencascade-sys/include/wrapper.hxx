#include "rust/cxx.h"
#include <sstream>
#include <BOPAlgo_GlueEnum.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <Bnd_Box.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <ShapeFix_Shape.hxx>
#include <BRepFeat_MakeCylindricalHole.hxx>
#include <BRepFeat_MakeDPrism.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepFilletAPI_MakeFillet2d.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepIntCurveSurface_Inter.hxx>
#include <BRepLib.hxx>
#include <BRepLib_ToolTriangulatedShape.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <GCE2d_MakeSegment.hxx>
#include <GCPnts_TangentialDeflection.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GC_MakeSegment.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Ellipse.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_BezierSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Plane.hxx>
#include <Geom_Surface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <Poly_Connect.hxx>
#include <STEPControl_Reader.hxx>
#include <STEPControl_Writer.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_Type.hxx>
#include <StlAPI_Writer.hxx>
// OCCT 8.0 moved the NCollection typedef aliases to src/Deprecated and stopped
// pulling them in transitively. They still ship, but every one now has to be
// included where it is used.
#include <BRepCheck_ListOfStatus.hxx>
#include <TColgp_Array1OfDir.hxx>
#include <TColgp_Array2OfPnt.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Lin.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

// Generic template constructor
template <typename T, typename... Args> std::unique_ptr<T> construct_unique(Args... args) {
  return std::unique_ptr<T>(new T(args...));
}

// Generic List
template <typename T> std::unique_ptr<std::vector<T>> list_to_vector(const NCollection_List<T> &list) {
  return std::unique_ptr<std::vector<T>>(new std::vector<T>(list.begin(), list.end()));
}

// Handles
typedef opencascade::handle<Standard_Type> HandleStandardType;
typedef opencascade::handle<Geom_Curve> HandleGeomCurve;
typedef opencascade::handle<Geom_TrimmedCurve> HandleGeomTrimmedCurve;
typedef opencascade::handle<Geom_Surface> HandleGeomSurface;
typedef opencascade::handle<Geom_BezierSurface> HandleGeomBezierSurface;
typedef opencascade::handle<Geom_Plane> HandleGeomPlane;
typedef opencascade::handle<Geom2d_Curve> HandleGeom2d_Curve;
typedef opencascade::handle<Geom2d_Ellipse> HandleGeom2d_Ellipse;
typedef opencascade::handle<Geom2d_TrimmedCurve> HandleGeom2d_TrimmedCurve;
typedef opencascade::handle<Geom_CylindricalSurface> HandleGeom_CylindricalSurface;
typedef opencascade::handle<Poly_Triangulation> Handle_Poly_Triangulation;

// Handle stuff
template <typename T> const T &handle_try_deref(const opencascade::handle<T> &handle) {
  if (handle.IsNull()) {
    throw std::runtime_error("null handle dereference");
  }
  return *handle;
}

inline const HandleStandardType &DynamicType(const HandleGeomSurface &surface) { return surface->DynamicType(); }

inline rust::String type_name(const HandleStandardType &handle) { return std::string(handle->Name()); }

inline std::unique_ptr<gp_Pnt> HandleGeomCurve_Value(const HandleGeomCurve &curve, const Standard_Real U) {
  return std::unique_ptr<gp_Pnt>(new gp_Pnt(curve->Value(U)));
}

inline std::unique_ptr<gp_Pnt> GCPnts_TangentialDeflection_Value(const GCPnts_TangentialDeflection &approximator,
                                                                 Standard_Integer i) {
  return std::unique_ptr<gp_Pnt>(new gp_Pnt(approximator.Value(i)));
}

inline std::unique_ptr<HandleGeomPlane> new_HandleGeomPlane_from_HandleGeomSurface(const HandleGeomSurface &surface) {
  HandleGeomPlane plane_handle = opencascade::handle<Geom_Plane>::DownCast(surface);
  return std::unique_ptr<HandleGeomPlane>(new opencascade::handle<Geom_Plane>(plane_handle));
}

// Collections
inline void shape_list_append_face(TopTools_ListOfShape &list, const TopoDS_Face &face) { list.Append(face); }

// Geometry
inline const gp_Pnt &handle_geom_plane_location(const HandleGeomPlane &plane) { return plane->Location(); }

inline std::unique_ptr<HandleGeom_CylindricalSurface> Geom_CylindricalSurface_ctor(const gp_Ax3 &axis, double radius) {
  return std::unique_ptr<HandleGeom_CylindricalSurface>(
      new opencascade::handle<Geom_CylindricalSurface>(new Geom_CylindricalSurface(axis, radius)));
}

inline std::unique_ptr<HandleGeomSurface> cylinder_to_surface(const HandleGeom_CylindricalSurface &cylinder_handle) {
  return std::unique_ptr<HandleGeomSurface>(new opencascade::handle<Geom_Surface>(cylinder_handle));
}

inline std::unique_ptr<HandleGeomBezierSurface> Geom_BezierSurface_ctor(const TColgp_Array2OfPnt &poles) {
  return std::unique_ptr<HandleGeomBezierSurface>(
      new opencascade::handle<Geom_BezierSurface>(new Geom_BezierSurface(poles)));
}

inline std::unique_ptr<HandleGeomSurface> bezier_to_surface(const HandleGeomBezierSurface &bezier_handle) {
  return std::unique_ptr<HandleGeomSurface>(new opencascade::handle<Geom_Surface>(bezier_handle));
}

inline std::unique_ptr<HandleGeom2d_Ellipse> Geom2d_Ellipse_ctor(const gp_Ax2d &axis, double major_radius,
                                                                 double minor_radius) {
  return std::unique_ptr<HandleGeom2d_Ellipse>(
      new opencascade::handle<Geom2d_Ellipse>(new Geom2d_Ellipse(axis, major_radius, minor_radius)));
}

inline std::unique_ptr<HandleGeom2d_Curve> ellipse_to_HandleGeom2d_Curve(const HandleGeom2d_Ellipse &ellipse_handle) {
  return std::unique_ptr<HandleGeom2d_Curve>(new opencascade::handle<Geom2d_Curve>(ellipse_handle));
}

inline std::unique_ptr<HandleGeom2d_TrimmedCurve> Geom2d_TrimmedCurve_ctor(const HandleGeom2d_Curve &curve, double u1,
                                                                           double u2) {
  return std::unique_ptr<HandleGeom2d_TrimmedCurve>(
      new opencascade::handle<Geom2d_TrimmedCurve>(new Geom2d_TrimmedCurve(curve, u1, u2)));
}

inline std::unique_ptr<HandleGeom2d_Curve>
HandleGeom2d_TrimmedCurve_to_curve(const HandleGeom2d_TrimmedCurve &trimmed_curve) {
  return std::unique_ptr<HandleGeom2d_Curve>(new opencascade::handle<Geom2d_Curve>(trimmed_curve));
}

inline std::unique_ptr<gp_Pnt2d> ellipse_value(const HandleGeom2d_Ellipse &ellipse, double u) {
  return std::unique_ptr<gp_Pnt2d>(new gp_Pnt2d(ellipse->Value(u)));
}

// Segment Stuff
inline std::unique_ptr<HandleGeomTrimmedCurve> GC_MakeSegment_Value(const GC_MakeSegment &segment) {
  return std::unique_ptr<HandleGeomTrimmedCurve>(new opencascade::handle<Geom_TrimmedCurve>(segment.Value()));
}

inline std::unique_ptr<HandleGeom2d_TrimmedCurve> GCE2d_MakeSegment_point_point(const gp_Pnt2d &p1,
                                                                                const gp_Pnt2d &p2) {
  return std::unique_ptr<HandleGeom2d_TrimmedCurve>(
      new opencascade::handle<Geom2d_TrimmedCurve>(GCE2d_MakeSegment(p1, p2)));
}

// Arc stuff
inline std::unique_ptr<HandleGeomTrimmedCurve> GC_MakeArcOfCircle_Value(const GC_MakeArcOfCircle &arc) {
  return std::unique_ptr<HandleGeomTrimmedCurve>(new opencascade::handle<Geom_TrimmedCurve>(arc.Value()));
}

inline std::unique_ptr<gp_Pnt> BRepAdaptor_Curve_value(const BRepAdaptor_Curve &curve, const Standard_Real U) {
  return std::unique_ptr<gp_Pnt>(new gp_Pnt(curve.Value(U)));
}

// BRepLib
inline bool BRepLibBuildCurves3d(const TopoDS_Shape &shape) { return BRepLib::BuildCurves3d(shape); }

inline void MakeThickSolidByJoin(BRepOffsetAPI_MakeThickSolid &make_thick_solid, const TopoDS_Shape &shape,
                                 const TopTools_ListOfShape &closing_faces, const Standard_Real offset,
                                 const Standard_Real tolerance) {
  make_thick_solid.MakeThickSolidByJoin(shape, closing_faces, offset, tolerance);
}

// Geometric processing
inline const gp_Ax1 &gp_OX() { return gp::OX(); }
inline const gp_Ax1 &gp_OY() { return gp::OY(); }
inline const gp_Ax1 &gp_OZ() { return gp::OZ(); }

inline const gp_Dir &gp_DZ() { return gp::DZ(); }

inline std::unique_ptr<gp_Ax1> gp_Ax1_ctor(const gp_Pnt &origin, const gp_Dir &main_dir) {
  return std::unique_ptr<gp_Ax1>(new gp_Ax1(origin, main_dir));
}

inline std::unique_ptr<gp_Ax2> gp_Ax2_ctor(const gp_Pnt &origin, const gp_Dir &main_dir) {
  return std::unique_ptr<gp_Ax2>(new gp_Ax2(origin, main_dir));
}

inline std::unique_ptr<gp_Ax3> gp_Ax3_from_gp_Ax2(const gp_Ax2 &axis) {
  return std::unique_ptr<gp_Ax3>(new gp_Ax3(axis));
}

inline std::unique_ptr<gp_Dir> gp_Dir_ctor(double x, double y, double z) {
  return std::unique_ptr<gp_Dir>(new gp_Dir(x, y, z));
}

inline std::unique_ptr<gp_Dir2d> gp_Dir2d_ctor(double x, double y) {
  return std::unique_ptr<gp_Dir2d>(new gp_Dir2d(x, y));
}

inline std::unique_ptr<gp_Ax2d> gp_Ax2d_ctor(const gp_Pnt2d &point, const gp_Dir2d &dir) {
  return std::unique_ptr<gp_Ax2d>(new gp_Ax2d(point, dir));
}

// Shape stuff
inline const TopoDS_Vertex &TopoDS_cast_to_vertex(const TopoDS_Shape &shape) { return TopoDS::Vertex(shape); }
inline const TopoDS_Edge &TopoDS_cast_to_edge(const TopoDS_Shape &shape) { return TopoDS::Edge(shape); }
inline const TopoDS_Wire &TopoDS_cast_to_wire(const TopoDS_Shape &shape) { return TopoDS::Wire(shape); }
inline const TopoDS_Face &TopoDS_cast_to_face(const TopoDS_Shape &shape) { return TopoDS::Face(shape); }
inline const TopoDS_Solid &TopoDS_cast_to_solid(const TopoDS_Shape &shape) { return TopoDS::Solid(shape); }
inline const TopoDS_Compound &TopoDS_cast_to_compound(const TopoDS_Shape &shape) { return TopoDS::Compound(shape); }

inline const TopoDS_Shape &cast_vertex_to_shape(const TopoDS_Vertex &vertex) { return vertex; }
inline const TopoDS_Shape &cast_edge_to_shape(const TopoDS_Edge &edge) { return edge; }
inline const TopoDS_Shape &cast_wire_to_shape(const TopoDS_Wire &wire) { return wire; }
inline const TopoDS_Shape &cast_face_to_shape(const TopoDS_Face &face) { return face; }
inline const TopoDS_Shape &cast_solid_to_shape(const TopoDS_Solid &solid) { return solid; }
inline const TopoDS_Shape &cast_compound_to_shape(const TopoDS_Compound &compound) { return compound; }

// Compound shapes
inline std::unique_ptr<TopoDS_Shape> TopoDS_Compound_as_shape(std::unique_ptr<TopoDS_Compound> compound) {
  return compound;
}

inline const TopoDS_Builder &BRep_Builder_upcast_to_topods_builder(const BRep_Builder &builder) { return builder; }

// Transforms
inline std::unique_ptr<HandleGeomSurface> BRep_Tool_Surface(const TopoDS_Face &face) {
  return std::unique_ptr<HandleGeomSurface>(new opencascade::handle<Geom_Surface>(BRep_Tool::Surface(face)));
}

inline std::unique_ptr<HandleGeomCurve> BRep_Tool_Curve(const TopoDS_Edge &edge, Standard_Real &first,
                                                        Standard_Real &last) {
  return std::unique_ptr<HandleGeomCurve>(new opencascade::handle<Geom_Curve>(BRep_Tool::Curve(edge, first, last)));
}

inline std::unique_ptr<gp_Pnt> BRep_Tool_Pnt(const TopoDS_Vertex &vertex) {
  return std::unique_ptr<gp_Pnt>(new gp_Pnt(BRep_Tool::Pnt(vertex)));
}

inline std::unique_ptr<gp_Trsf> TopLoc_Location_Transformation(const TopLoc_Location &location) {
  return std::unique_ptr<gp_Trsf>(new gp_Trsf(location.Transformation()));
}

inline std::unique_ptr<Handle_Poly_Triangulation> BRep_Tool_Triangulation(const TopoDS_Face &face,
                                                                          TopLoc_Location &location) {
  return std::unique_ptr<Handle_Poly_Triangulation>(
      new opencascade::handle<Poly_Triangulation>(BRep_Tool::Triangulation(face, location)));
}

inline std::unique_ptr<TopoDS_Shape> ExplorerCurrentShape(const TopExp_Explorer &explorer) {
  return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape(explorer.Current()));
}

inline std::unique_ptr<TopoDS_Vertex> TopExp_FirstVertex(const TopoDS_Edge &edge) {
  return std::unique_ptr<TopoDS_Vertex>(new TopoDS_Vertex(TopExp::FirstVertex(edge)));
}

inline std::unique_ptr<TopoDS_Vertex> TopExp_LastVertex(const TopoDS_Edge &edge) {
  return std::unique_ptr<TopoDS_Vertex>(new TopoDS_Vertex(TopExp::LastVertex(edge)));
}

inline void TopExp_EdgeVertices(const TopoDS_Edge &edge, TopoDS_Vertex &vertex1, TopoDS_Vertex &vertex2) {
  return TopExp::Vertices(edge, vertex1, vertex2);
}

inline void TopExp_WireVertices(const TopoDS_Wire &wire, TopoDS_Vertex &vertex1, TopoDS_Vertex &vertex2) {
  return TopExp::Vertices(wire, vertex1, vertex2);
}

inline bool TopExp_CommonVertex(const TopoDS_Edge &edge1, const TopoDS_Edge &edge2, TopoDS_Vertex &vertex) {
  return TopExp::CommonVertex(edge1, edge2, vertex);
}

inline std::unique_ptr<TopoDS_Face> BRepIntCurveSurface_Inter_face(const BRepIntCurveSurface_Inter &intersector) {
  return std::unique_ptr<TopoDS_Face>(new TopoDS_Face(intersector.Face()));
}

inline std::unique_ptr<gp_Pnt> BRepIntCurveSurface_Inter_point(const BRepIntCurveSurface_Inter &intersector) {
  return std::unique_ptr<gp_Pnt>(new gp_Pnt(intersector.Pnt()));
}

// BRepFeat
inline std::unique_ptr<BRepFeat_MakeCylindricalHole> BRepFeat_MakeCylindricalHole_ctor() {
  return std::unique_ptr<BRepFeat_MakeCylindricalHole>(new BRepFeat_MakeCylindricalHole());
}

// Data Import
inline IFSelect_ReturnStatus read_step(STEPControl_Reader &reader, rust::String theFileName) {
  return reader.ReadFile(theFileName.c_str());
}

inline std::unique_ptr<TopoDS_Shape> one_shape(const STEPControl_Reader &reader) {
  return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape(reader.OneShape()));
}

// Data Export
inline IFSelect_ReturnStatus transfer_shape(STEPControl_Writer &writer, const TopoDS_Shape &theShape) {
  return writer.Transfer(theShape, STEPControl_AsIs);
}

inline IFSelect_ReturnStatus write_step(STEPControl_Writer &writer, rust::String theFileName) {
  return writer.Write(theFileName.c_str());
}

inline bool write_stl(StlAPI_Writer &writer, const TopoDS_Shape &theShape, rust::String theFileName) {
  return writer.Write(theShape, theFileName.c_str());
}

inline std::unique_ptr<gp_Dir> Poly_Triangulation_Normal(const Poly_Triangulation &triangulation,
                                                         const Standard_Integer index) {
  return std::unique_ptr<gp_Dir>(new gp_Dir(triangulation.Normal(index)));
}

inline std::unique_ptr<gp_Pnt> Poly_Triangulation_Node(const Poly_Triangulation &triangulation,
                                                       const Standard_Integer index) {
  return std::unique_ptr<gp_Pnt>(new gp_Pnt(triangulation.Node(index)));
}

inline std::unique_ptr<gp_Pnt2d> Poly_Triangulation_UV(const Poly_Triangulation &triangulation,
                                                       const Standard_Integer index) {
  return std::unique_ptr<gp_Pnt2d>(new gp_Pnt2d(triangulation.UVNode(index)));
}

inline void compute_normals(const TopoDS_Face &face, const Handle(Poly_Triangulation) & triangulation) {
  BRepLib_ToolTriangulatedShape::ComputeNormals(face, triangulation);
}

// Shape Properties
inline std::unique_ptr<gp_Pnt> GProp_GProps_CentreOfMass(const GProp_GProps &props) {
  return std::unique_ptr<gp_Pnt>(new gp_Pnt(props.CentreOfMass()));
}

inline void BRepGProp_LinearProperties(const TopoDS_Shape &shape, GProp_GProps &props) {
  BRepGProp::LinearProperties(shape, props);
}

inline void BRepGProp_SurfaceProperties(const TopoDS_Shape &shape, GProp_GProps &props) {
  BRepGProp::SurfaceProperties(shape, props);
}

inline void BRepGProp_VolumeProperties(const TopoDS_Shape &shape, GProp_GProps &props) {
  BRepGProp::VolumeProperties(shape, props);
}

// Fillets
inline std::unique_ptr<TopoDS_Edge> BRepFilletAPI_MakeFillet2d_add_fillet(BRepFilletAPI_MakeFillet2d &make_fillet,
                                                                          const TopoDS_Vertex &vertex,
                                                                          Standard_Real radius) {
  return std::unique_ptr<TopoDS_Edge>(new TopoDS_Edge(make_fillet.AddFillet(vertex, radius)));
}

// Chamfers
inline std::unique_ptr<TopoDS_Edge>
BRepFilletAPI_MakeFillet2d_add_chamfer(BRepFilletAPI_MakeFillet2d &make_fillet, const TopoDS_Edge &edge1,
                                       const TopoDS_Edge &edge2, const Standard_Real dist1, const Standard_Real dist2) {
  return std::unique_ptr<TopoDS_Edge>(new TopoDS_Edge(make_fillet.AddChamfer(edge1, edge2, dist1, dist2)));
}

inline std::unique_ptr<TopoDS_Edge>
BRepFilletAPI_MakeFillet2d_add_chamfer_angle(BRepFilletAPI_MakeFillet2d &make_fillet, const TopoDS_Edge &edge,
                                             const TopoDS_Vertex &vertex, const Standard_Real dist,
                                             const Standard_Real angle) {
  return std::unique_ptr<TopoDS_Edge>(new TopoDS_Edge(make_fillet.AddChamfer(edge, vertex, dist, angle)));
}

// BRepTools
inline std::unique_ptr<TopoDS_Wire> outer_wire(const TopoDS_Face &face) {
  return std::unique_ptr<TopoDS_Wire>(new TopoDS_Wire(BRepTools::OuterWire(face)));
}

// Collections
inline void map_shapes(const TopoDS_Shape &S, const TopAbs_ShapeEnum T, TopTools_IndexedMapOfShape &M) {
  TopExp::MapShapes(S, T, M);
}

inline void map_shapes_and_ancestors(const TopoDS_Shape &S, const TopAbs_ShapeEnum TS, const TopAbs_ShapeEnum TA,
                                     TopTools_IndexedDataMapOfShapeListOfShape &M) {
  TopExp::MapShapesAndAncestors(S, TS, TA, M);
}

inline void map_shapes_and_unique_ancestors(const TopoDS_Shape &S, const TopAbs_ShapeEnum TS, const TopAbs_ShapeEnum TA,
                                            TopTools_IndexedDataMapOfShapeListOfShape &M) {
  TopExp::MapShapesAndUniqueAncestors(S, TS, TA, M);
}

inline std::unique_ptr<gp_Dir> TColgp_Array1OfDir_Value(const TColgp_Array1OfDir &array, Standard_Integer index) {
  return std::unique_ptr<gp_Dir>(new gp_Dir(array.Value(index)));
}

// ShapeFix: OpenCASCADE's shape-healing pass, absent from the upstream crate.
//
// Added for parcad to test whether a fillet result the kernel itself rejects
// can be repaired rather than refused. `precision` and `max_tolerance` bound
// how far vertices are allowed to move; pass them explicitly so a caller can
// prove the repair did not reshape the part.
inline std::unique_ptr<TopoDS_Shape> ShapeFix_repair(const TopoDS_Shape &shape, double precision,
                                                     double max_tolerance) {
  ShapeFix_Shape fixer(shape);
  fixer.SetPrecision(precision);
  fixer.SetMaxTolerance(max_tolerance);
  fixer.Perform();
  return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape(fixer.Shape()));
}

// Topology report: every face -> wire -> edge -> vertex, with geometry types,
// bounds and tolerances. Added for parcad as a diagnostic; the BRepCheck report
// below says *what* is wrong, this says what the kernel actually built, which
// is the evidence a fix has to start from.
//
// Each wire is listed twice on purpose: first raw (every edge the wire
// contains, with 3D and UV endpoints — both pcurves for a closed edge), then
// as far as BRepTools_WireExplorer can traverse it. A wire whose raw list is
// longer than its traversal is connectable evidence of exactly where a
// rebuilt boundary went wrong; that difference is what located the tangent
// pinch defect in the fillet corner code.
inline rust::String Shape_topology_report(const TopoDS_Shape &shape) {
  std::ostringstream out;
  out.precision(10);
  int fi = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next(), ++fi) {
    const TopoDS_Face &face = TopoDS::Face(f.Current());
    const Handle(Geom_Surface) &surf = BRep_Tool::Surface(face);
    out << "face " << fi << ": " << (surf.IsNull() ? "null" : surf->DynamicType()->Name())
        << (face.Orientation() == TopAbs_REVERSED ? " reversed" : "")
        << " tol " << BRep_Tool::Tolerance(face) << "\n";
    int wi = 0;
    for (TopExp_Explorer w(face, TopAbs_WIRE); w.More(); w.Next(), ++wi) {
      const TopoDS_Wire &wire = TopoDS::Wire(w.Current());
      int raw = 0;
      for (TopExp_Explorer re(wire, TopAbs_EDGE); re.More(); re.Next()) {
        ++raw;
      }
      out << "  wire " << wi << " (" << raw << " edges):\n";
      for (TopExp_Explorer re(wire, TopAbs_EDGE); re.More(); re.Next()) {
        const TopoDS_Edge &edge = TopoDS::Edge(re.Current());
        double u0 = 0.0, u1 = 0.0;
        Handle(Geom_Curve) curve = BRep_Tool::Curve(edge, u0, u1);
        out << "    raw " << (edge.Orientation() == TopAbs_REVERSED ? "rev " : "fwd ")
            << (curve.IsNull() ? "no-3d-curve" : curve->DynamicType()->Name());
        if (!curve.IsNull()) {
          gp_Pnt p0 = curve->Value(u0), p1 = curve->Value(u1);
          out << " [" << u0 << ".." << u1 << "]"
              << " (" << p0.X() << "," << p0.Y() << "," << p0.Z() << ")->("
              << p1.X() << "," << p1.Y() << "," << p1.Z() << ")";
        }
        double pf = 0.0, pl = 0.0;
        Handle(Geom2d_Curve) pc = BRep_Tool::CurveOnSurface(edge, face, pf, pl);
        if (!pc.IsNull()) {
          gp_Pnt2d q0 = pc->Value(pf), q1 = pc->Value(pl);
          out << " uv(" << q0.X() << "," << q0.Y() << ")->(" << q1.X() << "," << q1.Y() << ")";
        }
        if (BRep_Tool::IsClosed(edge, face)) {
          TopoDS_Edge redge = edge;
          redge.Reverse();
          Handle(Geom2d_Curve) pc2 = BRep_Tool::CurveOnSurface(redge, face, pf, pl);
          if (!pc2.IsNull()) {
            gp_Pnt2d q0 = pc2->Value(pf), q1 = pc2->Value(pl);
            out << " uv2(" << q0.X() << "," << q0.Y() << ")->(" << q1.X() << "," << q1.Y() << ")";
          }
        }
        out << "\n";
      }
      for (BRepTools_WireExplorer e(wire, face); e.More(); e.Next()) {
        const TopoDS_Edge &edge = e.Current();
        double u0 = 0.0, u1 = 0.0;
        Handle(Geom_Curve) curve = BRep_Tool::Curve(edge, u0, u1);
        out << "    edge " << (edge.Orientation() == TopAbs_REVERSED ? "rev " : "fwd ")
            << (curve.IsNull() ? "no-3d-curve" : curve->DynamicType()->Name())
            << " tol " << BRep_Tool::Tolerance(edge);
        if (BRep_Tool::Degenerated(edge)) out << " DEGENERATED";
        if (BRep_Tool::IsClosed(edge, face)) out << " seam";
        if (!curve.IsNull()) {
          gp_Pnt p0 = curve->Value(u0), p1 = curve->Value(u1);
          out << " [" << u0 << ".." << u1 << "]"
              << " (" << p0.X() << "," << p0.Y() << "," << p0.Z() << ")->("
              << p1.X() << "," << p1.Y() << "," << p1.Z() << ")";
        }
        out << "\n";
        TopoDS_Vertex v0, v1;
        TopExp::Vertices(edge, v0, v1);
        if (!v0.IsNull()) {
          gp_Pnt p = BRep_Tool::Pnt(v0);
          out << "      v0 (" << p.X() << "," << p.Y() << "," << p.Z() << ") tol "
              << BRep_Tool::Tolerance(v0) << "\n";
        }
        if (!v1.IsNull()) {
          gp_Pnt p = BRep_Tool::Pnt(v1);
          out << "      v1 (" << p.X() << "," << p.Y() << "," << p.Z() << ") tol "
              << BRep_Tool::Tolerance(v1) << "\n";
        }
      }
    }
  }
  return rust::String(out.str());
}

// Native BREP dump, exact topology preserved (STEP normalises it away).
// Added for parcad as a diagnostic.
inline bool BRepTools_write_brep(const TopoDS_Shape &shape, rust::String path) {
  return BRepTools::Write(shape, path.c_str());
}

// BRepCheck: OpenCASCADE's own answer to "is this shape actually valid?".
//
// Bound as one report-producing call rather than as the class, because the bool
// from IsValid() is not actionable on its own: what you need is *which*
// sub-shape is wrong and *why*, and reaching BRepCheck_Result from Rust would
// mean binding several more OCCT collection types to learn the same thing.
//
// Returns "" for a valid shape, otherwise one "<kind> <n>: <status>" line per
// fault. The index is the position in a TopExp_Explorer walk of that kind, which
// is the same order the rest of this wrapper enumerates sub-shapes in.
//
// `exact` enables per-point checking. It is slow, and off by default in OCCT,
// which is why a face carrying a wrecked surface can pass the cheap check.
inline rust::String BRepCheck_report(const TopoDS_Shape &shape, bool exact) {
  BRepCheck_Analyzer analyzer(shape, Standard_True, Standard_False, exact);
  if (analyzer.IsValid()) {
    return rust::String("");
  }

  const TopAbs_ShapeEnum kinds[] = {TopAbs_SOLID, TopAbs_SHELL, TopAbs_FACE,
                                    TopAbs_WIRE,  TopAbs_EDGE,  TopAbs_VERTEX};
  const char *names[] = {"solid", "shell", "face", "wire", "edge", "vertex"};

  std::ostringstream out;
  for (int kind = 0; kind < 6; ++kind) {
    int index = 0;
    for (TopExp_Explorer it(shape, kinds[kind]); it.More(); it.Next(), ++index) {
      if (analyzer.IsValid(it.Current())) {
        continue;
      }
      const Handle(BRepCheck_Result) &result = analyzer.Result(it.Current());
      if (result.IsNull()) {
        continue;
      }
      for (BRepCheck_ListIteratorOfListOfStatus status(result->Status()); status.More(); status.Next()) {
        if (status.Value() == BRepCheck_NoError) {
          continue;
        }
        out << names[kind] << " " << index << ": ";
        BRepCheck::Print(status.Value(), out); // writes the enum name and a newline
        out.seekp(-1, std::ios_base::end);     // reclaim that newline

        // Say what the offending sub-shape actually is. A status code alone
        // does not distinguish "the kernel mangled one face" from "the whole
        // solid is nonsense", and that is the first thing you want to know.
        if (kinds[kind] == TopAbs_FACE) {
          const TopoDS_Face &face = TopoDS::Face(it.Current());
          const Handle(Geom_Surface) &surface = BRep_Tool::Surface(face);
          if (!surface.IsNull()) {
            out << " on " << surface->DynamicType()->Name();
          }
        }
        Bnd_Box box;
        BRepBndLib::Add(it.Current(), box);
        if (!box.IsVoid()) {
          double x0, y0, z0, x1, y1, z1;
          box.Get(x0, y0, z0, x1, y1, z1);
          out << " [" << x0 << " " << y0 << " " << z0 << " .. " << x1 << " " << y1 << " " << z1
              << "]";
        }
        out << "\n";
      }
    }
  }
  return rust::String(out.str());
}
