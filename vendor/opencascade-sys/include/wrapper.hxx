#include "rust/cxx.h"
#include <BRepAdaptor_Surface.hxx>
#include <Geom_Line.hxx>
#include <NCollection_DataMap.hxx>
#include <ShapeBuild_Edge.hxx>
#include <set>
#include <sstream>
#include <BOPAlgo_GlueEnum.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <Extrema_ExtPC.hxx>
#include <Extrema_ExtPS.hxx>
#include <Precision.hxx>
#include <BRepAdaptor_Curve2d.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <Poly_Triangulation.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <array>
#include <map>
#include <algorithm>
#include <cmath>
#include <memory>
#include <vector>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepBuilderAPI_GTransform.hxx>
#include <gp_GTrsf.hxx>
#include <Bnd_Box.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <BOPAlgo_CheckerSI.hxx>
#include <BOPDS_DS.hxx>
#include <BOPDS_IteratorSI.hxx>
#include <IntTools_Context.hxx>
#include <IntTools_FaceFace.hxx>
#include <BOPAlgo_Alerts.hxx>
#include <OSD_Parallel.hxx>
#include <Standard_ErrorHandler.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <ShapeFix_Shape.hxx>
#include <BRepFeat_MakeCylindricalHole.hxx>
#include <BRepFeat_MakeDPrism.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepFilletAPI_MakeFillet2d.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepIntCurveSurface_Inter.hxx>
#include <BRepClass3d_SolidClassifier.hxx> // PARCAD: point-in-solid
#include <BRepTools_ReShape.hxx>
#include <GeomAdaptor_Curve.hxx>           // PARCAD: ray casting against a loaded shape
#include <BRepLib.hxx>
#include <BRepLib_ToolTriangulatedShape.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffsetAPI_MakePipe.hxx> // PARCAD: sweep a profile along a spine
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <GCPnts_TangentialDeflection.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GC_MakeSegment.hxx>
#include <GC_MakeSegment2d.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Ellipse.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_BezierSurface.hxx>
#include <Geom_Circle.hxx>
#include <Geom_ConicalSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Plane.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_SphericalSurface.hxx>
#include <Geom_Surface.hxx>
#include <Geom_ToroidalSurface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <Poly_Connect.hxx>
#include <STEPControl_Reader.hxx>
#include <STEPControl_Writer.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_Type.hxx>
#include <StlAPI_Writer.hxx>
// OCCT 8.0 deprecated the TColgp_/TopTools_/BRepCheck_ typedef aliases along
// with the headers that define them; these are the NCollection templates each
// one named as its replacement, aliased below because cxx can only name a type
// by a plain identifier.
#include <BRepCheck_Status.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp.hxx>
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

// Collection instantiations. OCCT's own aliases for these are deprecated.
typedef NCollection_List<TopoDS_Shape> ListOfShape;
typedef NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> IndexedMapOfShape;
typedef NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
    IndexedDataMapOfShapeListOfShape;
typedef NCollection_Array1<gp_Dir> Array1OfDir;
typedef NCollection_Array2<gp_Pnt> Array2OfPnt;

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

inline std::unique_ptr<gp_Pnt> HandleGeomCurve_Value(const HandleGeomCurve &curve, const double U) {
  return std::unique_ptr<gp_Pnt>(new gp_Pnt(curve->Value(U)));
}

inline std::unique_ptr<gp_Pnt> GCPnts_TangentialDeflection_Value(const GCPnts_TangentialDeflection &approximator,
                                                                 int i) {
  return std::unique_ptr<gp_Pnt>(new gp_Pnt(approximator.Value(i)));
}

inline std::unique_ptr<HandleGeomPlane> new_HandleGeomPlane_from_HandleGeomSurface(const HandleGeomSurface &surface) {
  HandleGeomPlane plane_handle = opencascade::handle<Geom_Plane>::DownCast(surface);
  return std::unique_ptr<HandleGeomPlane>(new opencascade::handle<Geom_Plane>(plane_handle));
}

// Collections
inline void shape_list_append_face(ListOfShape &list, const TopoDS_Face &face) { list.Append(face); }

// Geometry
inline const gp_Pnt &handle_geom_plane_location(const HandleGeomPlane &plane) { return plane->Location(); }

inline std::unique_ptr<HandleGeom_CylindricalSurface> Geom_CylindricalSurface_ctor(const gp_Ax3 &axis, double radius) {
  return std::unique_ptr<HandleGeom_CylindricalSurface>(
      new opencascade::handle<Geom_CylindricalSurface>(new Geom_CylindricalSurface(axis, radius)));
}

inline std::unique_ptr<HandleGeomSurface> cylinder_to_surface(const HandleGeom_CylindricalSurface &cylinder_handle) {
  return std::unique_ptr<HandleGeomSurface>(new opencascade::handle<Geom_Surface>(cylinder_handle));
}

