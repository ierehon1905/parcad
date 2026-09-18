#pragma once

// PARCAD: surface modelling — open shells built from wires, sewn, split,
// patched, offset and thickened, and the measurements that report on them.
// Every builder returns the kernel's own shape and, where it has one, the
// face history as (input face, output face) pairs in `TopExp::MapShapes`
// order, which is the numbering `Shape::face_map` uses. Nothing here decides
// whether a result is acceptable: the caller measures and refuses. See
// PARCAD-CHANGES.md.

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <Bnd_Box.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRepGProp.hxx>
#include <GProp_GProps.hxx>
#include <BRepLib_FindSurface.hxx>
#include <BRepOffsetAPI_MakeFilling.hxx>
#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <Geom_BSplineSurface.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <Geom2d_Curve.hxx>
#include <Precision.hxx>
#include <Geom_OffsetSurface.hxx>
#include <Poly_PolygonOnTriangulation.hxx>
#include <Poly_Triangulation.hxx>
#include <Geom_Plane.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <TopLoc_Location.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <NCollection_HSequence.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <ShapeAnalysis_FreeBounds.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax1.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "rust/cxx.h"

#include <algorithm>
#include <cmath>
#include <functional>
#include <map>
#include <limits>
#include <memory>
#include <stdexcept>
#include <string>
#include <typeinfo>
#include <vector>

using ParcadShapeMap = NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher>;
using ParcadAncestors =
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

inline std::string parcad_surfacing_raised(const Standard_Failure& raised) {
  const char* what = raised.what();
  std::string message = what ? what : "";
  if (message.empty()) {
    message = std::string("the kernel raised ") + typeid(raised).name();
  }
  return message;
}

// Runs `body`, turning the kernel's exceptions into ones cxx carries.
template <typename F>
auto parcad_surfacing_guard(const char* what, F body) -> decltype(body()) {
  try {
    return body();
  } catch (const Standard_Failure& raised) {
    throw std::runtime_error(std::string(what) + " raised: " + parcad_surfacing_raised(raised));
  }
}

inline std::unique_ptr<TopoDS_Shape> parcad_boxed(const TopoDS_Shape& shape) {
  return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape(shape));
}

// A compound's children in the order they were added.
inline std::vector<TopoDS_Shape> parcad_children(const TopoDS_Shape& shape) {
  std::vector<TopoDS_Shape> out;
  for (TopoDS_Iterator it(shape); it.More(); it.Next()) {
    out.push_back(it.Value());
  }
  return out;
}

inline TopoDS_Compound parcad_compound(const std::vector<TopoDS_Shape>& shapes) {
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const TopoDS_Shape& shape : shapes) {
    builder.Add(compound, shape);
  }
  return compound;
}

// An edge bordered by exactly one face, and not a seam of it.
inline bool parcad_is_free(const TopoDS_Edge& edge, const NCollection_List<TopoDS_Shape>& faces) {
  if (BRep_Tool::Degenerated(edge) || faces.Extent() != 1) {
    return false;
  }
  return !BRep_Tool::IsClosed(edge, TopoDS::Face(faces.First()));
}

inline double parcad_edge_length(const TopoDS_Edge& edge) {
  BRepAdaptor_Curve curve(edge);
  return GCPnts_AbscissaPoint::Length(curve);
}

// [solids, sheets — faces connected through shared edges —, faces, free
// edges, free edge length, edges shared by more than two faces, faces in no
// solid, closed free loops, open free chains]
inline rust::Vec<double> parcad_census(const TopoDS_Shape& shape) {
  return parcad_surfacing_guard("counting the shape", [&]() {
    ParcadShapeMap solids;
    ParcadShapeMap faces;
    TopExp::MapShapes(shape, TopAbs_SOLID, solids);
    TopExp::MapShapes(shape, TopAbs_FACE, faces);
    ParcadAncestors edge_faces;
    TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_EDGE, TopAbs_FACE, edge_faces);
    std::vector<int> parent(faces.Extent() + 1);
    for (size_t i = 0; i < parent.size(); ++i) {
      parent[i] = int(i);
    }
    std::function<int(int)> root = [&](int i) { return parent[i] == i ? i : (parent[i] = root(parent[i])); };
    for (int i = 1; i <= edge_faces.Extent(); ++i) {
      const NCollection_List<TopoDS_Shape>& around = edge_faces.FindFromIndex(i);
      int first = 0;
      for (const TopoDS_Shape& face : around) {
        const int at = faces.FindIndex(face);
        if (at <= 0) {
          continue;
        }
        if (first == 0) {
          first = at;
        } else {
          parent[root(at)] = root(first);
        }
      }
    }
    double sheets = 0.0;
    for (int i = 1; i <= faces.Extent(); ++i) {
      if (root(i) == i) {
        sheets += 1.0;
      }
    }
    double free_edges = 0.0;
    double free_length = 0.0;
    double multiple = 0.0;
    std::vector<TopoDS_Shape> free_list;
    for (int i = 1; i <= edge_faces.Extent(); ++i) {
      const TopoDS_Edge edge = TopoDS::Edge(edge_faces.FindKey(i));
      const NCollection_List<TopoDS_Shape>& around = edge_faces.FindFromIndex(i);
      if (parcad_is_free(edge, around)) {
        free_edges += 1.0;
        free_length += parcad_edge_length(edge);
        free_list.push_back(edge);
      } else if (around.Extent() > 2) {
        multiple += 1.0;
      }
    }
    ParcadAncestors face_solids;
    TopExp::MapShapesAndAncestors(shape, TopAbs_FACE, TopAbs_SOLID, face_solids);
    double loose = 0.0;
    for (int i = 1; i <= face_solids.Extent(); ++i) {
      if (face_solids.FindFromIndex(i).IsEmpty()) {
        loose += 1.0;
      }
    }
    double closed_loops = 0.0;
    double open_chains = 0.0;
    if (!free_list.empty()) {
      auto edges = new NCollection_HSequence<TopoDS_Shape>();
      for (const TopoDS_Shape& edge : free_list) {
        edges->Append(edge);
      }
      occ::handle<NCollection_HSequence<TopoDS_Shape>> held(edges);
      occ::handle<NCollection_HSequence<TopoDS_Shape>> wires =
          ShapeAnalysis_FreeBounds::ConnectEdgesToWires(held, 1e-7, true);
      for (int i = 1; i <= wires->Length(); ++i) {
        if (BRep_Tool::IsClosed(wires->Value(i))) {
          closed_loops += 1.0;
        } else {
          open_chains += 1.0;
        }
      }
    }
    rust::Vec<double> out;
    for (double x : {double(solids.Extent()), sheets, double(faces.Extent()), free_edges,
                     free_length, multiple, loose, closed_loops, open_chains}) {
      out.push_back(x);
    }
    return out;
  });
}

