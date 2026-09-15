#pragma once

// PARCAD: B-spline edges from explicit poles, and a loft that may close onto a
// point. A separate bridge so the additions merge as one new file. See
// PARCAD-CHANGES.md.

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <Standard_Failure.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pnt.hxx>

#include "rust/cxx.h"

#include <memory>
#include <stdexcept>
#include <string>

// A non-rational, non-periodic B-spline edge. `poles` is x, y, z flattened;
// `knots` and `mults` are the distinct knot values and their multiplicities.
inline std::unique_ptr<TopoDS_Edge> parcad_bspline_edge(rust::Slice<const double> poles,
                                                        rust::Slice<const double> knots,
                                                        rust::Slice<const int> mults,
                                                        int degree) {
  try {
    const int count = static_cast<int>(poles.size() / 3);
    if (count < 2 || poles.size() % 3 != 0 || knots.size() != mults.size() || knots.size() < 2) {
      throw std::runtime_error("a B-spline edge needs at least two poles and one knot per multiplicity");
    }
    NCollection_Array1<gp_Pnt> pole_array(1, count);
    for (int i = 0; i < count; ++i) {
      pole_array.SetValue(i + 1, gp_Pnt(poles[3 * i], poles[3 * i + 1], poles[3 * i + 2]));
    }
    const int distinct = static_cast<int>(knots.size());
    NCollection_Array1<double> knot_array(1, distinct);
    NCollection_Array1<int> mult_array(1, distinct);
    for (int i = 0; i < distinct; ++i) {
      knot_array.SetValue(i + 1, knots[i]);
      mult_array.SetValue(i + 1, mults[i]);
    }
    Handle(Geom_BSplineCurve) curve =
        new Geom_BSplineCurve(pole_array, knot_array, mult_array, degree, false);
    BRepBuilderAPI_MakeEdge make_edge(curve);
    if (!make_edge.IsDone()) {
      throw std::runtime_error("the B-spline curve could not be made into an edge");
    }
    return std::unique_ptr<TopoDS_Edge>(new TopoDS_Edge(make_edge.Edge()));
  } catch (const Standard_Failure& raised) {
    throw std::runtime_error(std::string("building a B-spline edge raised: ") + raised.what());
  }
}

// BRepOffsetAPI_ThruSections with the caller's pairing taken literally, as
// Solid::loft_sections does, and with a vertex allowed as the first or last
// section.
class ParcadLoft {
 public:
  ParcadLoft(bool ruled) : builder_(true, ruled) { builder_.CheckCompatibility(false); }

  void add_wire(const TopoDS_Wire& wire) { builder_.AddWire(wire); }

  void add_point(double x, double y, double z) {
    builder_.AddVertex(BRepBuilderAPI_MakeVertex(gp_Pnt(x, y, z)).Vertex());
  }

  std::unique_ptr<TopoDS_Shape> build() {
    try {
      builder_.Build();
      if (!builder_.IsDone()) {
        throw std::runtime_error("the loft builder reported it could not skin these sections");
      }
      return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape(builder_.Shape()));
    } catch (const Standard_Failure& raised) {
      throw std::runtime_error(std::string("the loft builder raised: ") + raised.what());
    }
  }

 private:
  BRepOffsetAPI_ThruSections builder_;
};

inline std::unique_ptr<ParcadLoft> parcad_loft(bool ruled) {
  return std::unique_ptr<ParcadLoft>(new ParcadLoft(ruled));
}