inline std::unique_ptr<HandleGeomBezierSurface> Geom_BezierSurface_ctor(const Array2OfPnt &poles) {
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

inline std::unique_ptr<HandleGeom2d_TrimmedCurve> GC_MakeSegment2d_point_point(const gp_Pnt2d &p1,
                                                                                const gp_Pnt2d &p2) {
  return std::unique_ptr<HandleGeom2d_TrimmedCurve>(
      new opencascade::handle<Geom2d_TrimmedCurve>(GC_MakeSegment2d(p1, p2)));
}

// Arc stuff
inline std::unique_ptr<HandleGeomTrimmedCurve> GC_MakeArcOfCircle_Value(const GC_MakeArcOfCircle &arc) {
  return std::unique_ptr<HandleGeomTrimmedCurve>(new opencascade::handle<Geom_TrimmedCurve>(arc.Value()));
}

inline std::unique_ptr<gp_Pnt> BRepAdaptor_Curve_value(const BRepAdaptor_Curve &curve, const double U) {
  return std::unique_ptr<gp_Pnt>(new gp_Pnt(curve.Value(U)));
}

// BRepLib
inline bool BRepLibBuildCurves3d(const TopoDS_Shape &shape) { return BRepLib::BuildCurves3d(shape); }

inline void MakeThickSolidByJoin(BRepOffsetAPI_MakeThickSolid &make_thick_solid, const TopoDS_Shape &shape,
                                 const ListOfShape &closing_faces, const double offset,
                                 const double tolerance) {
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

inline std::unique_ptr<HandleGeomCurve> BRep_Tool_Curve(const TopoDS_Edge &edge, double &first,
                                                        double &last) {
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

// Ray casting against a loaded shape — added for parcad, see PARCAD-CHANGES.md.
// `Init(shape, line, tol)` reloads the face list for every line; a thickness
// sweep fires thousands of lines at one shape, so the load is split out.
inline void BRepIntCurveSurface_Inter_load(BRepIntCurveSurface_Inter &intersector, const TopoDS_Shape &shape,
                                           double tolerance) {
  intersector.Load(shape, tolerance);
}

inline void BRepIntCurveSurface_Inter_init_line(BRepIntCurveSurface_Inter &intersector, const gp_Lin &line) {
  occ::handle<Geom_Line> geom_line = new Geom_Line(line);
  GeomAdaptor_Curve curve(geom_line);
  intersector.Init(curve);
}

// Parameter of the current hit along the line, in the line's own units.
inline double BRepIntCurveSurface_Inter_w(const BRepIntCurveSurface_Inter &intersector) { return intersector.W(); }

// Which way the line crosses the *material* at the current hit: 0 entering,
// 1 leaving, 2 tangent. The intersector reports the crossing against the
// surface's own normal, and a reversed face's material lies on the other side
// of its surface, so the face orientation is folded in here.
inline int BRepIntCurveSurface_Inter_transition(const BRepIntCurveSurface_Inter &intersector) {
  const bool reversed = intersector.Face().Orientation() == TopAbs_REVERSED;
  switch (intersector.Transition()) {
    case IntCurveSurface_In:
      return reversed ? 1 : 0;
    case IntCurveSurface_Out:
      return reversed ? 0 : 1;
    default:
      return 2;
  }
}

// Where the current hit lies on its face: 0 inside the face, 1 on its
// boundary, 2 elsewhere (never reported by the iterator, kept for completeness).
inline int BRepIntCurveSurface_Inter_state(const BRepIntCurveSurface_Inter &intersector) {
  switch (intersector.State()) {
    case TopAbs_IN:
      return 0;
    case TopAbs_ON:
      return 1;
    default:
      return 2;
  }
}

// Position of a sub-shape in a map, 1-based as OCCT counts, 0 when absent —
// added for parcad, see PARCAD-CHANGES.md. `TopExp::MapShapes` over faces
// lists them in `TopExp_Explorer` order, so index-1 is the face number the
// mesher and the face report use.
inline int IndexedMapOfShape_find_index(const IndexedMapOfShape &map, const TopoDS_Shape &shape) {
  return map.FindIndex(shape);
}

// Point-in-solid — added for parcad, see PARCAD-CHANGES.md. 0 inside, 1
// outside, 2 on the boundary within `tolerance`, 3 undecidable.
inline int BRepClass3d_classify(const TopoDS_Shape &shape, double x, double y, double z, double tolerance) {
  BRepClass3d_SolidClassifier classifier(shape, gp_Pnt(x, y, z), tolerance);
  switch (classifier.State()) {
    case TopAbs_IN:
      return 0;
    case TopAbs_OUT:
      return 1;
    case TopAbs_ON:
      return 2;
    default:
      return 3;
  }
}

// Tight bounds of a shape from its exact geometry — added for parcad, see
// PARCAD-CHANGES.md. `BRepBndLib::AddOptimal` without triangulation and
// without tolerance enlargement. False for a shape with no extent.
inline bool Shape_bounds_optimal(const TopoDS_Shape &shape, double &x0, double &y0, double &z0, double &x1,
                                 double &y1, double &z1) {
  Bnd_Box box;
  BRepBndLib::AddOptimal(shape, box, /*useTriangulation*/ false, /*useShapeTolerance*/ false);
  if (box.IsVoid()) {
    return false;
  }
  box.Get(x0, y0, z0, x1, y1, z1);
  return true;
}

// Two boxes that bracket a shape's tight one, both cheap — added for parcad,
// see PARCAD-CHANGES.md. `outer` (x0..z1) encloses the shape, from control
// points and tolerances; `inner` (a0..c1) is the box of its triangulation's
// nodes, which lie on it, and is left untouched with `false` when a face has
// no triangulation.
inline bool Shape_bounds_bracket(const TopoDS_Shape &shape, double &x0, double &y0, double &z0, double &x1,
                                 double &y1, double &z1, double &a0, double &b0, double &c0, double &a1, double &b1,
                                 double &c1) {
  Bnd_Box outer;
  BRepBndLib::Add(shape, outer, /*useTriangulation*/ false);
  if (outer.IsVoid()) {
    return false;
  }
  outer.Get(x0, y0, z0, x1, y1, z1);
  Bnd_Box inner;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    const opencascade::handle<Poly_Triangulation> &tri = BRep_Tool::Triangulation(TopoDS::Face(it.Current()), location);
    if (tri.IsNull() || tri->NbNodes() == 0) {
      return true;
    }
    const gp_Trsf &trsf = location.Transformation();
    for (int i = 1; i <= tri->NbNodes(); ++i) {
      inner.Add(tri->Node(i).Transformed(trsf));
    }
  }
  if (inner.IsVoid()) {
    return true;
  }
  inner.SetGap(0.0);
  inner.Get(a0, b0, c0, a1, b1, c1);
  return true;
}

// Points on every face of a shape, a `per_side` by `per_side` grid over each
// face's parameter bounds kept where the face classifier puts them inside the
// face — added for parcad, see PARCAD-CHANGES.md. Flat x, y, z.
inline rust::Vec<double> Shape_face_grid(const TopoDS_Shape &shape, int per_side) {
  rust::Vec<double> out;
  for (TopExp_Explorer faces(shape, TopAbs_FACE); faces.More(); faces.Next()) {
    const TopoDS_Face face = TopoDS::Face(faces.Current());
    double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    BRepAdaptor_Surface surface(face);
    for (int i = 0; i <= per_side; ++i) {
      for (int j = 0; j <= per_side; ++j) {
        const gp_Pnt2d uv(u0 + (u1 - u0) * i / per_side, v0 + (v1 - v0) * j / per_side);
        BRepClass_FaceClassifier inside(face, uv, Precision::PConfusion());
        if (inside.State() == TopAbs_OUT) {
          continue;
        }
        const gp_Pnt p = surface.Value(uv.X(), uv.Y());
        out.push_back(p.X());
        out.push_back(p.Y());
        out.push_back(p.Z());
      }
    }
  }
  return out;
}

// Closest points on a face's own triangulation, and local projection onto its
// surface from there — added for parcad, see PARCAD-CHANGES.md. The
// triangulation lies within a measured slack of the surface, so a distance to
// it bounds the distance to the face from both sides, and its nearest
// triangles seed a Newton step that finds the surface's own nearest point
// without the sample grid `Extrema_GenExtPS` rebuilds for every point.
namespace parcad_proximity {

struct Bounds {
  double lo[3] = {1e300, 1e300, 1e300};
  double hi[3] = {-1e300, -1e300, -1e300};
  void add(const gp_XYZ &p) {
    const double c[3] = {p.X(), p.Y(), p.Z()};
    for (int i = 0; i < 3; ++i) {
      lo[i] = std::min(lo[i], c[i]);
      hi[i] = std::max(hi[i], c[i]);
    }
  }
  void add(const Bounds &b) {
    for (int i = 0; i < 3; ++i) {
      lo[i] = std::min(lo[i], b.lo[i]);
      hi[i] = std::max(hi[i], b.hi[i]);
    }
  }
  double distance_sq(const gp_XYZ &p) const {
    const double c[3] = {p.X(), p.Y(), p.Z()};
    double sum = 0.0;
    for (int i = 0; i < 3; ++i) {
      const double d = c[i] < lo[i] ? lo[i] - c[i] : (c[i] > hi[i] ? c[i] - hi[i] : 0.0);
      sum += d * d;
    }
    return sum;
  }
  double distance_sq(const Bounds &b) const {
    double sum = 0.0;
    for (int i = 0; i < 3; ++i) {
      const double d = std::max({0.0, b.lo[i] - hi[i], lo[i] - b.hi[i]});
      sum += d * d;
    }
    return sum;
  }
};

// The point of triangle (a, b, c) nearest p, as a + v (b - a) + w (c - a)
// (Ericson, Real-Time Collision Detection, 5.1.5).
inline void closest_on_triangle(const gp_XYZ &p, const gp_XYZ &a, const gp_XYZ &b, const gp_XYZ &c, double &v,
                                double &w) {
  const gp_XYZ ab = b - a, ac = c - a, ap = p - a;
  const double d1 = ab.Dot(ap), d2 = ac.Dot(ap);
  if (d1 <= 0.0 && d2 <= 0.0) {
    v = 0.0, w = 0.0;
    return;
  }
  const gp_XYZ bp = p - b;
  const double d3 = ab.Dot(bp), d4 = ac.Dot(bp);
  if (d3 >= 0.0 && d4 <= d3) {
    v = 1.0, w = 0.0;
    return;
  }
  const double vc = d1 * d4 - d3 * d2;
  if (vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0) {
    v = d1 / (d1 - d3), w = 0.0;
    return;
  }
  const gp_XYZ cp = p - c;
  const double d5 = ab.Dot(cp), d6 = ac.Dot(cp);
  if (d6 >= 0.0 && d5 <= d6) {
    v = 0.0, w = 1.0;
    return;
  }
  const double vb = d5 * d2 - d1 * d6;
  if (vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0) {
    v = 0.0, w = d2 / (d2 - d6);
    return;
  }
  const double va = d3 * d6 - d5 * d4;
  if (va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0) {
    const double t = (d4 - d3) / ((d4 - d3) + (d5 - d6));
    v = 1.0 - t, w = t;
    return;
  }
  const double denom = va + vb + vc;
  if (std::abs(denom) < 1e-300) {
    v = 0.0, w = 0.0;
    return;
  }
  v = vb / denom, w = vc / denom;
}

// Parameters of the closest points of segments p1 q1 and p2 q2 (Ericson, 5.1.9).
inline void closest_on_segments(const gp_XYZ &p1, const gp_XYZ &q1, const gp_XYZ &p2, const gp_XYZ &q2, double &s,
                                double &t) {
  const gp_XYZ d1 = q1 - p1, d2 = q2 - p2, r = p1 - p2;
  const double a = d1.Dot(d1), e = d2.Dot(d2), f = d2.Dot(r);
  if (a <= 1e-300 && e <= 1e-300) {
    s = t = 0.0;
    return;
  }
  if (a <= 1e-300) {
    s = 0.0, t = std::clamp(f / e, 0.0, 1.0);
    return;
  }
  const double c = d1.Dot(r);
  if (e <= 1e-300) {
    t = 0.0, s = std::clamp(-c / a, 0.0, 1.0);
    return;
  }
  const double b = d1.Dot(d2), denom = a * e - b * b;
  s = denom > 0.0 ? std::clamp((b * f - c * e) / denom, 0.0, 1.0) : 0.0;
  t = (b * s + f) / e;
  if (t < 0.0) {
    t = 0.0, s = std::clamp(-c / a, 0.0, 1.0);
  } else if (t > 1.0) {
    t = 1.0, s = std::clamp((b - c) / a, 0.0, 1.0);
  }
}

// Where segment p q passes through triangle (a, b, c): the parameter along the
// segment and the weights on b and c (Möller & Trumbore).
inline bool segment_through_triangle(const gp_XYZ &p, const gp_XYZ &q, const gp_XYZ &a, const gp_XYZ &b,
                                     const gp_XYZ &c, double &s, double &v, double &w) {
  const gp_XYZ d = q - p, e1 = b - a, e2 = c - a;
  const gp_XYZ h = d.Crossed(e2);
  const double det = e1.Dot(h);
  if (std::abs(det) < 1e-300) {
    return false;
  }
  const gp_XYZ o = p - a;
  v = o.Dot(h) / det;
  if (v < 0.0 || v > 1.0) {
    return false;
  }
  const gp_XYZ k = o.Crossed(e1);
  w = d.Dot(k) / det;
  if (w < 0.0 || v + w > 1.0) {
    return false;
  }
  s = e2.Dot(k) / det;
  return s >= 0.0 && s <= 1.0;
}

struct TriangleMesh {
  std::vector<gp_XYZ> nodes;
  std::vector<gp_Pnt2d> uvs;
  std::vector<std::array<int, 3>> triangles;
  struct Node {
    Bounds box;
    int first = 0;
    int count = 0; // a leaf when non-zero
    int left = -1;
    int right = -1;
  };
  std::vector<Node> tree;
  std::vector<int> order;
  // How far the surface may lie from the triangles, mm: twice the largest
  // deviation measured at a triangle's middle, plus the face's tolerance.
  double slack = 0.0;

  bool empty() const { return tree.empty(); }

  gp_XYZ corner(int t, int k) const { return nodes[triangles[t][k]]; }
  gp_XYZ point_at(int t, double v, double w) const {
    return corner(t, 0) * (1.0 - v - w) + corner(t, 1) * v + corner(t, 2) * w;
  }
  gp_Pnt2d uv_at(int t, double v, double w) const {
    const gp_XY a = uvs[triangles[t][0]].XY(), b = uvs[triangles[t][1]].XY(), c = uvs[triangles[t][2]].XY();
    return gp_Pnt2d(a * (1.0 - v - w) + b * v + c * w);
  }
  double longest_side(int t) const {
    const gp_XYZ a = corner(t, 0), b = corner(t, 1), c = corner(t, 2);
    return std::sqrt(std::max({(b - a).SquareModulus(), (c - b).SquareModulus(), (a - c).SquareModulus()}));
  }
  Bounds bounds_of(int t) const {
    Bounds box;
    for (int k = 0; k < 3; ++k) {
      box.add(corner(t, k));
    }
    return box;
  }

  void build() {
    if (triangles.empty()) {
      return;
    }
    order.resize(triangles.size());
    std::vector<gp_XYZ> centres(triangles.size());
    for (int t = 0; t < (int)triangles.size(); ++t) {
      order[t] = t;
      centres[t] = (corner(t, 0) + corner(t, 1) + corner(t, 2)) / 3.0;
    }
    tree.reserve(triangles.size() / 2 + 2);
    split(0, (int)triangles.size(), centres);
  }

  int split(int first, int count, const std::vector<gp_XYZ> &centres) {
    const int index = (int)tree.size();
    tree.emplace_back();
    Bounds box, mids;
    for (int k = first; k < first + count; ++k) {
      box.add(bounds_of(order[k]));
      mids.add(centres[order[k]]);
    }
    tree[index].box = box;
    if (count <= 4) {
      tree[index].first = first;
      tree[index].count = count;
      return index;
    }
    int axis = 0;
    for (int i = 1; i < 3; ++i) {
      if (mids.hi[i] - mids.lo[i] > mids.hi[axis] - mids.lo[axis]) {
        axis = i;
      }
    }
    const int half = count / 2;
    std::nth_element(order.begin() + first, order.begin() + first + half, order.begin() + first + count,
                     [&](int a, int b) { return centres[a].Coord(axis + 1) < centres[b].Coord(axis + 1); });
    const int left = split(first, half, centres);
    const int right = split(first + half, count - half, centres);
    tree[index].left = left;
    tree[index].right = right;
    return index;
  }

  double triangle_distance_sq(int t, const gp_XYZ &p, double &v, double &w) const {
    closest_on_triangle(p, corner(t, 0), corner(t, 1), corner(t, 2), v, w);
    return (point_at(t, v, w) - p).SquareModulus();
  }

  // Nearer child last, so it is visited first.
  void push_children(const Node &node, const gp_XYZ &p, std::vector<int> &stack) const {
    if (tree[node.left].box.distance_sq(p) < tree[node.right].box.distance_sq(p)) {
      stack.push_back(node.right);
      stack.push_back(node.left);
    } else {
      stack.push_back(node.left);
      stack.push_back(node.right);
    }
  }

  // The least distance from p to the triangles, or `bound` when none is nearer.
  double distance(const gp_XYZ &p, double bound) const {
    if (empty()) {
      return bound;
    }
    double best_sq = bound * bound;
    std::vector<int> stack{0};
    while (!stack.empty()) {
      const Node &node = tree[stack.back()];
      stack.pop_back();
      if (node.box.distance_sq(p) >= best_sq) {
        continue;
      }
      if (node.count > 0) {
        for (int k = node.first; k < node.first + node.count; ++k) {
          double v, w;
          best_sq = std::min(best_sq, triangle_distance_sq(order[k], p, v, w));
        }
      } else {
        push_children(node, p, stack);
      }
    }
    return std::sqrt(best_sq);
  }

  // Every triangle within `radius` of p, with the weights of its nearest point.
  struct Hit {
    int t;
    double v, w, d;
  };
  void within(const gp_XYZ &p, double radius, std::vector<Hit> &out) const {
    if (empty()) {
      return;
    }
    const double limit = radius * radius;
    std::vector<int> stack{0};
    while (!stack.empty()) {
      const Node &node = tree[stack.back()];
      stack.pop_back();
      if (node.box.distance_sq(p) > limit) {
        continue;
      }
      if (node.count > 0) {
        for (int k = node.first; k < node.first + node.count; ++k) {
          double v, w;
          const double d = triangle_distance_sq(order[k], p, v, w);
          if (d <= limit) {
            out.push_back({order[k], v, w, std::sqrt(d)});
          }
        }
      } else {
        push_children(node, p, stack);
      }
    }
  }
};

// The closest points of triangle `ta` of `A` and `tb` of `B`, as weights on
// each, and their distance.
struct TrianglePair {
  double d = 1e300;
  double va = 0, wa = 0, vb = 0, wb = 0;
};

inline TrianglePair closest_triangles(const TriangleMesh &A, int ta, const TriangleMesh &B, int tb) {
  const gp_XYZ a[3] = {A.corner(ta, 0), A.corner(ta, 1), A.corner(ta, 2)};
  const gp_XYZ b[3] = {B.corner(tb, 0), B.corner(tb, 1), B.corner(tb, 2)};
  // Weights of each corner, and of a point along a side.
  static const double corner_vw[3][2] = {{0, 0}, {1, 0}, {0, 1}};
  auto along = [](int k, double s, double &v, double &w) {
    const int n = (k + 1) % 3;
    v = corner_vw[k][0] * (1 - s) + corner_vw[n][0] * s;
    w = corner_vw[k][1] * (1 - s) + corner_vw[n][1] * s;
  };
  TrianglePair best;
  auto consider = [&](double d, double va, double wa, double vb, double wb) {
    if (d < best.d) {
      best = {d, va, wa, vb, wb};
    }
  };
  for (int k = 0; k < 3; ++k) {
    double v, w, s;
    if (segment_through_triangle(a[k], a[(k + 1) % 3], b[0], b[1], b[2], s, v, w)) {
      double va, wa;
      along(k, s, va, wa);
      consider(0.0, va, wa, v, w);
      return best;
    }
    if (segment_through_triangle(b[k], b[(k + 1) % 3], a[0], a[1], a[2], s, v, w)) {
      double vb, wb;
      along(k, s, vb, wb);
      consider(0.0, v, w, vb, wb);
      return best;
    }
  }
  for (int k = 0; k < 3; ++k) {
    double v, w;
    closest_on_triangle(a[k], b[0], b[1], b[2], v, w);
    consider((B.point_at(tb, v, w) - a[k]).Modulus(), corner_vw[k][0], corner_vw[k][1], v, w);
    closest_on_triangle(b[k], a[0], a[1], a[2], v, w);
    consider((A.point_at(ta, v, w) - b[k]).Modulus(), v, w, corner_vw[k][0], corner_vw[k][1]);
  }
  for (int i = 0; i < 3; ++i) {
    for (int j = 0; j < 3; ++j) {
      double s, t;
      closest_on_segments(a[i], a[(i + 1) % 3], b[j], b[(j + 1) % 3], s, t);
      const gp_XYZ p = a[i] + (a[(i + 1) % 3] - a[i]) * s;
      const gp_XYZ q = b[j] + (b[(j + 1) % 3] - b[j]) * t;
      double va, wa, vb, wb;
      along(i, s, va, wa);
      along(j, t, vb, wb);
      consider((p - q).Modulus(), va, wa, vb, wb);
    }
  }
  return best;
}

struct PairHit {
  int ta, tb;
  TrianglePair at;
  double d;
};

// Triangle pairs of A and B no farther apart than `reach`: the nearest pair
// in each cell of a grid `cell` across, among those within `band` of the
// least distance found, which is left in `best`. Pairs whose point on A
// `skip` rejects are not counted at all. One pair a cell keeps a wall of even
// thickness, where every pair across it is within the band, from growing the
// list with the wall's area.
template <class Skip>
void close_triangles(const TriangleMesh &A, const TriangleMesh &B, double reach, double band, double cell,
                     std::vector<PairHit> &hits, double &best, Skip skip) {
  best = reach;
  if (A.empty() || B.empty()) {
    return;
  }
  std::map<std::array<long long, 3>, PairHit> nearest;
  std::vector<std::pair<int, int>> stack{{0, 0}};
  auto limit = [&]() { return std::min(reach, best + band); };
  const auto size = [](const Bounds &b) { return (b.hi[0] - b.lo[0]) + (b.hi[1] - b.lo[1]) + (b.hi[2] - b.lo[2]); };
  while (!stack.empty()) {
    const auto [na, nb] = stack.back();
    stack.pop_back();
    const auto &x = A.tree[na];
    const auto &y = B.tree[nb];
    const double l = limit();
    if (x.box.distance_sq(y.box) > l * l) {
      continue;
    }
    if (x.count > 0 && y.count > 0) {
      for (int i = x.first; i < x.first + x.count; ++i) {
        const int ta = A.order[i];
        const gp_XYZ middle = A.point_at(ta, 1.0 / 3.0, 1.0 / 3.0);
        const std::array<long long, 3> key = {(long long)std::floor(middle.X() / cell),
                                              (long long)std::floor(middle.Y() / cell),
                                              (long long)std::floor(middle.Z() / cell)};
        auto found = nearest.find(key);
        // A cell whose pair is already within the band needs no better one:
        // its pair only seeds the exact search.
        if (found != nearest.end() && found->second.d <= best + band) {
          continue;
        }
        const Bounds bi = A.bounds_of(ta);
        for (int j = y.first; j < y.first + y.count; ++j) {
          const double lj = limit();
          if (bi.distance_sq(B.bounds_of(B.order[j])) > lj * lj) {
            continue;
          }
          const TrianglePair pair = closest_triangles(A, ta, B, B.order[j]);
          if (pair.d > lj || skip(A.point_at(ta, pair.va, pair.wa))) {
            continue;
          }
          best = std::min(best, pair.d);
          if (found == nearest.end()) {
            found = nearest.emplace(key, PairHit{ta, B.order[j], pair, pair.d}).first;
          } else if (pair.d < found->second.d) {
            found->second = {ta, B.order[j], pair, pair.d};
          }
          if (found->second.d <= best + band) {
            break;
          }
        }
      }
      continue;
    }
    // Descend the larger box, or the only one that can be.
    if (y.count > 0 || (x.count == 0 && size(x.box) >= size(y.box))) {
      stack.push_back({x.left, nb});
      stack.push_back({x.right, nb});
    } else {
      stack.push_back({na, y.left});
      stack.push_back({na, y.right});
    }
  }
  const double keep = best + band;
  for (const auto &[key, hit] : nearest) {
    if (hit.d <= keep) {
      hits.push_back(hit);
    }
  }
}

// Solve the n x n system M x = b in place, n <= 4; false when singular.
inline bool solve(int n, double M[4][4], double b[4]) {
  for (int c = 0; c < n; ++c) {
    int pivot = c;
    for (int r = c + 1; r < n; ++r) {
      if (std::abs(M[r][c]) > std::abs(M[pivot][c])) {
        pivot = r;
      }
    }
    if (std::abs(M[pivot][c]) < 1e-300) {
      return false;
    }
    if (pivot != c) {
      for (int k = 0; k < n; ++k) {
        std::swap(M[c][k], M[pivot][k]);
      }
      std::swap(b[c], b[pivot]);
    }
    for (int r = c + 1; r < n; ++r) {
      const double f = M[r][c] / M[c][c];
      for (int k = c; k < n; ++k) {
        M[r][k] -= f * M[c][k];
      }
      b[r] -= f * b[c];
    }
  }
  for (int r = n - 1; r >= 0; --r) {
    double sum = b[r];
    for (int k = r + 1; k < n; ++k) {
      sum -= M[r][k] * b[k];
    }
    b[r] = sum / M[r][r];
  }
  return true;
}

// A face's box in its surface's parameters, which a search is kept inside:
// a line of equal distances runs on past a face along its untrimmed surface.
// A direction the face covers for a whole period is left free.
struct UVBox {
  double lo[2] = {-1e300, -1e300};
  double hi[2] = {1e300, 1e300};
  bool bounded[2] = {false, false};

  static UVBox of(const TopoDS_Face &face, const BRepAdaptor_Surface &S) {
    UVBox box;
    double u0, u1, v0, v1;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    box.lo[0] = u0, box.hi[0] = u1, box.lo[1] = v0, box.hi[1] = v1;
    box.bounded[0] = !(S.IsUPeriodic() && u1 - u0 >= S.UPeriod() - 1e-9);
    box.bounded[1] = !(S.IsVPeriodic() && v1 - v0 >= S.VPeriod() - 1e-9);
    return box;
  }

  void clamp(double &u, double &v) const {
    if (bounded[0]) {
      u = std::clamp(u, lo[0], hi[0]);
    }
    if (bounded[1]) {
      v = std::clamp(v, lo[1], hi[1]);
    }
  }
};

struct Jet {
  gp_Pnt p;
  gp_Vec du, dv, duu, dvv, duv;
  void at(const BRepAdaptor_Surface &S, double u, double v) { S.D2(u, v, p, du, dv, duu, dvv, duv); }
};

// Damped Newton (Levenberg–Marquardt) on the squared distance, from a start
// near the answer; n = 2 for a point and a surface, 4 for two surfaces. The
// Hessian is the exact one, so a line or a patch of equal distances — two
// coaxial cylinders — is a flat direction the damping holds still rather than
// a singular matrix. Returns whether the step settled.
struct Nearest {
  static bool point(const BRepAdaptor_Surface &S, const UVBox &box, const gp_Pnt &target, double &u, double &v,
                    gp_Pnt &at) {
    Jet j;
    j.at(S, u, v);
    double f = 0.5 * j.p.SquareDistance(target);
    double lambda = 1e-3;
    for (int it = 0; it < 80; ++it) {
      const gp_Vec D(target, j.p);
      double M[4][4] = {{j.du.Dot(j.du) + D.Dot(j.duu), j.du.Dot(j.dv) + D.Dot(j.duv)},
                        {j.du.Dot(j.dv) + D.Dot(j.duv), j.dv.Dot(j.dv) + D.Dot(j.dvv)}};
      double g[4] = {-j.du.Dot(D), -j.dv.Dot(D)};
      const double s0 = j.du.Dot(j.du), s1 = j.dv.Dot(j.dv);
      M[0][0] += lambda * std::max(s0, 1e-12);
      M[1][1] += lambda * std::max(s1, 1e-12);
      if (!solve(2, M, g)) {
        lambda *= 8;
        continue;
      }
      double un = u + g[0], vn = v + g[1];
      box.clamp(un, vn);
      Jet k;
      k.at(S, un, vn);
      const double fn = 0.5 * k.p.SquareDistance(target);
      if (fn <= f) {
        const double moved = k.p.Distance(j.p);
        u = un, v = vn, j = k, f = fn;
        lambda = std::max(lambda * 0.25, 1e-12);
        if (moved < 1e-10) {
          at = j.p;
          return true;
        }
      } else {
        lambda *= 8;
        if (lambda > 1e12) {
          break;
        }
      }
    }
    at = j.p;
    // Stalled: settled when nothing downhill is left.
    const gp_Vec D(target, j.p);
    return std::abs(j.du.Dot(D)) <= 1e-9 * (1.0 + j.du.Magnitude()) &&
           std::abs(j.dv.Dot(D)) <= 1e-9 * (1.0 + j.dv.Magnitude());
  }

  static bool pair(const BRepAdaptor_Surface &A, const UVBox &boxA, const BRepAdaptor_Surface &B,
                   const UVBox &boxB, double x[4], gp_Pnt &pa, gp_Pnt &pb) {
    Jet a, b;
    a.at(A, x[0], x[1]);
    b.at(B, x[2], x[3]);
    double f = 0.5 * a.p.SquareDistance(b.p);
    double lambda = 1e-3;
    for (int it = 0; it < 120; ++it) {
      const gp_Vec D(b.p, a.p);
      const gp_Vec J[4] = {a.du, a.dv, -b.du, -b.dv};
      double M[4][4];
      double g[4];
      for (int r = 0; r < 4; ++r) {
        g[r] = -J[r].Dot(D);
        for (int c = 0; c < 4; ++c) {
          M[r][c] = J[r].Dot(J[c]);
        }
      }
      M[0][0] += D.Dot(a.duu);
      M[0][1] += D.Dot(a.duv);
      M[1][0] += D.Dot(a.duv);
      M[1][1] += D.Dot(a.dvv);
      M[2][2] -= D.Dot(b.duu);
      M[2][3] -= D.Dot(b.duv);
      M[3][2] -= D.Dot(b.duv);
      M[3][3] -= D.Dot(b.dvv);
      for (int r = 0; r < 4; ++r) {
        M[r][r] += lambda * std::max(J[r].Dot(J[r]), 1e-12);
      }
      if (!solve(4, M, g)) {
        lambda *= 8;
        if (lambda > 1e12) {
          break;
        }
        continue;
      }
      double y[4] = {x[0] + g[0], x[1] + g[1], x[2] + g[2], x[3] + g[3]};
      boxA.clamp(y[0], y[1]);
      boxB.clamp(y[2], y[3]);
      Jet an, bn;
      an.at(A, y[0], y[1]);
      bn.at(B, y[2], y[3]);
      const double fn = 0.5 * an.p.SquareDistance(bn.p);
      if (fn <= f) {
        const double moved = std::max(an.p.Distance(a.p), bn.p.Distance(b.p));
        std::copy(y, y + 4, x);
        a = an, b = bn, f = fn;
        lambda = std::max(lambda * 0.25, 1e-12);
        if (moved < 1e-10) {
          break;
        }
      } else {
        lambda *= 8;
        if (lambda > 1e12) {
          break;
        }
      }
    }
    pa = a.p, pb = b.p;
    const gp_Vec D(b.p, a.p);
    const double scale = 1e-7 * (1.0 + D.Magnitude());
    // Settled where the distance is stationary along every direction the
    // boxes leave free: a component pressing against a box side is held.
    auto still = [&](const gp_Vec &d, double value, const UVBox &box, int k, double sign) {
      const double g = sign * d.Dot(D);
      if (std::abs(g) <= scale * (1.0 + d.Magnitude())) {
        return true;
      }
      // Descending would move the parameter past the side it rests on.
      return box.bounded[k] && ((value <= box.lo[k] && g > 0.0) || (value >= box.hi[k] && g < 0.0));
    };
    return still(a.du, x[0], boxA, 0, 1.0) && still(a.dv, x[1], boxA, 1, 1.0) && still(b.du, x[2], boxB, 0, -1.0) &&
           still(b.dv, x[3], boxB, 1, -1.0);
  }
};

// Seeds far enough apart to lie in different basins: the nearest hit first,
// then any hit farther than a few triangles from every seed already taken.
template <class Hit, class Where, class Size>
std::vector<Hit> distinct_seeds(std::vector<Hit> hits, Where where, Size size, std::size_t cap) {
  std::sort(hits.begin(), hits.end(), [](const Hit &a, const Hit &b) { return a.d < b.d; });
  std::vector<Hit> seeds;
  std::vector<std::pair<gp_XYZ, double>> taken;
  for (const Hit &h : hits) {
    const gp_XYZ p = where(h);
    const double r = 3.0 * size(h);
    bool near = false;
    for (const auto &[q, s] : taken) {
      if ((p - q).SquareModulus() < std::max(r, s) * std::max(r, s)) {
        near = true;
        break;
      }
    }
    if (near) {
      continue;
    }
    seeds.push_back(h);
    taken.push_back({p, r});
    if (seeds.size() >= cap) {
      break;
    }
  }
  return seeds;
}

} // namespace parcad_proximity

// The nearest boundary point of a shape to many query points — added for
// parcad, see PARCAD-CHANGES.md. `BRepExtrema_DistShapeShape` rebuilds every
// face's projector and bounding box per call; this builds them once, as
// `BRepExtrema_ExtPF` and `BRepExtrema_ExtPC` do, and keeps them. Faces are
// numbered in `TopExp::MapShapes` order, the order `IndexedMapOfShape` gives.
class NearestBoundary {
  struct Box {
    double lo[3];
    double hi[3];
    double distance_sq(const gp_Pnt &p) const {
      double sum = 0.0;
      const double c[3] = {p.X(), p.Y(), p.Z()};
      for (int i = 0; i < 3; ++i) {
        const double d = c[i] < lo[i] ? lo[i] - c[i] : (c[i] > hi[i] ? c[i] - hi[i] : 0.0);
        sum += d * d;
      }
      return sum;
    }
    static Box of(const TopoDS_Shape &shape) {
      Bnd_Box bnd;
      BRepBndLib::AddOptimal(shape, bnd, /*useTriangulation*/ false, /*useShapeTolerance*/ true);
      Box box;
      if (bnd.IsVoid()) {
        box.lo[0] = box.lo[1] = box.lo[2] = -1e300;
        box.hi[0] = box.hi[1] = box.hi[2] = 1e300;
      } else {
        bnd.Get(box.lo[0], box.lo[1], box.lo[2], box.hi[0], box.hi[1], box.hi[2]);
      }
      return box;
    }
    // Enclosing but not tight, from control points: a meshed face is pruned
    // by its triangles anyway, and `AddOptimal` on an offset B-spline face is
    // most of the cost of building this.
    static Box loose(const TopoDS_Shape &shape) {
      Bnd_Box bnd;
      BRepBndLib::Add(shape, bnd, /*useTriangulation*/ false);
      Box box;
      if (bnd.IsVoid()) {
        box.lo[0] = box.lo[1] = box.lo[2] = -1e300;
        box.hi[0] = box.hi[1] = box.hi[2] = 1e300;
      } else {
        bnd.Get(box.lo[0], box.lo[1], box.lo[2], box.hi[0], box.hi[1], box.hi[2]);
      }
      return box;
    }
  };
  struct FaceEntry {
    TopoDS_Face face;
    BRepAdaptor_Surface surface; // Extrema_ExtPS keeps a pointer to this
    Extrema_ExtPS extrema;
    double tolerance = 0.0;
    bool geometric = false;
    // A plane, cylinder, cone, sphere or torus: `extrema` answers those in
    // closed form, and every other surface from a sample grid it rebuilds
    // per point, which the triangulation replaces.
    bool analytic = false;
    Box box;
    std::vector<int> edges;
    parcad_proximity::TriangleMesh mesh;
    // Built on first use: the wires as polygons in the face's parameters,
    // with the exact classifier behind them for a point within tolerance.
    std::unique_ptr<BRepTopAdaptor_FClass2d> classifier;
    parcad_proximity::UVBox uv_box;
  };
  struct EdgeEntry {
    BRepAdaptor_Curve curve; // Extrema_ExtPC keeps a pointer to this
    Extrema_ExtPC extrema;
    gp_Pnt ends[2];
    bool geometric = false;
    Box box;
    int stamp = 0;
  };
  std::vector<std::unique_ptr<FaceEntry>> faces;
  std::vector<std::unique_ptr<EdgeEntry>> edges;
  int query = 0;
  TopoDS_Shape shape;
  IndexedMapOfShape face_map;
  IndexedMapOfShape edge_map;

  // One face's side of an edge: the surface's outward normal there and the
  // direction into the face, square to the edge.
  struct Side {
    TopoDS_Edge edge; // as the face's wire holds it
    opencascade::handle<Geom2d_Curve> pcurve;
    double sign = 1.0; // flips `into` when the classifier disagrees with the orientation rule
  };

  bool side_at(int face, const Side &side, double t, const gp_Vec &tangent, gp_Vec &normal, gp_Vec &into,
               gp_Pnt2d &uv) {
    FaceEntry &f = *faces[face];
    uv = side.pcurve->Value(t);
    gp_Pnt at;
    gp_Vec du, dv;
    f.surface.D1(uv.X(), uv.Y(), at, du, dv);
    normal = du.Crossed(dv);
    if (normal.SquareMagnitude() < 1e-24) {
      return false;
    }
    normal.Normalize();
    if (f.face.Orientation() == TopAbs_REVERSED) {
      normal.Reverse();
    }
    // The face lies to the left of its oriented boundary, seen from outside.
    gp_Vec along = side.edge.Orientation() == TopAbs_REVERSED ? tangent.Reversed() : tangent;
    into = normal.Crossed(along) * side.sign;
    if (into.SquareMagnitude() < 1e-24) {
      return false;
    }
    into.Normalize();
    return true;
  }

  // Whether a step along `into` from `uv` stays in the face; the one check
  // that the orientation rule in `side_at` holds for this face and edge.
  bool steps_inside(int face, const gp_Pnt2d &uv, const gp_Vec &into, double step) {
    FaceEntry &f = *faces[face];
    gp_Pnt at;
    gp_Vec du, dv;
    f.surface.D1(uv.X(), uv.Y(), at, du, dv);
    const double a11 = du.Dot(du), a12 = du.Dot(dv), a22 = dv.Dot(dv);
    const double det = a11 * a22 - a12 * a12;
    if (std::abs(det) < 1e-30) {
      return true;
    }
    const double b1 = du.Dot(into), b2 = dv.Dot(into);
    const double a = (a22 * b1 - a12 * b2) / det;
    const double b = (a11 * b2 - a12 * b1) / det;
    return state(f, uv.X() + a * step, uv.Y() + b * step) != TopAbs_OUT;
  }

  // The face's own triangulation, if the shape has been meshed, and how far
  // the surface strays from it: measured at every triangle's middle, where a
  // chord is farthest from a gently curved surface, and doubled.
  static void load_mesh(FaceEntry &f) {
    TopLoc_Location location;
    const opencascade::handle<Poly_Triangulation> &tri = BRep_Tool::Triangulation(f.face, location);
    if (tri.IsNull() || !tri->HasUVNodes() || tri->NbTriangles() == 0) {
      return;
    }
    auto &m = f.mesh;
    const gp_Trsf &trsf = location.Transformation();
    m.nodes.resize(tri->NbNodes());
    m.uvs.resize(tri->NbNodes());
    for (int i = 1; i <= tri->NbNodes(); ++i) {
      m.nodes[i - 1] = tri->Node(i).Transformed(trsf).XYZ();
      m.uvs[i - 1] = tri->UVNode(i);
    }
    m.triangles.resize(tri->NbTriangles());
    double worst = 0.0;
    for (int t = 1; t <= tri->NbTriangles(); ++t) {
      int a, b, c;
      tri->Triangle(t).Get(a, b, c);
      m.triangles[t - 1] = {a - 1, b - 1, c - 1};
      if (f.surface.GetType() != GeomAbs_Plane) {
        const gp_Pnt2d uv = m.uv_at(t - 1, 1.0 / 3.0, 1.0 / 3.0);
        worst = std::max(worst, f.surface.Value(uv.X(), uv.Y()).XYZ().Subtracted(m.point_at(t - 1, 1.0 / 3.0, 1.0 / 3.0)).Modulus());
      }
    }
    double edge_tolerance = 0.0;
    for (TopExp_Explorer it(f.face, TopAbs_EDGE); it.More(); it.Next()) {
      edge_tolerance = std::max(edge_tolerance, BRep_Tool::Tolerance(TopoDS::Edge(it.Current())));
    }
    m.slack = 2.0 * std::max(worst, tri->Deflection()) + std::max(f.tolerance, edge_tolerance);
    m.build();
  }

  static TopAbs_State state(FaceEntry &f, double u, double v) {
    if (!f.classifier) {
      f.classifier = std::make_unique<BRepTopAdaptor_FClass2d>(f.face, f.tolerance);
    }
    return f.classifier->Perform(gp_Pnt2d(u, v));
  }

  static bool inside(FaceEntry &f, double u, double v) {
    const TopAbs_State s = state(f, u, v);
    return s == TopAbs_IN || s == TopAbs_ON;
  }

  // Points of face `f`'s surface that may be its nearest to `p` and nearer
  // than `bound`, whether inside the face or not: every extremum for a
  // surface OCCT solves in closed form, otherwise a Newton step from each
  // triangle that could hold the nearest point. `mesh_distance` is p's
  // distance to the triangles, already known.
  template <class Accept>
  void surface_points(FaceEntry &f, const gp_Pnt &p, double bound, double mesh_distance, Accept accept) {
    if (f.analytic || f.mesh.empty()) {
      f.extrema.Perform(p);
      if (f.extrema.IsDone()) {
        for (int k = 1; k <= f.extrema.NbExt(); ++k) {
          double u, v;
          f.extrema.Point(k).Parameter(u, v);
          accept(u, v, f.extrema.Point(k).Value(), f.extrema.SquareDistance(k));
        }
      }
      return;
    }
    // Any surface point nearer than `bound` has a triangle within `slack` of
    // it, and so within `mesh_distance + 2 slack` of p.
    const double reach = std::min(bound + f.mesh.slack, mesh_distance + 2.0 * f.mesh.slack);
    std::vector<parcad_proximity::TriangleMesh::Hit> hits;
    f.mesh.within(p.XYZ(), reach, hits);
    const auto seeds = parcad_proximity::distinct_seeds(
        std::move(hits), [&](const auto &h) { return f.mesh.point_at(h.t, h.v, h.w); },
        [&](const auto &h) { return f.mesh.longest_side(h.t); }, 8);
    for (const auto &h : seeds) {
      const gp_Pnt2d uv = f.mesh.uv_at(h.t, h.v, h.w);
      double u = uv.X(), v = uv.Y();
      gp_Pnt at;
      if (parcad_proximity::Nearest::point(f.surface, f.uv_box, p, u, v, at)) {
        accept(u, v, at, at.SquareDistance(p));
      }
    }
  }

public:
  explicit NearestBoundary(const TopoDS_Shape &shape) : shape(shape) {
    TopExp::MapShapes(shape, TopAbs_FACE, face_map);
    TopExp::MapShapes(shape, TopAbs_EDGE, edge_map);
    for (int i = 1; i <= edge_map.Extent(); ++i) {
      auto entry = std::make_unique<EdgeEntry>();
      const TopoDS_Edge &edge = TopoDS::Edge(edge_map(i));
      entry->box = Box::of(edge);
      if (BRep_Tool::IsGeometric(edge) && !BRep_Tool::Degenerated(edge)) {
        entry->curve.Initialize(edge);
        double first, last;
        BRep_Tool::Range(edge, first, last);
        const double tol = std::max(entry->curve.Resolution(Precision::Confusion()), Precision::PConfusion());
        entry->extrema.Initialize(entry->curve, first, last, tol);
        entry->ends[0] = entry->curve.Value(first);
        entry->ends[1] = entry->curve.Value(last);
        entry->geometric = true;
      }
      edges.push_back(std::move(entry));
    }
    for (int i = 1; i <= face_map.Extent(); ++i) {
      auto entry = std::make_unique<FaceEntry>();
      entry->face = TopoDS::Face(face_map(i));
      entry->surface.Initialize(entry->face, false);
      entry->tolerance = BRep_Tool::Tolerance(entry->face);
      if (entry->surface.GetType() != GeomAbs_OtherSurface) {
        const double tol = std::min(entry->tolerance, Precision::Confusion());
        const double tol_u = std::max(entry->surface.UResolution(tol), Precision::PConfusion());
        const double tol_v = std::max(entry->surface.VResolution(tol), Precision::PConfusion());
        double u0, u1, v0, v1;
        BRepTools::UVBounds(entry->face, u0, u1, v0, v1);
        // Every local extremum, not only the least: the least over the
        // untrimmed patch can lie outside the face while a nearer one inside
        // it does not.
        entry->extrema.SetFlag(Extrema_ExtFlag_MINMAX);
        entry->extrema.SetAlgo(Extrema_ExtAlgo_Grad);
        entry->extrema.Initialize(entry->surface, u0, u1, v0, v1, tol_u, tol_v);
        entry->geometric = true;
        entry->uv_box = parcad_proximity::UVBox::of(entry->face, entry->surface);
        const GeomAbs_SurfaceType type = entry->surface.GetType();
        entry->analytic = type == GeomAbs_Plane || type == GeomAbs_Cylinder || type == GeomAbs_Cone ||
                          type == GeomAbs_Sphere || type == GeomAbs_Torus;
        load_mesh(*entry);
      }
      entry->box = entry->mesh.empty() ? Box::of(entry->face) : Box::loose(entry->face);
      for (TopExp_Explorer it(entry->face, TopAbs_EDGE); it.More(); it.Next()) {
        const int index = edge_map.FindIndex(it.Current());
        if (index > 0) {
          entry->edges.push_back(index - 1);
        }
      }
      faces.push_back(std::move(entry));
    }
  }

  // The nearest boundary point closer than `within`, and the 0-based face it
  // lies on — for a point on an edge, the first face met that owns the edge.
  // A negative distance when nothing is that close.
  double nearest(const gp_Pnt &p, double within, gp_Pnt &at, int &face_index) {
    ++query;
    double best_sq = within * within;
    bool found = false;
    std::vector<std::pair<double, int>> order;
    for (int i = 0; i < (int)faces.size(); ++i) {
      const double d = faces[i]->box.distance_sq(p);
      if (d < best_sq) {
        order.emplace_back(d, i);
      }
    }
    std::sort(order.begin(), order.end());
    for (const auto &entry : order) {
      const double box_sq = entry.first;
      const int i = entry.second;
      if (box_sq >= best_sq) {
        break;
      }
      FaceEntry &f = *faces[i];
      double mesh_distance = 0.0;
      if (!f.mesh.empty()) {
        // Nothing of the face, its boundary included, is nearer than this.
        const double best = std::sqrt(best_sq);
        mesh_distance = f.mesh.distance(p.XYZ(), best + f.mesh.slack);
        if (mesh_distance - f.mesh.slack >= best) {
          continue;
        }
      }
      if (f.geometric) {
        surface_points(f, p, std::sqrt(best_sq), mesh_distance, [&](double u, double v, const gp_Pnt &q, double d) {
          if (d < best_sq && inside(f, u, v)) {
            best_sq = d;
            at = q;
            face_index = i;
            found = true;
          }
        });
      }
      for (int e : f.edges) {
        EdgeEntry &edge = *edges[e];
        if (edge.stamp == query || !edge.geometric || edge.box.distance_sq(p) >= best_sq) {
          continue;
        }
        edge.stamp = query;
        for (const gp_Pnt &end : edge.ends) {
          const double d = end.SquareDistance(p);
          if (d < best_sq) {
            best_sq = d;
            at = end;
            face_index = i;
            found = true;
          }
        }
        edge.extrema.Perform(p);
        if (edge.extrema.IsDone()) {
          for (int k = 1; k <= edge.extrema.NbExt(); ++k) {
            const double d = edge.extrema.SquareDistance(k);
            if (d < best_sq) {
              best_sq = d;
              at = edge.extrema.Point(k).Value();
              face_index = i;
              found = true;
            }
          }
        }
      }
    }
    return found ? std::sqrt(best_sq) : -1.0;
  }

  // The point of face `index` nearest `p`, and the face's outward normal
  // there, unnormalised. False where the surface cannot answer, or where that
  // point of the surface lies outside the face's boundary.
  bool project(int index, const gp_Pnt &p, gp_Pnt &at, gp_Vec &normal) {
    if (index < 0 || index >= (int)faces.size() || !faces[index]->geometric) {
      return false;
    }
    FaceEntry &f = *faces[index];
    const double mesh_distance = f.mesh.empty() ? 0.0 : f.mesh.distance(p.XYZ(), 1e300);
    double best_sq = 1e300, u = 0.0, v = 0.0;
    surface_points(f, p, 1e150, mesh_distance, [&](double pu, double pv, const gp_Pnt &, double d) {
      if (d < best_sq) {
        best_sq = d, u = pu, v = pv;
      }
    });
    if (best_sq >= 1e300) {
      return false;
    }
    if (state(f, u, v) == TopAbs_OUT) {
      return false;
    }
    gp_Vec du, dv;
    f.surface.D1(u, v, at, du, dv);
    normal = du.Crossed(dv);
    if (f.face.Orientation() == TopAbs_REVERSED) {
      normal.Reverse();
    }
    return true;
  }

  // The point of face `index` at surface parameters (u, v) and its outward
  // normal, unnormalised; with `inside`, false for a point outside the face.
  bool evaluate(int index, double u, double v, bool inside, gp_Pnt &at, gp_Vec &normal) {
    if (index < 0 || index >= (int)faces.size() || !faces[index]->geometric) {
      return false;
    }
    FaceEntry &f = *faces[index];
    if (inside && state(f, u, v) == TopAbs_OUT) {
      return false;
    }
    gp_Vec du, dv;
    f.surface.D1(u, v, at, du, dv);
    normal = du.Crossed(dv);
    if (f.face.Orientation() == TopAbs_REVERSED) {
      normal.Reverse();
    }
    return true;
  }

  // Every edge between two different faces, sampled along its length: the
  // angle the material encloses between the faces there, in degrees — 0 a
  // knife, 90 a box's edge, 180 smooth, over 180 concave. Appends eleven
  // numbers per sample: edge, the two faces, the point, the angle, the number
  // of faces whose orientation the classifier corrected (for tests), and the
  // direction into the material halfway between the faces.
  void edge_wedges(double spacing, rust::Vec<double> &out) {
    IndexedDataMapOfShapeListOfShape edge_faces;
    TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, edge_faces);
    for (int e = 0; e < (int)edges.size(); ++e) {
      if (!edges[e]->geometric) {
        continue;
      }
      const TopoDS_Shape &edge_shape = edge_map(e + 1);
      const int around = edge_faces.FindIndex(edge_shape);
      if (around == 0) {
        continue;
      }
      std::vector<int> owners;
      for (const TopoDS_Shape &face : edge_faces(around)) {
        const int index = face_map.FindIndex(face) - 1;
        if (index >= 0 && std::find(owners.begin(), owners.end(), index) == owners.end()) {
          owners.push_back(index);
        }
      }
      if (owners.size() != 2) {
        continue;
      }
      Side sides[2];
      bool usable = true;
      for (int k = 0; k < 2 && usable; ++k) {
        int uses = 0;
        for (TopExp_Explorer it(faces[owners[k]]->face, TopAbs_EDGE); it.More(); it.Next()) {
          if (it.Current().IsSame(edge_shape)) {
            sides[k].edge = TopoDS::Edge(it.Current());
            ++uses;
          }
        }
        double f0, l0;
        if (uses == 1) {
          sides[k].pcurve = BRep_Tool::CurveOnSurface(sides[k].edge, faces[owners[k]]->face, f0, l0);
        }
        usable = uses == 1 && !sides[k].pcurve.IsNull() && faces[owners[k]]->geometric;
      }
      // A pcurve is only a parametrisation of the edge's own curve when the
      // two share parameters; booleans keep that, and an edge without it is
      // left to the sampled sweep.
      if (!usable || !BRep_Tool::SameParameter(TopoDS::Edge(edge_shape))) {
        continue;
      }
      BRepAdaptor_Curve &curve = edges[e]->curve;
      const double first = curve.FirstParameter(), last = curve.LastParameter();
      double length = 0.0;
      try {
        length = GCPnts_AbscissaPoint::Length(curve, first, last, 1e-6);
      } catch (...) {
        length = edges[e]->ends[0].Distance(edges[e]->ends[1]);
      }
      const int n = std::clamp((int)std::ceil(length / spacing), 8, 512);
      int corrected = 0;
      {
        const double t = 0.5 * (first + last);
        gp_Pnt p;
        gp_Vec tangent;
        curve.D1(t, p, tangent);
        if (tangent.SquareMagnitude() > 1e-24) {
          tangent.Normalize();
          for (int k = 0; k < 2; ++k) {
            gp_Vec normal, into;
            gp_Pnt2d uv;
            if (side_at(owners[k], sides[k], t, tangent, normal, into, uv) &&
                !steps_inside(owners[k], uv, into, std::max(1e-4, 1e-3 * length))) {
              sides[k].sign = -1.0;
              ++corrected;
            }
          }
        }
      }
      for (int i = 0; i <= n; ++i) {
        const double t = first + (last - first) * i / n;
        gp_Pnt p;
        gp_Vec tangent;
        curve.D1(t, p, tangent);
        if (tangent.SquareMagnitude() < 1e-24) {
          continue;
        }
        tangent.Normalize();
        gp_Vec normal[2], into[2];
        gp_Pnt2d uv;
        if (!side_at(owners[0], sides[0], t, tangent, normal[0], into[0], uv) ||
            !side_at(owners[1], sides[1], t, tangent, normal[1], into[1], uv)) {
          continue;
        }
        const double between = std::atan2(into[0].Crossed(into[1]).Magnitude(), into[0].Dot(into[1])) * 180.0 / M_PI;
        const double facing = into[0].Dot(normal[1]);
        const bool convex = facing < -1e-9 || (facing <= 1e-9 && between < 90.0);
        const double angle = convex ? between : 360.0 - between;
        gp_Vec bisector = into[0] + into[1];
        if (bisector.SquareMagnitude() > 1e-24) {
          bisector.Normalize();
        }
        const double row[11] = {(double)e, (double)owners[0], (double)owners[1], p.X(),        p.Y(),        p.Z(),
                                angle,     (double)corrected, bisector.X(),       bisector.Y(), bisector.Z()};
        for (double value : row) {
          out.push_back(value);
        }
      }
    }
  }

  // Surface parameters of the point of face `f` nearest `p`, inside the face
  // or not.
  bool nearest_uv(FaceEntry &f, const gp_Pnt &p, double &u, double &v) {
    const double mesh_distance = f.mesh.empty() ? 0.0 : f.mesh.distance(p.XYZ(), 1e300);
    double best_sq = 1e300;
    surface_points(f, p, 1e150, mesh_distance, [&](double pu, double pv, const gp_Pnt &, double d) {
      if (d < best_sq) {
        best_sq = d, u = pu, v = pv;
      }
    });
    return best_sq < 1e300;
  }

  // The least distance between faces `a` and `b` nearest the points given,
  // settled by Newton from the surface points nearest them: the other end of
  // a line or a patch of equal distances from where a first search stopped.
  // Negative when it does not settle; `inside` when both ends are in their
  // faces.
  double settle_pair(int a, int b, const gp_Pnt &ga, const gp_Pnt &gb, gp_Pnt &pa, gp_Pnt &pb, bool &inside) {
    if (a < 0 || b < 0 || a >= (int)faces.size() || b >= (int)faces.size() || !faces[a]->geometric ||
        !faces[b]->geometric) {
      return -1.0;
    }
    FaceEntry &A = *faces[a];
    FaceEntry &B = *faces[b];
    double x[4];
    if (!nearest_uv(A, ga, x[0], x[1]) || !nearest_uv(B, gb, x[2], x[3]) ||
        !parcad_proximity::Nearest::pair(A.surface, A.uv_box, B.surface, B.uv_box, x, pa, pb)) {
      return -1.0;
    }
    inside = this->inside(A, x[0], x[1]) && this->inside(B, x[2], x[3]);
    return pa.Distance(pb);
  }

  // Pairs of faces that share no edge and come nearer each other than
  // `reach`, each with the points where its least distance is attained. Ten
  // numbers a pair: the two faces, the distance, the point on each, and
  // whether both points are inside their faces — a least distance inside
  // both is a double normal, and one on a boundary is only near the wall.
  // Faces without a triangulation are not considered.
  void close_pairs(double reach, rust::Vec<double> &out) {
    std::vector<std::vector<int>> faces_of_edge(edges.size());
    for (int f = 0; f < (int)faces.size(); ++f) {
      for (int e : faces[f]->edges) {
        faces_of_edge[e].push_back(f);
      }
    }
    std::set<std::pair<int, int>> adjacent;
    for (const auto &around : faces_of_edge) {
      for (int a : around) {
        for (int b : around) {
          adjacent.insert({a, b});
        }
      }
    }
    // Faces that meet only at a vertex are as near each other there as the
    // vertex's own angle makes them, as beside an edge; that neighbourhood is
    // the feather search's and the sweep's.
    IndexedDataMapOfShapeListOfShape vertex_faces;
    TopExp::MapShapesAndAncestors(shape, TopAbs_VERTEX, TopAbs_FACE, vertex_faces);
    std::map<std::pair<int, int>, std::vector<gp_XYZ>> shared_vertices;
    for (int k = 1; k <= vertex_faces.Extent(); ++k) {
      std::vector<int> around;
      for (const TopoDS_Shape &face : vertex_faces(k)) {
        const int index = face_map.FindIndex(face) - 1;
        if (index >= 0 && std::find(around.begin(), around.end(), index) == around.end()) {
          around.push_back(index);
        }
      }
      const gp_XYZ at = BRep_Tool::Pnt(TopoDS::Vertex(vertex_faces.FindKey(k))).XYZ();
      for (int a : around) {
        for (int b : around) {
          if (a < b && !adjacent.count({a, b})) {
            shared_vertices[{a, b}].push_back(at);
          }
        }
      }
    }
    for (int i = 0; i < (int)faces.size(); ++i) {
      FaceEntry &A = *faces[i];
      if (A.mesh.empty() || !A.geometric) {
        continue;
      }
      for (int j = i + 1; j < (int)faces.size(); ++j) {
        FaceEntry &B = *faces[j];
        if (B.mesh.empty() || !B.geometric || adjacent.count({i, j})) {
          continue;
        }
        const double slack = A.mesh.slack + B.mesh.slack;
        const double far = reach + slack;
        const Box &x = A.box, &y = B.box;
        double gap = 0.0;
        for (int k = 0; k < 3; ++k) {
          const double d = std::max({0.0, y.lo[k] - x.hi[k], x.lo[k] - y.hi[k]});
          gap += d * d;
        }
        if (gap >= far * far) {
          continue;
        }
        std::vector<parcad_proximity::PairHit> hits;
        double least = far;
        const auto shared = shared_vertices.find({i, j});
        const double apart = 2.0 * far;
        parcad_proximity::close_triangles(A.mesh, B.mesh, far, 2.0 * slack, far, hits, least, [&](const gp_XYZ &p) {
          if (shared == shared_vertices.end()) {
            return false;
          }
          for (const gp_XYZ &v : shared->second) {
            if ((p - v).SquareModulus() < apart * apart) {
              return true;
            }
          }
          return false;
        });
        if (hits.empty() || least - slack >= reach) {
          continue;
        }
        const auto seeds = parcad_proximity::distinct_seeds(
            std::move(hits), [&](const auto &h) { return A.mesh.point_at(h.ta, h.at.va, h.at.wa); },
            [&](const auto &h) { return std::max(A.mesh.longest_side(h.ta), B.mesh.longest_side(h.tb)); }, 8);
        double best = 1e300;
        gp_Pnt best_a, best_b;
        bool interior = false;
        for (const auto &h : seeds) {
          const gp_Pnt2d ua = A.mesh.uv_at(h.ta, h.at.va, h.at.wa);
          const gp_Pnt2d ub = B.mesh.uv_at(h.tb, h.at.vb, h.at.wb);
          double x4[4] = {ua.X(), ua.Y(), ub.X(), ub.Y()};
          gp_Pnt pa, pb;
          if (!parcad_proximity::Nearest::pair(A.surface, A.uv_box, B.surface, B.uv_box, x4, pa, pb)) {
            continue;
          }
          const double d = pa.Distance(pb);
          if (d < best && inside(A, x4[0], x4[1]) && inside(B, x4[2], x4[3])) {
            best = d, best_a = pa, best_b = pb, interior = true;
          }
        }
        if (!interior) {
          // The least distance is on a boundary: report the nearest
          // triangles' points, on the surfaces, for the caller to search near.
          const auto &h = seeds.front();
          const gp_Pnt2d ua = A.mesh.uv_at(h.ta, h.at.va, h.at.wa);
          const gp_Pnt2d ub = B.mesh.uv_at(h.tb, h.at.vb, h.at.wb);
          best_a = A.surface.Value(ua.X(), ua.Y());
          best_b = B.surface.Value(ub.X(), ub.Y());
          best = h.d;
        } else if (best >= reach) {
          continue;
        }
        for (double value : {(double)i, (double)j, best, best_a.X(), best_a.Y(), best_a.Z(), best_b.X(),
                             best_b.Y(), best_b.Z(), interior ? 1.0 : 0.0}) {
          out.push_back(value);
        }
      }
    }
  }
};

inline std::unique_ptr<NearestBoundary> NearestBoundary_new(const TopoDS_Shape &shape) {
  return std::unique_ptr<NearestBoundary>(new NearestBoundary(shape));
}

inline double NearestBoundary_nearest(NearestBoundary &nearest, double x, double y, double z, double within,
                                      gp_Pnt &at, int32_t &face) {
  int index = -1;
  const double d = nearest.nearest(gp_Pnt(x, y, z), within, at, index);
  face = index;
  return d;
}

inline bool NearestBoundary_project(NearestBoundary &nearest, int32_t face, double x, double y, double z,
                                    gp_Pnt &at, gp_Vec &normal) {
  return nearest.project(face, gp_Pnt(x, y, z), at, normal);
}

inline bool NearestBoundary_evaluate(NearestBoundary &nearest, int32_t face, double u, double v, bool inside,
                                     gp_Pnt &at, gp_Vec &normal) {
  return nearest.evaluate(face, u, v, inside, at, normal);
}

inline void NearestBoundary_edge_wedges(NearestBoundary &nearest, double spacing, rust::Vec<double> &out) {
  nearest.edge_wedges(spacing, out);
}

inline double NearestBoundary_settle_pair(NearestBoundary &nearest, int32_t a, int32_t b, const gp_Pnt &ga,
                                          const gp_Pnt &gb, gp_Pnt &pa, gp_Pnt &pb, bool &inside) {
  return nearest.settle_pair(a, b, ga, gb, pa, pb, inside);
}

inline void NearestBoundary_close_pairs(NearestBoundary &nearest, double reach, rust::Vec<double> &out) {
  nearest.close_pairs(reach, out);
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
                                                         const int index) {
  return std::unique_ptr<gp_Dir>(new gp_Dir(triangulation.Normal(index)));
}

inline std::unique_ptr<gp_Pnt> Poly_Triangulation_Node(const Poly_Triangulation &triangulation,
                                                       const int index) {
  return std::unique_ptr<gp_Pnt>(new gp_Pnt(triangulation.Node(index)));
}

inline std::unique_ptr<gp_Pnt2d> Poly_Triangulation_UV(const Poly_Triangulation &triangulation,
                                                       const int index) {
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

// The adaptive form, to a relative error `eps`. Added for parcad: the fixed-order
// default misreads a B-spline solid — an elliptic cylinder by 0.86%.
inline double BRepGProp_VolumeProperties_eps(const TopoDS_Shape &shape, GProp_GProps &props, double eps) {
  return BRepGProp::VolumeProperties(shape, props, eps);
}

// Fillets
inline std::unique_ptr<TopoDS_Edge> BRepFilletAPI_MakeFillet2d_add_fillet(BRepFilletAPI_MakeFillet2d &make_fillet,
                                                                          const TopoDS_Vertex &vertex,
                                                                          double radius) {
  return std::unique_ptr<TopoDS_Edge>(new TopoDS_Edge(make_fillet.AddFillet(vertex, radius)));
}

// Chamfers
inline std::unique_ptr<TopoDS_Edge>
BRepFilletAPI_MakeFillet2d_add_chamfer(BRepFilletAPI_MakeFillet2d &make_fillet, const TopoDS_Edge &edge1,
                                       const TopoDS_Edge &edge2, const double dist1, const double dist2) {
  return std::unique_ptr<TopoDS_Edge>(new TopoDS_Edge(make_fillet.AddChamfer(edge1, edge2, dist1, dist2)));
}

inline std::unique_ptr<TopoDS_Edge>
BRepFilletAPI_MakeFillet2d_add_chamfer_angle(BRepFilletAPI_MakeFillet2d &make_fillet, const TopoDS_Edge &edge,
                                             const TopoDS_Vertex &vertex, const double dist,
                                             const double angle) {
  return std::unique_ptr<TopoDS_Edge>(new TopoDS_Edge(make_fillet.AddChamfer(edge, vertex, dist, angle)));
}

// BRepTools
inline std::unique_ptr<TopoDS_Wire> outer_wire(const TopoDS_Face &face) {
  return std::unique_ptr<TopoDS_Wire>(new TopoDS_Wire(BRepTools::OuterWire(face)));
}

// Collections
inline void map_shapes(const TopoDS_Shape &S, const TopAbs_ShapeEnum T, IndexedMapOfShape &M) {
  TopExp::MapShapes(S, T, M);
}

inline void map_shapes_and_ancestors(const TopoDS_Shape &S, const TopAbs_ShapeEnum TS, const TopAbs_ShapeEnum TA,
                                     IndexedDataMapOfShapeListOfShape &M) {
  TopExp::MapShapesAndAncestors(S, TS, TA, M);
}

inline void map_shapes_and_unique_ancestors(const TopoDS_Shape &S, const TopAbs_ShapeEnum TS, const TopAbs_ShapeEnum TA,
                                            IndexedDataMapOfShapeListOfShape &M) {
  TopExp::MapShapesAndUniqueAncestors(S, TS, TA, M);
}

inline std::unique_ptr<gp_Dir> Array1OfDir_Value(const Array1OfDir &array, int index) {
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

// The least distance between two shapes and where it is measured. Added for
// parcad's fit check: with an interference of zero, this is the clearance,
// exact rather than sampled. Returns a negative number when the search fails.
inline double BRepExtrema_least_distance(const TopoDS_Shape &a, const TopoDS_Shape &b, gp_Pnt &on_a,
                                         gp_Pnt &on_b) {
  // Default-constructed: the (a, b) constructor already runs Perform, so loading
  // through it and calling Perform again measured every clearance twice.
  BRepExtrema_DistShapeShape search;
  search.SetFlag(Extrema_ExtFlag_MIN);
  search.SetMultiThread(true);
  search.LoadS1(a);
  search.LoadS2(b);
  search.Perform();
  if (!search.IsDone() || search.NbSolution() < 1) {
    return -1.0;
  }
  on_a = search.PointOnShape1(1);
  on_b = search.PointOnShape2(1);
  return search.Value();
}

// BRepBuilderAPI_GTransform with an axis-aligned scale about the origin. Added
// for parcad: the one affine map gp_Trsf cannot hold. The builder converts every
// surface to its exact B-spline form. An empty shape when it fails.
inline std::unique_ptr<TopoDS_Shape> Shape_scaled_axes(const TopoDS_Shape &shape, double x, double y,
                                                       double z) {
  gp_GTrsf scale;
  scale.SetValue(1, 1, x);
  scale.SetValue(2, 2, y);
  scale.SetValue(3, 3, z);
  BRepBuilderAPI_GTransform builder(shape, scale, true);
  if (!builder.IsDone()) {
    return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape());
  }
  return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape(builder.Shape()));
}

// Which way each solid of a shape faces, asked of each of its shells on its
// own: a point outside the solid must classify as outside the outer shell and
// inside every other shell, each a sealed void facing inward. The outer shell
// is the one with the largest box, which does not depend on which way any of
// them faces. Classification casts a ray and reads the face it meets, so it
// needs no volume integral. Added for parcad: a walled smooth loft came back
// inside out and nothing downstream asked.
struct ParcadSolidFacing {
  TopAbs_State far = TopAbs_UNKNOWN;           // the outer shell's verdict on the far point
  std::vector<std::pair<TopoDS_Shell, TopAbs_State>> shells;
  TopoDS_Shell outer_shell;
};

inline TopAbs_State parcad_shell_classifies(const TopoDS_Shell &shell, const gp_Pnt &far) {
  BRep_Builder builder;
  TopoDS_Solid alone;
  builder.MakeSolid(alone);
  builder.Add(alone, shell);
  BRepClass3d_SolidClassifier where(alone, far, Precision::Confusion());
  return where.State();
}

inline ParcadSolidFacing parcad_facing(const TopoDS_Solid &solid) {
  ParcadSolidFacing out;
  Bnd_Box box;
  BRepBndLib::Add(solid, box);
  if (box.IsVoid()) {
    return out;
  }
  double x0, y0, z0, x1, y1, z1;
  box.Get(x0, y0, z0, x1, y1, z1);
  const double margin = 1.0 + 0.1 * std::sqrt(box.SquareExtent());
  const gp_Pnt far(x1 + margin, y1 + 0.37 * margin, z1 + 0.61 * margin);
  double largest = -1.0;
  for (TopExp_Explorer it(solid, TopAbs_SHELL); it.More(); it.Next()) {
    const TopoDS_Shell shell = TopoDS::Shell(it.Current());
    Bnd_Box shell_box;
    BRepBndLib::Add(shell, shell_box);
    const double size = shell_box.IsVoid() ? 0.0 : shell_box.SquareExtent();
    if (size > largest) {
      largest = size;
      out.outer_shell = shell;
    }
    out.shells.emplace_back(shell, parcad_shell_classifies(shell, far));
  }
  for (const auto &shell : out.shells) {
    if (shell.first.IsSame(out.outer_shell)) {
      out.far = shell.second;
    }
  }
  return out;
}

inline bool parcad_faces_out(const ParcadSolidFacing &f) {
  if (f.far != TopAbs_OUT) {
    return false;
  }
  for (const auto &shell : f.shells) {
    if (!shell.first.IsSame(f.outer_shell) && shell.second != TopAbs_IN) {
      return false;
    }
  }
  return true;
}

// "" when every solid faces outward; otherwise one line per solid that does not.
inline rust::String Shape_orientation_report(const TopoDS_Shape &shape) {
  const char *states[] = {"inside", "outside", "on its surface", "unknown"};
  std::ostringstream out;
  int index = 0;
  for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next(), ++index) {
    const ParcadSolidFacing f = parcad_facing(TopoDS::Solid(it.Current()));
    if (parcad_faces_out(f)) {
      continue;
    }
    out << "solid " << index << ": a point outside it is " << states[f.far] << " its outer surface";
    int bad = 0;
    for (const auto &shell : f.shells) {
      if (!shell.first.IsSame(f.outer_shell) && shell.second != TopAbs_IN) {
        ++bad;
      }
    }
    if (bad > 0) {
      out << ", and " << bad << " of its " << f.shells.size() - 1
          << " inner shell(s) face outward, enclosing material instead of a cavity";
    }
    out << "\n";
  }
  return rust::String(out.str());
}