// The enclosed volume by Gauss–Kronrod integration over every knot span of
// every face, to relative error `eps`: [volume, the error estimate].
inline rust::Vec<double> parcad_volume_by_spans(const TopoDS_Shape& shape, double eps) {
  return parcad_surfacing_guard("integrating the volume", [&]() {
    GProp_GProps props;
    const double error = BRepGProp::VolumePropertiesGK(shape, props, eps, false, true);
    rust::Vec<double> out;
    out.push_back(props.Mass());
    out.push_back(error);
    return out;
  });
}

// Faces whose triangulation does not cover the face: for each face the
// triangles' area in the face's parameter plane against the area the
// triangulation's own boundary — the polygon every edge of the face was
// discretised into — encloses there. A triangulation of that polygon covers
// it exactly, so the two agree to rounding; a mesh of part of the face falls
// short. Returns [face index, triangulated area, boundary area] for every
// face whose two differ by more than `rel` of the boundary's plus its
// perimeter times `Precision::PConfusion`, or that carries no triangulation.
inline rust::Vec<double> parcad_uncovered_faces(const TopoDS_Shape& shape, double rel) {
  return parcad_surfacing_guard("checking the mesh covers every face", [&]() {
    ParcadShapeMap faces;
    TopExp::MapShapes(shape, TopAbs_FACE, faces);
    rust::Vec<double> out;
    for (int i = 1; i <= faces.Extent(); ++i) {
      const TopoDS_Face face = TopoDS::Face(faces.FindKey(i));
      TopLoc_Location location;
      const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(face, location);
      if (mesh.IsNull() || !mesh->HasUVNodes()) {
        for (double x : {double(i - 1), 0.0, 0.0}) {
          out.push_back(x);
        }
        continue;
      }
      double meshed = 0.0;
      for (int t = 1; t <= mesh->NbTriangles(); ++t) {
        int a, b, c;
        mesh->Triangle(t).Get(a, b, c);
        const gp_Pnt2d pa = mesh->UVNode(a);
        const gp_Pnt2d pb = mesh->UVNode(b);
        const gp_Pnt2d pc = mesh->UVNode(c);
        meshed += std::abs((pb.X() - pa.X()) * (pc.Y() - pa.Y()) - (pb.Y() - pa.Y()) * (pc.X() - pa.X())) / 2.0;
      }
      // The boundary, walked edge by edge in the face's own direction, each
      // loop closed in the parameter plane (a seam's two sides are its two
      // polygons), and summed about a node of the face: on a face whose
      // parameters span 1e-5 the terms about the origin cancel to rounding.
      const gp_Pnt2d o = mesh->UVNode(1);
      double boundary = 0.0;
      double perimeter = 0.0;
      bool complete = true;
      const TopoDS_Face forward = TopoDS::Face(face.Oriented(TopAbs_FORWARD));
      for (TopExp_Explorer wires(forward, TopAbs_WIRE); wires.More(); wires.Next()) {
        for (TopExp_Explorer it(wires.Current(), TopAbs_EDGE); it.More(); it.Next()) {
          const TopoDS_Edge edge = TopoDS::Edge(it.Current());
          if (edge.Orientation() != TopAbs_FORWARD && edge.Orientation() != TopAbs_REVERSED) {
            continue;
          }
          // A pole's polygon still bounds the parameter domain, so a degenerate
          // edge is walked like any other.
          occ::handle<Poly_PolygonOnTriangulation> polygon = BRep_Tool::PolygonOnTriangulation(edge, mesh, location);
          if (polygon.IsNull()) {
            complete = false;
            continue;
          }
          // A seam has a polygon on each side, and which one an orientation
          // returns is not the side this walk is on: take the one that starts
          // where the edge's own curve on this face starts.
          double first = 0.0;
          double last = 0.0;
          const occ::handle<Geom2d_Curve> pcurve =
              BRep_Tool::IsClosed(edge, forward) ? BRep_Tool::CurveOnSurface(edge, forward, first, last) : nullptr;
          if (!pcurve.IsNull()) {
            const occ::handle<Poly_PolygonOnTriangulation> other =
                BRep_Tool::PolygonOnTriangulation(TopoDS::Edge(edge.Reversed()), mesh, location);
            const gp_Pnt2d start = pcurve->Value(first);
            if (!other.IsNull() && mesh->UVNode(other->Node(1)).Distance(start) <
                                       mesh->UVNode(polygon->Node(1)).Distance(start)) {
              polygon = other;
            }
          }
          const int count = polygon->NbNodes();
          const bool reversed = edge.Orientation() == TopAbs_REVERSED;
          for (int k = 1; k < count; ++k) {
            const int from = reversed ? count - k + 1 : k;
            const int to = reversed ? count - k : k + 1;
            const gp_Pnt2d p = mesh->UVNode(polygon->Node(from));
            const gp_Pnt2d q = mesh->UVNode(polygon->Node(to));
            boundary += (p.X() - o.X()) * (q.Y() - o.Y()) - (q.X() - o.X()) * (p.Y() - o.Y());
            perimeter += p.Distance(q);
          }
        }
      }
      boundary = std::abs(boundary) / 2.0;
      // A boundary node may sit off its polygon by the kernel's parametric
      // confusion, which on a face a few 1e-6 across is a share of its area.
      const double slack = rel * boundary + perimeter * Precision::PConfusion();
      if (!complete || std::abs(meshed - boundary) > slack) {
        for (double x : {double(i - 1), meshed, boundary}) {
          out.push_back(x);
        }
      }
    }
    return out;
  });
}

// Every free edge of the shape, as a compound in `MapShapes` edge order.
inline std::unique_ptr<TopoDS_Shape> parcad_free_edges(const TopoDS_Shape& shape) {
  return parcad_surfacing_guard("finding the free edges", [&]() {
    ParcadAncestors edge_faces;
    TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_EDGE, TopAbs_FACE, edge_faces);
    std::vector<TopoDS_Shape> free_list;
    for (int i = 1; i <= edge_faces.Extent(); ++i) {
      const TopoDS_Edge edge = TopoDS::Edge(edge_faces.FindKey(i));
      if (parcad_is_free(edge, edge_faces.FindFromIndex(i))) {
        free_list.push_back(edge);
      }
    }
    return parcad_boxed(parcad_compound(free_list));
  });
}