// The same shape with each solid rebuilt to face outward: the outer shell
// turned to leave the outside outside, every void shell to hold it in.
inline std::unique_ptr<TopoDS_Shape> Shape_turned_outward(const TopoDS_Shape &shape) {
  BRepTools_ReShape reshape;
  bool changed = false;
  for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
    const ParcadSolidFacing f = parcad_facing(TopoDS::Solid(it.Current()));
    if (parcad_faces_out(f)) {
      continue;
    }
    BRep_Builder builder;
    TopoDS_Solid turned;
    builder.MakeSolid(turned);
    for (const auto &shell : f.shells) {
      const bool outer = shell.first.IsSame(f.outer_shell);
      const bool flip = outer ? shell.second == TopAbs_IN : shell.second == TopAbs_OUT;
      builder.Add(turned, flip ? TopoDS::Shell(shell.first.Reversed()) : shell.first);
    }
    reshape.Replace(it.Current(), turned);
    changed = true;
  }
  return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape(changed ? reshape.Apply(shape) : shape));
}

// The shape with its orientation reversed: every face of it pointing the
// other way. Added for parcad, to make an inside-out solid on purpose.
inline std::unique_ptr<TopoDS_Shape> Shape_reversed(const TopoDS_Shape &shape) {
  return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape(shape.Reversed()));
}

// The one solid a shape is, or bounds: a solid as it is, or a single closed
// shell made into a solid and turned right side out. An empty shape
// otherwise. Added for parcad: `BRepOffsetAPI_MakeThickSolid` hands back the
// inward offset of a boolean result as a bare shell, which every later
// boolean reads as nothing.
inline std::unique_ptr<TopoDS_Shape> Shape_closed_solid(const TopoDS_Shape &shape) {
  TopExp_Explorer solids(shape, TopAbs_SOLID);
  if (solids.More()) {
    const TopoDS_Shape solid = solids.Current();
    solids.Next();
    return std::unique_ptr<TopoDS_Shape>(solids.More() ? new TopoDS_Shape() : new TopoDS_Shape(solid));
  }
  TopExp_Explorer shells(shape, TopAbs_SHELL);
  if (!shells.More()) {
    return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape());
  }
  const TopoDS_Shell shell = TopoDS::Shell(shells.Current());
  shells.Next();
  if (shells.More() || !BRep_Tool::IsClosed(shell)) {
    return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape());
  }
  BRep_Builder builder;
  TopoDS_Solid solid;
  builder.MakeSolid(solid);
  builder.Add(solid, shell);
  BRepLib::OrientClosedSolid(solid);
  return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape(solid));
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