// How a face bends at a point of one of its edges: its outward normal and
// its mean and Gaussian curvature there, signed by that normal.
inline bool parcad_bend_at(const TopoDS_Edge& edge, const TopoDS_Face& face, double t, gp_Pnt& at, gp_Dir& normal,
                           double& mean, double& gauss) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  if (pcurve.IsNull()) {
    return false;
  }
  const gp_Pnt2d uv = pcurve->Value(first + (last - first) * t);
  BRepAdaptor_Surface surface(face);
  BRepLProp_SLProps props(surface, uv.X(), uv.Y(), 2, 1e-9);
  if (!props.IsNormalDefined() || !props.IsCurvatureDefined()) {
    return false;
  }
  const double sign = face.Orientation() == TopAbs_REVERSED ? -1.0 : 1.0;
  normal = props.Normal();
  if (sign < 0.0) {
    normal.Reverse();
  }
  mean = sign * props.MeanCurvature();
  gauss = props.GaussianCurvature();
  at = props.Value();
  return true;
}

// Edges no one could see: two faces meeting there with the same tangent
// plane and the same curvature at points along it, as the pieces of one
// surface do where a loft is cut into bands. A fillet's boundary keeps its
// jump in curvature and stays an edge. Like a closed surface's seam, such an
// edge is a split in the representation, not an edge of the shape.
// Whether `edge` between `first` and `second` is a split in the
// representation: the faces agree on position, normal and bend along it.
inline bool parcad_is_split(const TopoDS_Edge& edge, const TopoDS_Face& first, const TopoDS_Face& second) {
  for (double t : {0.2, 0.5, 0.8}) {
    gp_Pnt p;
    gp_Pnt q;
    gp_Dir n;
    gp_Dir m;
    double h1 = 0.0, k1 = 0.0, h2 = 0.0, k2 = 0.0;
    if (!parcad_bend_at(edge, first, t, p, n, h1, k1) || !parcad_bend_at(edge, second, t, q, m, h2, k2)) {
      return false;
    }
    const double scale = 1.0 + std::abs(h1) + std::abs(h2);
    if (n.Angle(m) > 1e-6 || std::abs(h1 - h2) > 1e-6 * scale ||
        std::abs(k1 - k2) > 1e-6 * scale * scale || p.Distance(q) > 1e-6) {
      return false;
    }
  }
  return true;
}

// Whether two edges lie on one curve: the same underlying curve, or equal
// lines or circles. Any other pair is kept apart rather than guessed at.
inline bool parcad_same_curve(const TopoDS_Edge& a, const TopoDS_Edge& b) {
  TopLoc_Location at_a, at_b;
  double first = 0.0, last = 0.0;
  Handle(Geom_Curve) curve_a = BRep_Tool::Curve(a, at_a, first, last);
  Handle(Geom_Curve) curve_b = BRep_Tool::Curve(b, at_b, first, last);
  if (curve_a.IsNull() || curve_b.IsNull()) {
    return false;
  }
  const auto basis = [](Handle(Geom_Curve) curve) {
    while (auto trimmed = Handle(Geom_TrimmedCurve)::DownCast(curve)) {
      curve = trimmed->BasisCurve();
    }
    return curve;
  };
  if (basis(curve_a) == basis(curve_b) && at_a.IsEqual(at_b)) {
    return true;
  }
  const BRepAdaptor_Curve along_a(a);
  const BRepAdaptor_Curve along_b(b);
  if (along_a.GetType() != along_b.GetType()) {
    return false;
  }
  const double tolerance = std::max({BRep_Tool::Tolerance(a), BRep_Tool::Tolerance(b), Precision::Confusion()});
  switch (along_a.GetType()) {
    case GeomAbs_Line: {
      const gp_Lin line_a = along_a.Line();
      const gp_Lin line_b = along_b.Line();
      return line_a.Direction().IsParallel(line_b.Direction(), Precision::Angular()) &&
             line_a.Distance(line_b.Location()) <= tolerance;
    }
    case GeomAbs_Circle: {
      const gp_Circ circle_a = along_a.Circle();
      const gp_Circ circle_b = along_b.Circle();
      return circle_a.Location().Distance(circle_b.Location()) <= tolerance &&
             std::abs(circle_a.Radius() - circle_b.Radius()) <= tolerance &&
             circle_a.Axis().Direction().IsParallel(circle_b.Axis().Direction(), Precision::Angular());
    }
    default:
      return false;
  }
}