// Measured geometry of a shape as JSON, for reading a foreign B-rep — a STEP
// export from another CAD system — back into numbers a part can be authored
// from. Added for parcad; see PARCAD-CHANGES.md.
//
// One call returns the whole document because the caller sits on the far side
// of a process boundary (parcad's kernel worker): per-face accessors would
// mean a worker round trip per face. The schema is consumed by typed structs
// in parcad's protocol.rs, so a change here fails loudly over there.
//
// Per solid: exact mass properties (BRepGProp, not a tessellation), bounding
// box, and every face with its surface geometry — plane origin/normal,
// cylinder/cone/sphere/torus axes and radii, and for a B-spline surface the
// full pole grid with knots and multiplicities, which is the data a loft
// section has to be reverse-measured from. Each face carries its boundary
// wires in traversal order with orientation applied, so a wire of straight
// lines reads directly as a polygon.
namespace parcad_geometry_json {

inline void write_xyz(std::ostringstream &out, double x, double y, double z) {
  out << "[" << x << "," << y << "," << z << "]";
}

inline void write_pnt(std::ostringstream &out, const gp_Pnt &p) {
  write_xyz(out, p.X(), p.Y(), p.Z());
}

inline void write_dir(std::ostringstream &out, const gp_Dir &d) {
  write_xyz(out, d.X(), d.Y(), d.Z());
}

// A face's outward normal direction flips with its orientation; report the
// outward one, because a draft angle read off an inward normal is a sign error
// nobody catches downstream.
inline gp_Dir face_axis(const TopoDS_Face &face, gp_Dir axis) {
  return face.Orientation() == TopAbs_REVERSED ? axis.Reversed() : axis;
}

inline void write_surface(std::ostringstream &out, const TopoDS_Face &face) {
  Handle(Geom_Surface) surf = BRep_Tool::Surface(face);
  while (!surf.IsNull() && surf->DynamicType() == STANDARD_TYPE(Geom_RectangularTrimmedSurface)) {
    surf = Handle(Geom_RectangularTrimmedSurface)::DownCast(surf)->BasisSurface();
  }
  if (surf.IsNull()) {
    out << "{\"kind\":\"other\",\"name\":\"null\"}";
    return;
  }
  if (surf->DynamicType() == STANDARD_TYPE(Geom_Plane)) {
    const gp_Pln pln = Handle(Geom_Plane)::DownCast(surf)->Pln();
    out << "{\"kind\":\"plane\",\"origin\":";
    write_pnt(out, pln.Location());
    out << ",\"normal\":";
    write_dir(out, face_axis(face, pln.Axis().Direction()));
    out << "}";
  } else if (surf->DynamicType() == STANDARD_TYPE(Geom_CylindricalSurface)) {
    const gp_Cylinder cyl = Handle(Geom_CylindricalSurface)::DownCast(surf)->Cylinder();
    out << "{\"kind\":\"cylinder\",\"origin\":";
    write_pnt(out, cyl.Location());
    out << ",\"axis\":";
    write_dir(out, cyl.Axis().Direction());
    out << ",\"radius\":" << cyl.Radius() << "}";
  } else if (surf->DynamicType() == STANDARD_TYPE(Geom_ConicalSurface)) {
    const gp_Cone cone = Handle(Geom_ConicalSurface)::DownCast(surf)->Cone();
    out << "{\"kind\":\"cone\",\"origin\":";
    write_pnt(out, cone.Location());
    out << ",\"axis\":";
    write_dir(out, cone.Axis().Direction());
    out << ",\"radius\":" << cone.RefRadius()
        << ",\"half_angle_deg\":" << cone.SemiAngle() * 180.0 / M_PI << "}";
  } else if (surf->DynamicType() == STANDARD_TYPE(Geom_SphericalSurface)) {
    const gp_Sphere sph = Handle(Geom_SphericalSurface)::DownCast(surf)->Sphere();
    out << "{\"kind\":\"sphere\",\"center\":";
    write_pnt(out, sph.Location());
    out << ",\"radius\":" << sph.Radius() << "}";
  } else if (surf->DynamicType() == STANDARD_TYPE(Geom_ToroidalSurface)) {
    const gp_Torus tor = Handle(Geom_ToroidalSurface)::DownCast(surf)->Torus();
    out << "{\"kind\":\"torus\",\"center\":";
    write_pnt(out, tor.Location());
    out << ",\"axis\":";
    write_dir(out, tor.Axis().Direction());
    out << ",\"major_radius\":" << tor.MajorRadius()
        << ",\"minor_radius\":" << tor.MinorRadius() << "}";
  } else if (surf->DynamicType() == STANDARD_TYPE(Geom_BSplineSurface)) {
    Handle(Geom_BSplineSurface) bs = Handle(Geom_BSplineSurface)::DownCast(surf);
    out << "{\"kind\":\"nurbs\",\"u_degree\":" << bs->UDegree()
        << ",\"v_degree\":" << bs->VDegree()
        << ",\"rational\":" << ((bs->IsURational() || bs->IsVRational()) ? "true" : "false");
    out << ",\"u_knots\":[";
    for (int i = 1; i <= bs->NbUKnots(); ++i) {
      out << (i > 1 ? "," : "") << bs->UKnot(i);
    }
    out << "],\"v_knots\":[";
    for (int i = 1; i <= bs->NbVKnots(); ++i) {
      out << (i > 1 ? "," : "") << bs->VKnot(i);
    }
    out << "],\"u_mults\":[";
    for (int i = 1; i <= bs->NbUKnots(); ++i) {
      out << (i > 1 ? "," : "") << bs->UMultiplicity(i);
    }
    out << "],\"v_mults\":[";
    for (int i = 1; i <= bs->NbVKnots(); ++i) {
      out << (i > 1 ? "," : "") << bs->VMultiplicity(i);
    }
    out << "],\"poles\":[";
    for (int i = 1; i <= bs->NbUPoles(); ++i) {
      out << (i > 1 ? "," : "") << "[";
      for (int j = 1; j <= bs->NbVPoles(); ++j) {
        if (j > 1) {
          out << ",";
        }
        write_pnt(out, bs->Pole(i, j));
      }
      out << "]";
    }
    out << "]}";
  } else {
    out << "{\"kind\":\"other\",\"name\":\"" << surf->DynamicType()->Name() << "\"}";
  }
}

inline void write_edge(std::ostringstream &out, const TopoDS_Edge &edge) {
  double u0 = 0.0, u1 = 0.0;
  Handle(Geom_Curve) curve = BRep_Tool::Curve(edge, u0, u1);
  while (!curve.IsNull() && curve->DynamicType() == STANDARD_TYPE(Geom_TrimmedCurve)) {
    curve = Handle(Geom_TrimmedCurve)::DownCast(curve)->BasisCurve();
  }
  if (curve.IsNull()) {
    out << "{\"kind\":\"other\",\"name\":\"no-3d-curve\",\"a\":[0,0,0],\"b\":[0,0,0],\"samples\":[]}";
    return;
  }
  // Endpoints in the wire's direction of travel, so consecutive edges chain
  // a -> b -> a -> b and a loop of lines reads off as an ordered polygon.
  const bool reversed = edge.Orientation() == TopAbs_REVERSED;
  const gp_Pnt a = curve->Value(reversed ? u1 : u0);
  const gp_Pnt b = curve->Value(reversed ? u0 : u1);
  if (curve->DynamicType() == STANDARD_TYPE(Geom_Line)) {
    out << "{\"kind\":\"line\",\"a\":";
    write_pnt(out, a);
    out << ",\"b\":";
    write_pnt(out, b);
    out << "}";
    return;
  }
  if (curve->DynamicType() == STANDARD_TYPE(Geom_Circle)) {
    const gp_Circ circ = Handle(Geom_Circle)::DownCast(curve)->Circ();
    out << "{\"kind\":\"circle\",\"center\":";
    write_pnt(out, circ.Location());
    out << ",\"axis\":";
    write_dir(out, circ.Axis().Direction());
    out << ",\"radius\":" << circ.Radius() << ",\"a\":";
    write_pnt(out, a);
    out << ",\"b\":";
    write_pnt(out, b);
    out << "}";
    return;
  }
  out << "{\"kind\":\"other\",\"name\":\"" << curve->DynamicType()->Name() << "\",\"a\":";
  write_pnt(out, a);
  out << ",\"b\":";
  write_pnt(out, b);
  out << ",\"samples\":[";
  const int samples = 16;
  for (int i = 0; i <= samples; ++i) {
    const double t = static_cast<double>(reversed ? samples - i : i) / samples;
    if (i > 0) {
      out << ",";
    }
    write_pnt(out, curve->Value(u0 + (u1 - u0) * t));
  }
  out << "]}";
}

// Faces sharing an edge with this one, as indices into the solid's face order.
// `edge_faces` is built once per solid rather than per face.
inline void write_neighbours(std::ostringstream &out, const TopoDS_Face &face,
                             const IndexedMapOfShape &faces,
                             const IndexedDataMapOfShapeListOfShape &edge_faces) {
  // A pair meeting along several edges is named once, not once per edge.
  std::set<int> neighbours;
  const int self = faces.FindIndex(face);
  for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
    if (!edge_faces.Contains(e.Current())) {
      continue;
    }
    const ListOfShape &touching = edge_faces.FindFromKey(e.Current());
    for (const TopoDS_Shape &neighbour : touching) {
      const int index = faces.FindIndex(neighbour);
      // A seam edge lists its own face twice; a face is not its own neighbour.
      if (index > 0 && index != self) {
        neighbours.insert(index - 1);
      }
    }
  }
  out << "[";
  bool first = true;
  for (const int index : neighbours) {
    out << (first ? "" : ",") << index;
    first = false;
  }
  out << "]";
}