// The edges of the shape as a person reads them, one compound of kernel
// edges each. A seam, a split between faces of one surface and a degenerate
// pole are the representation's, not the shape's, so none is an edge here;
// and where only such an edge ends on a vertex, the vertex is no corner, so
// the two pieces meeting there are one edge when they lie on one curve. A
// sphere's seam ending on the rim of a boss fused onto it no longer halves
// the rim. Added for parcad; see PARCAD-CHANGES.md.
inline std::unique_ptr<std::vector<TopoDS_Shape>> parcad_logical_edges(const TopoDS_Shape& shape) {
  return parcad_surfacing_guard("grouping the edges", [&]() {
    ParcadAncestors edge_faces;
    TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_EDGE, TopAbs_FACE, edge_faces);
    const int count = edge_faces.Extent();
    std::vector<bool> hidden(count + 1, false);
    for (int i = 1; i <= count; ++i) {
      const TopoDS_Edge edge = TopoDS::Edge(edge_faces.FindKey(i));
      const NCollection_List<TopoDS_Shape>& around = edge_faces.FindFromIndex(i);
      hidden[i] = BRep_Tool::Degenerated(edge) ||
                  (around.Extent() == 1 && BRep_Tool::IsClosed(edge, TopoDS::Face(around.First()))) ||
                  (around.Extent() == 2 &&
                   parcad_is_split(edge, TopoDS::Face(around.First()), TopoDS::Face(around.Last())));
    }

    std::vector<int> group(count + 1);
    for (int i = 0; i <= count; ++i) {
      group[i] = i;
    }
    const auto root = [&](int i) {
      while (group[i] != i) {
        i = group[i] = group[group[i]];
      }
      return i;
    };

    ParcadAncestors vertex_edges;
    TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_VERTEX, TopAbs_EDGE, vertex_edges);
    for (int v = 1; v <= vertex_edges.Extent(); ++v) {
      std::vector<int> shown;
      bool passes_hidden = false;
      for (const TopoDS_Shape& incident : vertex_edges.FindFromIndex(v)) {
        const int index = edge_faces.FindIndex(incident);
        if (index == 0) {
          continue;
        }
        if (hidden[index]) {
          passes_hidden = true;
        } else {
          shown.push_back(index);
        }
      }
      if (!passes_hidden || shown.size() != 2) {
        continue;
      }
      const TopoDS_Edge a = TopoDS::Edge(edge_faces.FindKey(shown[0]));
      const TopoDS_Edge b = TopoDS::Edge(edge_faces.FindKey(shown[1]));
      // An edge that starts and ends here is closed on its own.
      const TopoDS_Vertex at = TopoDS::Vertex(vertex_edges.FindKey(v));
      const auto ends_twice = [&](const TopoDS_Edge& edge) {
        TopoDS_Vertex from, to;
        TopExp::Vertices(edge, from, to);
        return from.IsSame(at) && to.IsSame(at);
      };
      if (ends_twice(a) || ends_twice(b) || !parcad_same_curve(a, b)) {
        continue;
      }
      group[root(shown[0])] = root(shown[1]);
    }

    std::map<int, std::vector<TopoDS_Shape>> members;
    for (int i = 1; i <= count; ++i) {
      if (!hidden[i]) {
        members[root(i)].push_back(edge_faces.FindKey(i));
      }
    }
    auto out = std::unique_ptr<std::vector<TopoDS_Shape>>(new std::vector<TopoDS_Shape>());
    for (const auto& [_, edges] : members) {
      out->push_back(parcad_compound(edges));
    }
    // An edge no face bounds, a loose wire's, is an edge of the shape by itself.
    ParcadShapeMap loose;
    for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
      if (!edge_faces.Contains(e.Current()) && loose.Add(e.Current())) {
        out->push_back(parcad_compound({e.Current()}));
      }
    }
    return out;
  });
}

inline TopoDS_Shape parcad_reversed_shape(const TopoDS_Shape& shape) { return shape.Reversed(); }

// Each input face's images in `result`: what `modified` and `generated`
// report for it, and itself if it survived untouched, appended to `history`
// as (input, output) pairs; and the faces generated from its free edges, as
// (-1 - input, output).
inline void parcad_record_history(const TopoDS_Shape& input, const TopoDS_Shape& result,
                                  const std::function<NCollection_List<TopoDS_Shape>(const TopoDS_Shape&)>& images,
                                  rust::Vec<int>& history) {
  ParcadShapeMap in_faces;
  ParcadShapeMap out_faces;
  TopExp::MapShapes(input, TopAbs_FACE, in_faces);
  TopExp::MapShapes(result, TopAbs_FACE, out_faces);
  auto push = [&](int from, const TopoDS_Shape& made, bool lateral) {
    TopExp_Explorer faces(made, TopAbs_FACE);
    for (; faces.More(); faces.Next()) {
      const int to = out_faces.FindIndex(faces.Current());
      if (to > 0) {
        history.push_back(lateral ? -from : from - 1);
        history.push_back(to - 1);
      }
    }
  };
  for (int i = 1; i <= in_faces.Extent(); ++i) {
    const TopoDS_Shape& face = in_faces.FindKey(i);
    if (out_faces.FindIndex(face) > 0) {
      push(i, face, false);
    }
    for (const TopoDS_Shape& made : images(face)) {
      push(i, made, false);
    }
  }
  ParcadAncestors edge_faces;
  TopExp::MapShapesAndUniqueAncestors(input, TopAbs_EDGE, TopAbs_FACE, edge_faces);
  for (int i = 1; i <= edge_faces.Extent(); ++i) {
    const NCollection_List<TopoDS_Shape>& around = edge_faces.FindFromIndex(i);
    if (around.Extent() != 1) {
      continue;
    }
    const int from = in_faces.FindIndex(around.First());
    for (const TopoDS_Shape& made : images(edge_faces.FindKey(i))) {
      if (made.ShapeType() == TopAbs_FACE) {
        push(from, made, true);
      }
    }
  }
}

// An open or closed wire swept along a straight vector: one face per edge.
inline std::unique_ptr<TopoDS_Shape> parcad_prism(const TopoDS_Shape& wire, double dx, double dy, double dz) {
  return parcad_surfacing_guard("extruding the curve", [&]() {
    BRepPrimAPI_MakePrism make(wire, gp_Vec(dx, dy, dz), true);
    if (!make.IsDone()) {
      throw std::runtime_error("the kernel could not extrude the curve into a surface");
    }
    return parcad_boxed(make.Shape());
  });
}

// A wire revolved about +Z through the origin.
inline std::unique_ptr<TopoDS_Shape> parcad_revolve(const TopoDS_Shape& wire, double degrees) {
  return parcad_surfacing_guard("revolving the curve", [&]() {
    const gp_Ax1 axis(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0));
    const bool full = std::abs(degrees - 360.0) < 1e-12;
    std::unique_ptr<BRepPrimAPI_MakeRevol> make(
        full ? new BRepPrimAPI_MakeRevol(wire, axis, true)
             : new BRepPrimAPI_MakeRevol(wire, axis, degrees * M_PI / 180.0, true));
    if (!make->IsDone()) {
      throw std::runtime_error("the kernel could not revolve the curve into a surface");
    }
    return parcad_boxed(make->Shape());
  });
}

// A shell through the compound's wires, in order, pairing them as given.
inline std::unique_ptr<TopoDS_Shape> parcad_thru_sections(const TopoDS_Shape& wires, bool ruled) {
  return parcad_surfacing_guard("lofting the curves", [&]() {
    BRepOffsetAPI_ThruSections make(false, ruled, 1e-6);
    make.CheckCompatibility(false);
    for (const TopoDS_Shape& wire : parcad_children(wires)) {
      make.AddWire(TopoDS::Wire(wire));
    }
    make.Build();
    if (!make.IsDone()) {
      throw std::runtime_error("the kernel could not loft the curves into a surface");
    }
    return parcad_boxed(make.Shape());
  });
}

// An open or closed profile wire swept along a spine into a shell.
inline std::unique_ptr<TopoDS_Shape> parcad_pipe_surface(const TopoDS_Wire& spine, const TopoDS_Wire& profile,
                                                         int mode) {
  return parcad_surfacing_guard("sweeping the curve", [&]() {
    BRepOffsetAPI_MakePipeShell make(spine);
    switch (mode) {
      case 1:
        make.SetMode(true);
        break;
      case 2:
        make.SetMode(gp_Dir(0.0, 0.0, 1.0));
        break;
      default:
        make.SetMode(false);
        break;
    }
    make.Add(profile, false, false);
    make.Build();
    if (!make.IsDone()) {
      throw std::runtime_error("the kernel could not sweep the curve into a surface");
    }
    return parcad_boxed(make.Shape());
  });
}

inline NCollection_Array1<double> parcad_reals(rust::Slice<const double> values) {
  NCollection_Array1<double> out(1, static_cast<int>(values.size()));
  for (size_t i = 0; i < values.size(); ++i) {
    out.SetValue(static_cast<int>(i) + 1, values[i]);
  }
  return out;
}

inline NCollection_Array1<int> parcad_ints(rust::Slice<const int> values) {
  NCollection_Array1<int> out(1, static_cast<int>(values.size()));
  for (size_t i = 0; i < values.size(); ++i) {
    out.SetValue(static_cast<int>(i) + 1, values[i]);
  }
  return out;
}

// A B-spline surface given as a pole grid, cut into one face per stretch
// between consecutive `v_breaks` and `u_pieces` stretches of u, and sewn,
// open at every other boundary. `bounds` gets the whole surface's exact box.
inline std::unique_ptr<TopoDS_Shape> parcad_bspline_bands(int nu, int nv, rust::Slice<const double> poles,
                                                          rust::Slice<const double> uknots,
                                                          rust::Slice<const int> umults, int udeg,
                                                          rust::Slice<const double> vknots,
                                                          rust::Slice<const int> vmults, int vdeg,
                                                          rust::Slice<const double> v_breaks,
                                                          int u_pieces, rust::Vec<double>& bounds) {
  return parcad_surfacing_guard("building the lofted surface", [&]() {
    if (nu < 2 || nv < 2 || poles.size() != static_cast<size_t>(nu) * static_cast<size_t>(nv) * 3 ||
        v_breaks.size() < 2) {
      throw std::runtime_error("a lofted surface needs nu x nv poles and two breaks");
    }
    NCollection_Array2<gp_Pnt> grid(1, nu, 1, nv);
    for (int i = 0; i < nu; ++i) {
      for (int j = 0; j < nv; ++j) {
        const size_t at = (static_cast<size_t>(i) * nv + j) * 3;
        grid.SetValue(i + 1, j + 1, gp_Pnt(poles[at], poles[at + 1], poles[at + 2]));
      }
    }
    occ::handle<Geom_BSplineSurface> surface = new Geom_BSplineSurface(
        grid, parcad_reals(uknots), parcad_reals(vknots), parcad_ints(umults), parcad_ints(vmults), udeg, vdeg,
        false, false);
    double u0 = 0.0;
    double u1 = 0.0;
    double vmin = 0.0;
    double vmax = 0.0;
    surface->Bounds(u0, u1, vmin, vmax);
    {
      BRepBuilderAPI_MakeFace whole(surface, u0, u1, v_breaks[0], v_breaks[v_breaks.size() - 1], 1e-7);
      Bnd_Box box;
      BRepBndLib::AddOptimal(whole.Face(), box, false, false);
      double x0, y0, z0, x1, y1, z1;
      box.Get(x0, y0, z0, x1, y1, z1);
      for (double x : {x0, y0, z0, x1, y1, z1}) {
        bounds.push_back(x);
      }
    }
    BRepBuilderAPI_Sewing sewing(1e-6);
    const int pieces = std::max(1, u_pieces);
    for (size_t k = 0; k + 1 < v_breaks.size(); ++k) {
      for (int p = 0; p < pieces; ++p) {
        const double a = u0 + (u1 - u0) * p / pieces;
        const double b = p + 1 == pieces ? u1 : u0 + (u1 - u0) * (p + 1) / pieces;
        // Each piece its own segment of the surface: an operation that
        // measures a face's surface — an offset, a same-domain check —
        // otherwise works over the whole surface for every piece.
        occ::handle<Geom_BSplineSurface> piece = occ::handle<Geom_BSplineSurface>::DownCast(surface->Copy());
        piece->Segment(a, b, v_breaks[k], v_breaks[k + 1]);
        BRepBuilderAPI_MakeFace make(piece, 1e-7);
        if (!make.IsDone()) {
          throw std::runtime_error("a band of the lofted surface could not be made into a face");
        }
        sewing.Add(make.Face());
      }
    }
    sewing.Perform();
    return parcad_boxed(sewing.SewedShape());
  });
}

// Every shape in the compound sewn at `tolerance`. `stats` gets
// [free edges, edges met by more than two faces].
inline std::unique_ptr<TopoDS_Shape> parcad_sew(const TopoDS_Shape& shapes, double tolerance,
                                                rust::Vec<int>& history, rust::Vec<double>& stats) {
  return parcad_surfacing_guard("stitching the surfaces", [&]() {
    BRepBuilderAPI_Sewing sewing(tolerance);
    for (const TopoDS_Shape& shape : parcad_children(shapes)) {
      sewing.Add(shape);
    }
    sewing.Perform();
    const TopoDS_Shape sewn = sewing.SewedShape();
    if (sewn.IsNull()) {
      throw std::runtime_error("stitching returned nothing");
    }
    parcad_record_history(
        shapes, sewn,
        [&](const TopoDS_Shape& s) {
          NCollection_List<TopoDS_Shape> out;
          if (sewing.IsModified(s)) {
            out.Append(sewing.Modified(s));
          } else if (sewing.IsModifiedSubShape(s)) {
            out.Append(sewing.ModifiedSubShape(s));
          }
          return out;
        },
        history);
    stats.push_back(double(sewing.NbFreeEdges()));
    stats.push_back(double(sewing.NbMultipleEdges()));
    return parcad_boxed(sewn);
  });
}

inline std::unique_ptr<TopoDS_Shape> parcad_plane_face(double px, double py, double pz, double nx, double ny,
                                                       double nz, double half) {
  return parcad_surfacing_guard("making the cutting plane", [&]() {
    const gp_Pln plane(gp_Pnt(px, py, pz), gp_Dir(nx, ny, nz));
    BRepBuilderAPI_MakeFace make(plane, -half, half, -half, half);
    if (!make.IsDone()) {
      throw std::runtime_error("the cutting plane could not be made into a face");
    }
    return parcad_boxed(make.Face());
  });
}