// Surface kind and placement only. `direction` is a plane's outward normal or
// the axis of anything turned about one; both it and `radius` are absent where
// the surface has no single one. Does not descend into a B-spline's poles.
inline void write_surface_placement(std::ostringstream &out, const TopoDS_Face &face) {
  Handle(Geom_Surface) surf = BRep_Tool::Surface(face);
  while (!surf.IsNull() && surf->DynamicType() == STANDARD_TYPE(Geom_RectangularTrimmedSurface)) {
    surf = Handle(Geom_RectangularTrimmedSurface)::DownCast(surf)->BasisSurface();
  }
  if (surf.IsNull()) {
    out << "{\"kind\":\"other\"}";
    return;
  }
  if (surf->DynamicType() == STANDARD_TYPE(Geom_Plane)) {
    const gp_Pln pln = Handle(Geom_Plane)::DownCast(surf)->Pln();
    out << "{\"kind\":\"plane\",\"direction\":";
    write_dir(out, face_axis(face, pln.Axis().Direction()));
    out << "}";
  } else if (surf->DynamicType() == STANDARD_TYPE(Geom_CylindricalSurface)) {
    const gp_Cylinder cyl = Handle(Geom_CylindricalSurface)::DownCast(surf)->Cylinder();
    out << "{\"kind\":\"cylinder\",\"direction\":";
    write_dir(out, cyl.Axis().Direction());
    out << ",\"radius\":" << cyl.Radius() << "}";
  } else if (surf->DynamicType() == STANDARD_TYPE(Geom_ConicalSurface)) {
    const gp_Cone cone = Handle(Geom_ConicalSurface)::DownCast(surf)->Cone();
    out << "{\"kind\":\"cone\",\"direction\":";
    write_dir(out, cone.Axis().Direction());
    out << ",\"radius\":" << cone.RefRadius() << "}";
  } else if (surf->DynamicType() == STANDARD_TYPE(Geom_SphericalSurface)) {
    const gp_Sphere sph = Handle(Geom_SphericalSurface)::DownCast(surf)->Sphere();
    out << "{\"kind\":\"sphere\",\"radius\":" << sph.Radius() << "}";
  } else if (surf->DynamicType() == STANDARD_TYPE(Geom_ToroidalSurface)) {
    const gp_Torus tor = Handle(Geom_ToroidalSurface)::DownCast(surf)->Torus();
    out << "{\"kind\":\"torus\",\"direction\":";
    write_dir(out, tor.Axis().Direction());
    out << ",\"radius\":" << tor.MajorRadius() << "}";
  } else if (surf->DynamicType() == STANDARD_TYPE(Geom_BSplineSurface)) {
    out << "{\"kind\":\"nurbs\"}";
  } else {
    out << "{\"kind\":\"other\"}";
  }
}

inline void write_face(std::ostringstream &out, const TopoDS_Face &face,
                       const IndexedMapOfShape &faces,
                       const IndexedDataMapOfShapeListOfShape &edge_faces) {
  // Exact from the B-rep: a tessellated area is short by the chord error.
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  const gp_Pnt centroid = props.CentreOfMass();

  out << "{\"area_mm2\":" << props.Mass() << ",\"centroid\":";
  write_pnt(out, centroid);
  out << ",\"adjacent\":";
  write_neighbours(out, face, faces, edge_faces);
  out << ",\"surface\":";
  write_surface(out, face);
  out << ",\"wires\":[";
  const TopoDS_Wire outer = BRepTools::OuterWire(face);
  int wi = 0;
  for (TopExp_Explorer w(face, TopAbs_WIRE); w.More(); w.Next(), ++wi) {
    const TopoDS_Wire &wire = TopoDS::Wire(w.Current());
    out << (wi > 0 ? "," : "") << "{\"outer\":" << (wire.IsSame(outer) ? "true" : "false")
        << ",\"edges\":[";
    int ei = 0;
    for (BRepTools_WireExplorer e(wire, face); e.More(); e.Next(), ++ei) {
      if (ei > 0) {
        out << ",";
      }
      write_edge(out, e.Current());
    }
    out << "]}";
  }
  out << "]}";
}

inline void write_solid(std::ostringstream &out, const TopoDS_Shape &solid) {
  GProp_GProps volume_props;
  BRepGProp::VolumeProperties(solid, volume_props);
  GProp_GProps area_props;
  BRepGProp::SurfaceProperties(solid, area_props);
  Bnd_Box box;
  BRepBndLib::AddOptimal(solid, box, /*useTriangulation*/ false, /*useShapeTolerance*/ false);
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  if (!box.IsVoid()) {
    box.Get(x0, y0, z0, x1, y1, z1);
  }
  out << "{\"volume_mm3\":" << volume_props.Mass() << ",\"area_mm2\":" << area_props.Mass()
      << ",\"bbox_min\":";
  write_xyz(out, x0, y0, z0);
  out << ",\"bbox_max\":";
  write_xyz(out, x1, y1, z1);
  // Map order matches the write order below, so index-1 is the reader's
  // position. Checked by `face_adjacency_is_symmetric_and_indexed_as_written`.
  IndexedMapOfShape faces;
  TopExp::MapShapes(solid, TopAbs_FACE, faces);
  IndexedDataMapOfShapeListOfShape edge_faces;
  TopExp::MapShapesAndAncestors(solid, TopAbs_EDGE, TopAbs_FACE, edge_faces);

  out << ",\"faces\":[";
  int fi = 0;
  for (TopExp_Explorer f(solid, TopAbs_FACE); f.More(); f.Next(), ++fi) {
    if (fi > 0) {
      out << ",";
    }
    write_face(out, TopoDS::Face(f.Current()), faces, edge_faces);
  }
  out << "]}";
}

} // namespace parcad_geometry_json

// What each face of the first solid *is*, without its boundary or pole grid.
// Why this exists rather than a flag on the full writer, with the measurements:
// docs/GOTCHAS.md, "Two writers for measured geometry".
inline rust::String Shape_faces_json(const TopoDS_Shape &shape) {
  using namespace parcad_geometry_json;
  std::ostringstream out;
  out.precision(15);
  out << "[";
  int written = 0;
  for (TopExp_Explorer s(shape, TopAbs_SOLID); s.More(); s.Next()) {
    // First solid only: concatenating a second would renumber the faces.
    const TopoDS_Shape &solid = s.Current();
    IndexedMapOfShape faces;
    TopExp::MapShapes(solid, TopAbs_FACE, faces);
    IndexedDataMapOfShapeListOfShape edge_faces;
    TopExp::MapShapesAndAncestors(solid, TopAbs_EDGE, TopAbs_FACE, edge_faces);

    for (TopExp_Explorer f(solid, TopAbs_FACE); f.More(); f.Next(), ++written) {
      const TopoDS_Face &face = TopoDS::Face(f.Current());
      GProp_GProps props;
      BRepGProp::SurfaceProperties(face, props);
      out << (written > 0 ? "," : "") << "{\"area_mm2\":" << props.Mass() << ",\"centroid\":";
      write_pnt(out, props.CentreOfMass());
      out << ",\"adjacent\":";
      write_neighbours(out, face, faces, edge_faces);
      out << ",\"surface\":";
      write_surface_placement(out, face);
      out << "}";
    }
    break;
  }
  out << "]";
  return rust::String(out.str());
}

inline rust::String Shape_geometry_json(const TopoDS_Shape &shape) {
  using namespace parcad_geometry_json;
  std::ostringstream out;
  out.precision(15);
  out << "{\"solids\":[";
  int si = 0;
  int solid_faces = 0;
  for (TopExp_Explorer s(shape, TopAbs_SOLID); s.More(); s.Next(), ++si) {
    if (si > 0) {
      out << ",";
    }
    write_solid(out, s.Current());
    for (TopExp_Explorer f(s.Current(), TopAbs_FACE); f.More(); f.Next()) {
      ++solid_faces;
    }
  }
  int all_faces = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    ++all_faces;
  }
  out << "],\"free_faces\":" << (all_faces - solid_faces) << "}";
  return rust::String(out.str());
}