// `object` cut along everything `tool` touches; nothing is removed.
inline std::unique_ptr<TopoDS_Shape> parcad_split(const TopoDS_Shape& object, const TopoDS_Shape& tool,
                                                  rust::Vec<int>& history) {
  return parcad_surfacing_guard("splitting the surface", [&]() {
    BRepAlgoAPI_Splitter split;
    NCollection_List<TopoDS_Shape> arguments;
    arguments.Append(object);
    NCollection_List<TopoDS_Shape> tools;
    tools.Append(tool);
    split.SetArguments(arguments);
    split.SetTools(tools);
    split.Build();
    if (split.HasErrors() || !split.IsDone()) {
      throw std::runtime_error("the kernel could not split the surface by the tool");
    }
    const TopoDS_Shape result = split.Shape();
    parcad_record_history(
        object, result,
        [&](const TopoDS_Shape& s) {
          NCollection_List<TopoDS_Shape> out;
          for (const TopoDS_Shape& m : split.Modified(s)) {
            out.Append(m);
          }
          return out;
        },
        history);
    return parcad_boxed(result);
  });
}

// For each face, in `MapShapes` order: a point inside it and the face's
// outward normal there, as [px, py, pz, nx, ny, nz]; NaN when no grid up to
// 31 x 31 found an inside point.
inline rust::Vec<double> parcad_face_inside_points(const TopoDS_Shape& shape) {
  return parcad_surfacing_guard("finding a point on each face", [&]() {
    ParcadShapeMap faces;
    TopExp::MapShapes(shape, TopAbs_FACE, faces);
    rust::Vec<double> out;
    for (int i = 1; i <= faces.Extent(); ++i) {
      const TopoDS_Face face = TopoDS::Face(faces.FindKey(i));
      double u0, u1, v0, v1;
      BRepTools::UVBounds(face, u0, u1, v0, v1);
      BRepAdaptor_Surface surface(face);
      bool found = false;
      for (int per = 1; per <= 31 && !found; per = per * 2 + 1) {
        for (int a = 0; a < per && !found; ++a) {
          for (int b = 0; b < per && !found; ++b) {
            const double u = u0 + (u1 - u0) * (a + 0.5) / per;
            const double v = v0 + (v1 - v0) * (b + 0.5) / per;
            BRepClass_FaceClassifier classify(face, gp_Pnt2d(u, v), 1e-9);
            if (classify.State() != TopAbs_IN) {
              continue;
            }
            BRepLProp_SLProps props(surface, u, v, 1, 1e-9);
            if (!props.IsNormalDefined()) {
              continue;
            }
            gp_Dir n = props.Normal();
            if (face.Orientation() == TopAbs_REVERSED) {
              n.Reverse();
            }
            const gp_Pnt p = props.Value();
            for (double x : {p.X(), p.Y(), p.Z(), n.X(), n.Y(), n.Z()}) {
              out.push_back(x);
            }
            found = true;
          }
        }
      }
      if (!found) {
        for (int k = 0; k < 6; ++k) {
          out.push_back(std::numeric_limits<double>::quiet_NaN());
        }
      }
    }
    return out;
  });
}

// The two points of every face that bend it most toward its outward normal
// and most away from it, in `parcad_face_samples`' layout: a grid of `per`
// points in every continuous stretch of the surface each way (its C2
// intervals, so a knot span of a B-spline is never skipped), kept inside the
// face, then each extreme climbed by a compass search in the face's
// parameters down to 1e-9 of their range. A narrow pleat tip lies between
// the points of any grid laid over the face as a whole.
inline rust::Vec<double> parcad_bend_extremes(const TopoDS_Shape& shape, int per) {
  return parcad_surfacing_guard("finding where the surface bends most", [&]() {
    ParcadShapeMap faces;
    TopExp::MapShapes(shape, TopAbs_FACE, faces);
    rust::Vec<double> out;
    for (int i = 1; i <= faces.Extent(); ++i) {
      const TopoDS_Face face = TopoDS::Face(faces.FindKey(i));
      double u0, u1, v0, v1;
      BRepTools::UVBounds(face, u0, u1, v0, v1);
      BRepAdaptor_Surface surface(face);
      BRepTopAdaptor_FClass2d inside(face, 1e-9);
      const double sign = face.Orientation() == TopAbs_REVERSED ? -1.0 : 1.0;
      auto stations = [&](bool along_u, double lo, double hi) {
        const int count = along_u ? surface.NbUIntervals(GeomAbs_C2) : surface.NbVIntervals(GeomAbs_C2);
        NCollection_Array1<double> knots(1, count + 1);
        if (along_u) {
          surface.UIntervals(knots, GeomAbs_C2);
        } else {
          surface.VIntervals(knots, GeomAbs_C2);
        }
        std::vector<double> at;
        for (int k = 1; k <= count; ++k) {
          const double a = std::max(lo, knots(k));
          const double b = std::min(hi, knots(k + 1));
          for (int j = 0; b > a && j < per; ++j) {
            at.push_back(a + (b - a) * (j + 0.5) / per);
          }
        }
        return at;
      };
      // Curvature toward the outward normal, most and least, with the point.
      struct Bend {
        bool ok = false;
        gp_Pnt p;
        gp_Dir n;
        double most = 0.0;
        double least = 0.0;
      };
      auto bend = [&](double u, double v) {
        Bend b;
        if (u < u0 || u > u1 || v < v0 || v > v1 || inside.Perform(gp_Pnt2d(u, v)) != TopAbs_IN) {
          return b;
        }
        BRepLProp_SLProps props(surface, u, v, 2, 1e-9);
        if (!props.IsNormalDefined() || !props.IsCurvatureDefined()) {
          return b;
        }
        b.ok = true;
        b.p = props.Value();
        b.n = props.Normal();
        if (sign < 0.0) {
          b.n.Reverse();
        }
        const double k1 = sign * props.MaxCurvature();
        const double k2 = sign * props.MinCurvature();
        b.most = std::max(k1, k2);
        b.least = std::min(k1, k2);
        return b;
      };
      const std::vector<double> us = stations(true, u0, u1);
      const std::vector<double> vs = stations(false, v0, v1);
      for (int side = 0; side < 2; ++side) {
        // side 0 climbs `most`, side 1 descends `least`.
        auto score = [&](const Bend& b) { return side == 0 ? b.most : -b.least; };
        double best = -1e300;
        double bu = 0.0;
        double bv = 0.0;
        for (double u : us) {
          for (double v : vs) {
            const Bend b = bend(u, v);
            if (b.ok && score(b) > best) {
              best = score(b);
              bu = u;
              bv = v;
            }
          }
        }
        if (best <= -1e300) {
          continue;
        }
        double su = (u1 - u0) / std::max<size_t>(us.size(), 1);
        double sv = (v1 - v0) / std::max<size_t>(vs.size(), 1);
        const double floor_u = 1e-9 * (u1 - u0);
        const double floor_v = 1e-9 * (v1 - v0);
        while (su > floor_u || sv > floor_v) {
          bool moved = false;
          for (const auto& d : {std::make_pair(su, 0.0), std::make_pair(-su, 0.0), std::make_pair(0.0, sv), std::make_pair(0.0, -sv)}) {
            const Bend b = bend(bu + d.first, bv + d.second);
            if (b.ok && score(b) > best) {
              best = score(b);
              bu += d.first;
              bv += d.second;
              moved = true;
              break;
            }
          }
          if (!moved) {
            su /= 2.0;
            sv /= 2.0;
          }
        }
        const Bend b = bend(bu, bv);
        for (double x : {double(i - 1), b.p.X(), b.p.Y(), b.p.Z(), b.n.X(), b.n.Y(), b.n.Z(), b.most, b.least}) {
          out.push_back(x);
        }
      }
    }
    return out;
  });
}