// Drop the unused half of a stale seam representation. A boolean can leave an
// edge that was a cylinder's seam bordering the face only on one side — the
// wire references it once — while the edge still carries both pcurves. That
// dead second pcurve is what stops UnifySameDomain from merging the edge with
// a collinear neighbour: the concatenation cannot join a curve to both
// representations and raises. Genuine seams appear twice in their face's wires
// and are left untouched. Added for parcad; see PARCAD-CHANGES.md.
//
// Mutates the shape in place (only representation data, never geometry) and
// returns how many pcurves were dropped.
inline int Shape_drop_unused_seam_pcurves(const TopoDS_Shape &shape) {
  int dropped = 0;
  BRep_Builder builder;
  ShapeBuild_Edge sbe;
  IndexedDataMapOfShapeListOfShape edge_faces;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, edge_faces);
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face &face = TopoDS::Face(f.Current());
    TopLoc_Location face_loc;
    const Handle(Geom_Surface) &face_surface = BRep_Tool::Surface(face, face_loc);
    // Only cylindrical faces, and below only line generators: that is the
    // configuration this heals — a plane-tangent cut leaving half a seam.
    // On doubly periodic surfaces a boolean legitimately leaves a full
    // boundary circle carrying both representations even though the wire
    // uses it once, and the mesher needs them: stripping one opened the
    // torus-gland groove by 168 mesh edges.
    {
      BRepAdaptor_Surface bas(face, false);
      if (bas.GetType() != GeomAbs_Cylinder) {
        continue;
      }
    }
    // Count how often each closed-flagged edge appears in the face's wires,
    // and with which orientation.
    NCollection_DataMap<TopoDS_Shape, int, TopTools_ShapeMapHasher> count;
    NCollection_DataMap<TopoDS_Shape, TopAbs_Orientation, TopTools_ShapeMapHasher> orient;
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge &edge = TopoDS::Edge(e.Current());
      if (!BRep_Tool::IsClosed(edge, face)) {
        continue;
      }
      double cf = 0.0, cl = 0.0;
      Handle(Geom_Curve) c3 = BRep_Tool::Curve(edge, cf, cl);
      while (!c3.IsNull() && c3->DynamicType() == STANDARD_TYPE(Geom_TrimmedCurve)) {
        c3 = Handle(Geom_TrimmedCurve)::DownCast(c3)->BasisCurve();
      }
      if (c3.IsNull() || c3->DynamicType() != STANDARD_TYPE(Geom_Line)) {
        continue;
      }
      int n = 0;
      count.Find(edge, n);
      count.Bind(edge, n + 1);
      orient.Bind(edge, edge.Orientation());
    }
    for (NCollection_DataMap<TopoDS_Shape, int, TopTools_ShapeMapHasher>::Iterator it(count);
         it.More(); it.Next()) {
      if (it.Value() != 1) {
        continue; // a genuine seam, or something stranger — leave it alone
      }
      // Two pieces of one split cylinder share its surface, so a pcurve is
      // stored per surface, not per face: a seam each piece uses once is still
      // using both representations, and dropping one broke the other piece.
      bool shared_surface = false;
      for (const TopoDS_Shape &other : edge_faces.FindFromKey(it.Key())) {
        TopLoc_Location other_loc;
        if (!other.IsSame(face) &&
            BRep_Tool::Surface(TopoDS::Face(other), other_loc) == face_surface &&
            other_loc == face_loc) {
          shared_surface = true;
        }
      }
      if (shared_surface) {
        continue;
      }
      TopoDS_Edge edge = TopoDS::Edge(it.Key());
      edge.Orientation(orient.Find(it.Key()));
      double pf = 0.0, pl = 0.0;
      Handle(Geom2d_Curve) used = BRep_Tool::CurveOnSurface(edge, face, pf, pl);
      if (used.IsNull()) {
        continue;
      }
      double tol = BRep_Tool::Tolerance(edge);
      sbe.RemovePCurve(edge, face);
      builder.UpdateEdge(edge, used, face, tol);
      builder.Range(edge, face, pf, pl);
      ++dropped;
    }
  }
  return dropped;
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
  BRepCheck_Analyzer analyzer(shape, true, false, exact);
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
      for (NCollection_List<BRepCheck_Status>::Iterator status(result->Status()); status.More(); status.Next()) {
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

// BOPAlgo_CheckerSI: does any part of this shape run into another part of it?
// Added for parcad. BRepCheck_Analyzer judges each face against its own
// boundary and passes a solid whose faces cross each other; the checker
// intersects every sub-shape with every other, as a boolean would, and
// reports each pair that meets anywhere but through a sub-shape they share.

// The checker's candidate pairs, kept only where one side is, or lies on, a
// face in `fresh`: faces an operation left alone met nothing before it, and
// intersecting them again is most of what the check would cost.
class ParcadFreshPairsSI : public BOPDS_IteratorSI {
public:
  ParcadFreshPairsSI(const Handle(NCollection_BaseAllocator) &allocator,
                     const NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> *fresh)
      : BOPDS_IteratorSI(allocator), fresh_(fresh) {}

protected:
  void Intersect(const occ::handle<IntTools_Context> &context, const bool obb, const double fuzzy) override {
    BOPDS_IteratorSI::Intersect(context, obb, fuzzy);
    if (fresh_ == nullptr) {
      return;
    }
    const int count = myDS->NbSourceShapes();
    std::vector<char> touched(count, 0);
    for (int i = 0; i < count; ++i) {
      const BOPDS_ShapeInfo &info = myDS->ShapeInfo(i);
      if (info.ShapeType() != TopAbs_FACE || !fresh_->Contains(info.Shape())) {
        continue;
      }
      std::vector<int> stack{i};
      while (!stack.empty()) {
        const int at = stack.back();
        stack.pop_back();
        if (touched[at]) {
          continue;
        }
        touched[at] = 1;
        for (NCollection_List<int>::Iterator sub(myDS->ShapeInfo(at).SubShapes()); sub.More(); sub.Next()) {
          stack.push_back(sub.Value());
        }
      }
    }
    for (int list = 0; list < myLists.Length(); ++list) {
      NCollection_DynamicArray<BOPDS_Pair> kept;
      for (NCollection_DynamicArray<BOPDS_Pair>::Iterator it(myLists(list)); it.More(); it.Next()) {
        int a, b;
        it.Value().Indices(a, b);
        if (touched[a] || touched[b]) {
          kept.Append(it.Value());
        }
      }
      myLists(list) = kept;
    }
  }

private:
  const NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> *fresh_;
};

class ParcadCheckerSI : public BOPAlgo_CheckerSI {
public:
  explicit ParcadCheckerSI(const NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> *fresh) : fresh_(fresh) {}

  // BOPAlgo_CheckerSI::Perform, with each surface intersected with itself
  // only where its face is fresh: that pass is most of the cost on a fillet.
  void Perform(const Message_ProgressRange &range = Message_ProgressRange()) override {
    if (fresh_ == nullptr) {
      BOPAlgo_CheckerSI::Perform(range);
      return;
    }
    try {
      OCC_CATCH_SIGNALS
      BOPAlgo_PaveFiller::Perform(range);
      ((NCollection_Map<BOPDS_Pair> &)myDS->Interferences()).Clear();
      CheckFreshFacesOnThemselves();
      if (!HasErrors()) PerformVZ(Message_ProgressRange());
      if (!HasErrors()) PerformEZ(Message_ProgressRange());
      if (!HasErrors()) PerformFZ(Message_ProgressRange());
      if (!HasErrors()) PerformZZ(Message_ProgressRange());
      if (HasErrors()) {
        return;
      }
      PostTreat();
    } catch (Standard_Failure const &) {
      AddError(new BOPAlgo_AlertIntersectionFailed);
    }
  }

private:
  struct SelfIntersection {
    std::vector<TopoDS_Face> *faces;
    std::vector<char> *crossed;
    void operator()(int i) const {
      IntTools_FaceFace intersection;
      intersection.Perform((*faces)[i], (*faces)[i], false);
      (*crossed)[i] = intersection.IsDone() && (intersection.Lines().Length() > 0 || intersection.Points().Length() > 0);
    }
  };

  void CheckFreshFacesOnThemselves() {
    std::vector<TopoDS_Face> faces;
    std::vector<int> indices;
    for (int i = 0; i < myDS->NbSourceShapes(); ++i) {
      const BOPDS_ShapeInfo &info = myDS->ShapeInfo(i);
      if (info.ShapeType() != TopAbs_FACE || !fresh_->Contains(info.Shape())) {
        continue;
      }
      const TopoDS_Face &face = TopoDS::Face(info.Shape());
      BRepAdaptor_Surface surface(face, false);
      const GeomAbs_SurfaceType type = surface.GetType();
      // The kinds BOPAlgo_CheckerSI itself trusts not to meet themselves.
      if (type == GeomAbs_Plane || type == GeomAbs_Cylinder || type == GeomAbs_Cone || type == GeomAbs_Sphere) {
        continue;
      }
      if (type == GeomAbs_Torus && surface.Torus().MajorRadius() > surface.Torus().MinorRadius() + Precision::Confusion()) {
        continue;
      }
      faces.push_back(face);
      indices.push_back(i);
    }
    std::vector<char> crossed(faces.size(), 0);
    OSD_Parallel::For(0, static_cast<int>(faces.size()), SelfIntersection{&faces, &crossed}, !myRunParallel);
    NCollection_Map<BOPDS_Pair> &pairs = (NCollection_Map<BOPDS_Pair> &)myDS->Interferences();
    for (size_t k = 0; k < faces.size(); ++k) {
      if (crossed[k]) {
        pairs.Add(BOPDS_Pair(indices[k], indices[k]));
      }
    }
  }

public:

protected:
  void Init(const Message_ProgressRange &) override {
    Clear();
    myDS = new BOPDS_DS(myAllocator);
    myDS->SetArguments(myArguments);
    myDS->Init(myFuzzyValue);
    myContext = new IntTools_Context;
    ParcadFreshPairsSI *iterator = new ParcadFreshPairsSI(myAllocator, fresh_);
    iterator->SetDS(myDS);
    iterator->Prepare(myContext, myUseOBB, myFuzzyValue);
    iterator->UpdateByLevelOfCheck(myLevelOfCheck);
    myIterator = iterator;
  }

private:
  const NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> *fresh_;
};

// Returns "" when nothing meets. Otherwise the first line is
// "<pairs> <aborted>", then one "<kind> <kind> <x> <y> <z>" line for each of
// the first `located` pairs, the point being where the two come closest.
inline rust::String parcad_self_interference(const TopoDS_Shape &shape, double fuzzy, int located,
                                             const NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> *fresh) {
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(shape);
  ParcadCheckerSI checker(fresh);
  checker.SetArguments(arguments);
  checker.SetNonDestructive(true);
  // Face pairs are intersected independently; in parallel the check costs a
  // quarter to a sixth of its serial time on the fuzzed treatments.
  checker.SetRunParallel(true);
  checker.SetFuzzyValue(fuzzy);
  checker.Perform();
  const bool aborted = checker.HasErrors();
  if (checker.PDS() == nullptr) {
    return rust::String(aborted ? "0 1\n" : "");
  }
  const BOPDS_DS &ds = *checker.PDS();
  std::ostringstream lines;
  lines.precision(10);
  int pairs = 0;
  for (NCollection_Map<BOPDS_Pair>::Iterator it(ds.Interferences()); it.More(); it.Next()) {
    int n1, n2;
    it.Value().Indices(n1, n2);
    if (ds.IsNewShape(n1) || ds.IsNewShape(n2)) {
      continue;
    }
    if (pairs < located) {
      const TopoDS_Shape &a = ds.Shape(n1);
      const TopoDS_Shape &b = ds.Shape(n2);
      auto kind = [](const TopoDS_Shape &s) {
        switch (s.ShapeType()) {
        case TopAbs_VERTEX: return "vertex";
        case TopAbs_EDGE: return "edge";
        case TopAbs_FACE: return "face";
        default: return "shape";
        }
      };
      if (n1 == n2 && a.ShapeType() == TopAbs_FACE) {
        // A face that meets itself: where, from its own intersection.
        IntTools_FaceFace itself;
        itself.Perform(TopoDS::Face(a), TopoDS::Face(a), false);
        lines << kind(a) << " itself";
        if (itself.IsDone() && itself.Lines().Length() > 0) {
          const Handle(Geom_Curve) &curve = itself.Lines().First().Curve();
          if (!curve.IsNull()) {
            const gp_Pnt p = curve->Value(0.5 * (curve->FirstParameter() + curve->LastParameter()));
            lines << " " << p.X() << " " << p.Y() << " " << p.Z();
          }
        } else if (itself.IsDone() && itself.Points().Length() > 0) {
          const gp_Pnt p = itself.Points().First().P1().Pnt();
          lines << " " << p.X() << " " << p.Y() << " " << p.Z();
        }
      } else {
        BRepExtrema_DistShapeShape search;
        search.SetFlag(Extrema_ExtFlag_MIN);
        search.LoadS1(a);
        search.LoadS2(b);
        search.Perform();
        lines << kind(a) << " " << kind(b);
        if (search.IsDone() && search.NbSolution() > 0) {
          const gp_Pnt p = search.PointOnShape1(1);
          lines << " " << p.X() << " " << p.Y() << " " << p.Z();
        }
      }
      lines << "\n";
    }
    ++pairs;
  }
  if (pairs == 0 && !aborted) {
    return rust::String("");
  }
  std::ostringstream out;
  out << pairs << " " << (aborted ? 1 : 0) << "\n" << lines.str();
  return rust::String(out.str());
}

inline rust::String Shape_self_interference_report(const TopoDS_Shape &shape, double fuzzy, int located) {
  return parcad_self_interference(shape, fuzzy, located, nullptr);
}

// The same question asked of what an operation changed: the faces of `after`
// that are not faces of `before`, against every face whose box meets one of
// theirs. Only pairs with a changed face on one side are intersected, since
// the faces an operation left alone met nothing before it. Added for parcad.
inline rust::String Shape_self_interference_since(const TopoDS_Shape &after, const TopoDS_Shape &before,
                                                  double fuzzy, int located) {
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> kept;
  for (TopExp_Explorer it(before, TopAbs_FACE); it.More(); it.Next()) {
    kept.Add(it.Current());
  }
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> fresh;
  std::vector<Bnd_Box> regions;
  for (TopExp_Explorer it(after, TopAbs_FACE); it.More(); it.Next()) {
    if (kept.Contains(it.Current())) {
      continue;
    }
    fresh.Add(it.Current());
    Bnd_Box box;
    BRepBndLib::Add(it.Current(), box);
    regions.push_back(box);
  }
  if (fresh.IsEmpty()) {
    return rust::String("");
  }
  BRep_Builder builder;
  TopoDS_Compound near;
  builder.MakeCompound(near);
  for (TopExp_Explorer it(after, TopAbs_FACE); it.More(); it.Next()) {
    Bnd_Box box;
    BRepBndLib::Add(it.Current(), box);
    for (const Bnd_Box &region : regions) {
      if (!box.IsOut(region)) {
        builder.Add(near, it.Current());
        break;
      }
    }
  }
  return parcad_self_interference(near, fuzzy, located, &fresh);
}