// A grid of `per` by `per` points inside every face, as [face, px, py, pz,
// nx, ny, nz, most curvature toward n, most curvature away from n], the
// normal the face's outward one and the curvatures signed so that positive
// bends the surface toward it: an offset of d along n folds where d times
// the first reaches 1, and of -d where d times minus the second does.
inline rust::Vec<double> parcad_face_samples(const TopoDS_Shape& shape, int per) {
  return parcad_surfacing_guard("sampling the surface", [&]() {
    ParcadShapeMap faces;
    TopExp::MapShapes(shape, TopAbs_FACE, faces);
    rust::Vec<double> out;
    for (int i = 1; i <= faces.Extent(); ++i) {
      const TopoDS_Face face = TopoDS::Face(faces.FindKey(i));
      double u0, u1, v0, v1;
      BRepTools::UVBounds(face, u0, u1, v0, v1);
      BRepAdaptor_Surface surface(face);
      const double sign = face.Orientation() == TopAbs_REVERSED ? -1.0 : 1.0;
      for (int a = 0; a < per; ++a) {
        for (int b = 0; b < per; ++b) {
          const double u = u0 + (u1 - u0) * (a + 0.5) / per;
          const double v = v0 + (v1 - v0) * (b + 0.5) / per;
          BRepClass_FaceClassifier classify(face, gp_Pnt2d(u, v), 1e-9);
          if (classify.State() != TopAbs_IN) {
            continue;
          }
          BRepLProp_SLProps props(surface, u, v, 2, 1e-9);
          if (!props.IsNormalDefined()) {
            continue;
          }
          gp_Dir n = props.Normal();
          if (sign < 0.0) {
            n.Reverse();
          }
          double kmax = 0.0;
          double kmin = 0.0;
          if (props.IsCurvatureDefined()) {
            const double k1 = sign * props.MaxCurvature();
            const double k2 = sign * props.MinCurvature();
            kmax = std::max(k1, k2);
            kmin = std::min(k1, k2);
          }
          const gp_Pnt p = props.Value();
          for (double x : {double(i - 1), p.X(), p.Y(), p.Z(), n.X(), n.Y(), n.Z(), kmax, kmin}) {
            out.push_back(x);
          }
        }
      }
    }
    return out;
  });
}

// The nearest point of `shape` to (x, y, z), as [distance, foot x, y, z,
// normal x, y, z, 1 when the foot is inside a face and 0 on an edge or
// vertex]. The normal is the face's outward one; zero off a face.
inline rust::Vec<double> parcad_nearest_on(const TopoDS_Shape& shape, double x, double y, double z) {
  return parcad_surfacing_guard("finding the nearest point", [&]() {
    const TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(gp_Pnt(x, y, z));
    BRepExtrema_DistShapeShape distance(vertex, shape);
    if (!distance.IsDone() || distance.NbSolution() < 1) {
      throw std::runtime_error("the kernel could not measure the distance to the tool");
    }
    const gp_Pnt foot = distance.PointOnShape2(1);
    gp_Dir n(0.0, 0.0, 1.0);
    double on_face = 0.0;
    double nx = 0.0;
    double ny = 0.0;
    double nz = 0.0;
    if (distance.SupportTypeShape2(1) == BRepExtrema_IsInFace) {
      const TopoDS_Face face = TopoDS::Face(distance.SupportOnShape2(1));
      double u = 0.0;
      double v = 0.0;
      distance.ParOnFaceS2(1, u, v);
      BRepAdaptor_Surface surface(face);
      BRepLProp_SLProps props(surface, u, v, 1, 1e-9);
      if (props.IsNormalDefined()) {
        n = props.Normal();
        if (face.Orientation() == TopAbs_REVERSED) {
          n.Reverse();
        }
        nx = n.X();
        ny = n.Y();
        nz = n.Z();
        on_face = 1.0;
      }
    }
    rust::Vec<double> out;
    for (double value : {distance.Value(), foot.X(), foot.Y(), foot.Z(), nx, ny, nz, on_face}) {
      out.push_back(value);
    }
    return out;
  });
}

// The faces filling each closed loop the compound's edges make on `shape`:
// a plane where the loop is flat, otherwise a filling surface through the
// edges, tangent to the faces they border when `tangent`. `stats` gets
// [planar faces, filled faces, worst distance from a filled face's boundary
// to its edges, worst angle to the bordering faces in radians, open chains].
inline std::unique_ptr<TopoDS_Shape> parcad_fill(const TopoDS_Shape& shape, const TopoDS_Shape& edges,
                                                 bool tangent, rust::Vec<double>& stats) {
  return parcad_surfacing_guard("patching the boundary", [&]() {
    ParcadAncestors edge_faces;
    TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_EDGE, TopAbs_FACE, edge_faces);
    auto sequence = new NCollection_HSequence<TopoDS_Shape>();
    for (const TopoDS_Shape& edge : parcad_children(edges)) {
      sequence->Append(edge);
    }
    occ::handle<NCollection_HSequence<TopoDS_Shape>> held(sequence);
    occ::handle<NCollection_HSequence<TopoDS_Shape>> wires =
        ShapeAnalysis_FreeBounds::ConnectEdgesToWires(held, 1e-7, true);
    std::vector<TopoDS_Shape> made;
    double planar = 0.0;
    double filled = 0.0;
    double g0 = 0.0;
    double g1 = 0.0;
    double open = 0.0;
    for (int w = 1; w <= wires->Length(); ++w) {
      const TopoDS_Wire wire = TopoDS::Wire(wires->Value(w));
      if (!BRep_Tool::IsClosed(wire)) {
        open += 1.0;
        continue;
      }
      BRepLib_FindSurface find(wire, 1e-7, true);
      if (find.Found()) {
        occ::handle<Geom_Plane> plane = occ::handle<Geom_Plane>::DownCast(find.Surface());
        BRepBuilderAPI_MakeFace make(plane->Pln(), wire, true);
        if (make.IsDone()) {
          made.push_back(make.Face());
          planar += 1.0;
          continue;
        }
      }
      BRepOffsetAPI_MakeFilling fill;
      for (TopExp_Explorer it(wire, TopAbs_EDGE); it.More(); it.Next()) {
        const TopoDS_Edge edge = TopoDS::Edge(it.Current());
        const int index = edge_faces.FindIndex(edge);
        if (tangent && index > 0 && edge_faces.FindFromIndex(index).Extent() == 1) {
          fill.Add(edge, TopoDS::Face(edge_faces.FindFromIndex(index).First()), GeomAbs_G1, true);
        } else {
          fill.Add(edge, GeomAbs_C0, true);
        }
      }
      fill.Build();
      if (!fill.IsDone()) {
        throw std::runtime_error("the kernel could not fill a loop of the boundary");
      }
      g0 = std::max(g0, fill.G0Error());
      if (tangent) {
        g1 = std::max(g1, fill.G1Error());
      }
      made.push_back(fill.Shape());
      filled += 1.0;
    }
    for (double x : {planar, filled, g0, g1, open}) {
      stats.push_back(x);
    }
    return parcad_boxed(parcad_compound(made));
  });
}

// Each shell of `shape` offset by `offset` along its faces' normals,
// thickened into a solid when `thicken`. More than one shell is fused.
inline std::unique_ptr<TopoDS_Shape> parcad_offset(const TopoDS_Shape& shape, double offset, bool thicken,
                                                   rust::Vec<int>& history) {
  return parcad_surfacing_guard(thicken ? "thickening the surface" : "offsetting the surface", [&]() {
    std::vector<TopoDS_Shape> pieces;
    std::vector<TopoDS_Shape> shells;
    for (TopExp_Explorer it(shape, TopAbs_SHELL); it.More(); it.Next()) {
      shells.push_back(it.Current());
    }
    ParcadAncestors face_shells;
    TopExp::MapShapesAndAncestors(shape, TopAbs_FACE, TopAbs_SHELL, face_shells);
    for (int i = 1; i <= face_shells.Extent(); ++i) {
      if (face_shells.FindFromIndex(i).IsEmpty()) {
        shells.push_back(face_shells.FindKey(i));
      }
    }
    struct Step {
      std::unique_ptr<BRepOffset_MakeOffset> maker;
      TopoDS_Shape input;
    };
    std::vector<Step> steps;
    for (const TopoDS_Shape& shell : shells) {
      std::unique_ptr<BRepOffset_MakeOffset> maker(new BRepOffset_MakeOffset());
      maker->Initialize(shell, offset, 1e-7, BRepOffset_Skin, false, false, GeomAbs_Arc, thicken, false);
      if (thicken) {
        maker->MakeThickSolid();
      } else {
        maker->MakeOffsetShape();
      }
      if (!maker->IsDone() || maker->Shape().IsNull()) {
        throw std::runtime_error(std::string("the kernel could not ") +
                                 (thicken ? "thicken" : "offset") + " the surface (error " +
                                 std::to_string(int(maker->Error())) + ")");
      }
      pieces.push_back(maker->Shape());
      steps.push_back(Step{std::move(maker), shell});
    }
    if (pieces.empty()) {
      throw std::runtime_error("there is no surface to offset");
    }
    TopoDS_Shape result;
    if (pieces.size() == 1) {
      result = pieces[0];
    } else if (thicken) {
      BRepAlgoAPI_Fuse fuse;
      NCollection_List<TopoDS_Shape> first;
      first.Append(pieces[0]);
      NCollection_List<TopoDS_Shape> rest;
      for (size_t k = 1; k < pieces.size(); ++k) {
        rest.Append(pieces[k]);
      }
      fuse.SetArguments(first);
      fuse.SetTools(rest);
      fuse.Build();
      if (fuse.HasErrors() || !fuse.IsDone()) {
        throw std::runtime_error("the thickened pieces could not be joined");
      }
      result = fuse.Shape();
    } else {
      result = parcad_compound(pieces);
    }
    // History through each step, then through the fuse when there was one:
    // an image that the fuse replaced is looked up again in its result.
    ParcadShapeMap result_faces;
    TopExp::MapShapes(result, TopAbs_FACE, result_faces);
    ParcadShapeMap all_in;
    TopExp::MapShapes(shape, TopAbs_FACE, all_in);
    for (Step& step : steps) {
      rust::Vec<int> local;
      parcad_record_history(
          step.input, step.maker->Shape(),
          [&](const TopoDS_Shape& s) {
            NCollection_List<TopoDS_Shape> out;
            for (const TopoDS_Shape& m : step.maker->Modified(s)) {
              out.Append(m);
            }
            for (const TopoDS_Shape& g : step.maker->Generated(s)) {
              out.Append(g);
            }
            return out;
          },
          local);
      ParcadShapeMap in_faces;
      ParcadShapeMap out_faces;
      TopExp::MapShapes(step.input, TopAbs_FACE, in_faces);
      TopExp::MapShapes(step.maker->Shape(), TopAbs_FACE, out_faces);
      for (size_t k = 0; k + 1 < local.size(); k += 2) {
        const bool lateral = local[k] < 0;
        const int local_from = lateral ? -1 - local[k] : local[k];
        const TopoDS_Shape& from = in_faces.FindKey(local_from + 1);
        const TopoDS_Shape& to = out_faces.FindKey(local[k + 1] + 1);
        const int global_from = all_in.FindIndex(from) - 1;
        const int direct = result_faces.FindIndex(to);
        if (direct > 0) {
          history.push_back(lateral ? -1 - global_from : global_from);
          history.push_back(direct - 1);
        }
      }
    }
    return parcad_boxed(result);
  });
}
