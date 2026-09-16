#pragma once

// PARCAD: B-spline edges from explicit poles or fitted through points, a
// planar outline stepped inward, and a loft that may close onto a point. A
// separate bridge so the additions merge as one new file. See
// PARCAD-CHANGES.md.

#include <AppDef_BSplineCompute.hxx>
#include <AppDef_MultiLine.hxx>
#include <AppDef_MultiPointConstraint.hxx>
#include <AppParCurves_Constraint.hxx>
#include <AppParCurves_MultiBSpCurve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepOffsetAPI_MakeOffset.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Tool.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <GeomAbs_JoinType.hxx>
#include <GeomAbs_Shape.hxx>
#include <GeomConvert.hxx>
#include <GeomConvert_CompCurveToBSplineCurve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Curve.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Array1.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_Orientation.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>
#include <math_Vector.hxx>

#include "rust/cxx.h"

#include <algorithm>
#include <limits>
#include <memory>
#include <stdexcept>
#include <string>
#include <typeinfo>
#include <vector>

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

// How many samples along a curve find the span its nearest point is in: a
// few per knot span of a B-spline, and a floor for everything else.
inline int parcad_samples_along(const Handle(Geom_Curve)& curve) {
  int samples = 64;
  Handle(Geom_BSplineCurve) spline = Handle(Geom_BSplineCurve)::DownCast(curve);
  if (!spline.IsNull()) {
    samples = std::max(samples, 16 * spline->NbKnots());
  }
  return samples;
}

// The parameter of the point of `curve` on [first, last] nearest `p`, and
// the distance to it. GeomAPI_ProjectPointOnCurve alone is not trusted for
// this: on a B-spline of 130 poles it handed back an extremum a quarter of
// the way round the curve from the true nearest point, so the span is found
// by sampling first and the projection is asked only within it.
inline double parcad_nearest_on_curve(const gp_Pnt& p, const Handle(Geom_Curve)& curve,
                                      double first, double last, double& distance) {
  const int samples = parcad_samples_along(curve);
  int nearest = 0;
  distance = std::numeric_limits<double>::infinity();
  for (int s = 0; s <= samples; ++s) {
    const double d = p.Distance(curve->Value(first + (last - first) * s / samples));
    if (d < distance) {
      distance = d;
      nearest = s;
    }
  }
  const double step = (last - first) / samples;
  double at = first + step * nearest;
  const double lo = std::max(first, at - step);
  const double hi = std::min(last, at + step);
  GeomAPI_ProjectPointOnCurve project(p, curve, lo, hi);
  if (project.NbPoints() > 0 && project.LowerDistance() < distance) {
    distance = project.LowerDistance();
    at = project.LowerDistanceParameter();
  }
  return at;
}

// The distance from `p` to the nearest point of `curve` on [first, last].
inline double parcad_distance_to_curve(const gp_Pnt& p, const Handle(Geom_Curve)& curve,
                                       double first, double last) {
  double distance = 0.0;
  parcad_nearest_on_curve(p, curve, first, last, distance);
  return distance;
}

// The furthest any of `points` (x, y, z flattened) lies from `edge`'s curve.
inline double parcad_edge_deviation(const TopoDS_Edge& edge, rust::Slice<const double> points) {
  try {
    double first = 0.0;
    double last = 0.0;
    Handle(Geom_Curve) curve = BRep_Tool::Curve(edge, first, last);
    if (curve.IsNull()) {
      throw std::runtime_error("the edge has no 3D curve to measure against");
    }
    double worst = 0.0;
    for (size_t i = 0; i + 2 < points.size(); i += 3) {
      const gp_Pnt p(points[i], points[i + 1], points[i + 2]);
      worst = std::max(worst, parcad_distance_to_curve(p, curve, first, last));
    }
    return worst;
  } catch (const Standard_Failure& raised) {
    throw std::runtime_error(std::string("measuring an edge against points raised: ") + raised.what());
  }
}

inline double parcad_distance_to_wire(const gp_Pnt& p, const TopoDS_Wire& wire) {
  double best = std::numeric_limits<double>::infinity();
  for (TopExp_Explorer it(wire, TopAbs_EDGE); it.More(); it.Next()) {
    double first = 0.0;
    double last = 0.0;
    Handle(Geom_Curve) curve = BRep_Tool::Curve(TopoDS::Edge(it.Current()), first, last);
    if (curve.IsNull()) {
      continue;
    }
    best = std::min(best, parcad_distance_to_curve(p, curve, first, last));
  }
  return best;
}

// A B-spline fitted through sampled points, and how far it is from them,
// measured on the curve that becomes the edge.
//
// AppDef_BSplineCompute is what GeomAPI_PointsToBSpline wraps, driven the
// same way — chord-length parameters, least squares, no iteration — but with
// its end constraints in reach: an open fit passes through its first and last
// point exactly, which is what lets a wire close on the corners, and a closed
// fit is the loop back to its first point with the *same tangent* at both
// ends, the chord between the point's two neighbours. The seam is C1 by
// construction rather than pinned shut afterwards, and nothing is moved after
// the fit, so the deviation the fitter held is the deviation measured.
class ParcadFit {
 public:
  ParcadFit(rust::Slice<const double> points, double tolerance, bool closed) {
    try {
      const int n = static_cast<int>(points.size() / 3);
      if (points.size() % 3 != 0 || n < 3) {
        throw std::runtime_error("a fit needs at least 3 points");
      }
      std::vector<gp_Pnt> given;
      given.reserve(n + 1);
      for (int i = 0; i < n; ++i) {
        given.emplace_back(points[3 * i], points[3 * i + 1], points[3 * i + 2]);
      }
      std::vector<gp_Pnt> seq = given;
      if (closed) {
        seq.push_back(given.front());
      }
      // Chord-length parameters, normalised to [0, 1]: the algorithm's own
      // knots live on that range (GeomAPI_PointsToBSpline normalises the same
      // way), and parameters in millimetres leave it fitting nothing but the
      // first sliver of the curve, then falling back to interpolation.
      const int count = static_cast<int>(seq.size());
      math_Vector params(1, count);
      double u = 0.0;
      for (int i = 0; i < count; ++i) {
        if (i > 0) {
          const double step = seq[i - 1].Distance(seq[i]);
          if (step < 1e-9) {
            throw std::runtime_error("points " + std::to_string(i - 1) + " and " +
                                     std::to_string(i % n) + " are the same point; drop one of them");
          }
          u += step;
        }
        params(i + 1) = u;
      }
      const double length = u;
      for (int i = 1; i <= count; ++i) {
        params(i) /= length;
      }
      // The tangent at the seam: along the chord through its two neighbours,
      // at the speed a chord-length parameterisation over [0, 1] gives, which
      // is the whole length per unit of parameter.
      gp_Vec seam(given[n - 1], given[1]);
      if (seam.Magnitude() < 1e-9) {
        throw std::runtime_error("the points either side of the first fold back onto each other");
      }
      seam.Normalize();
      seam.Multiply(length);
      AppDef_MultiLine line(count);
      for (int i = 0; i < count; ++i) {
        NCollection_Array1<gp_Pnt> point(1, 1);
        point.SetValue(1, seq[i]);
        if (closed && (i == 0 || i == count - 1)) {
          NCollection_Array1<gp_Vec> tangent(1, 1);
          tangent.SetValue(1, seam);
          line.SetValue(i + 1, AppDef_MultiPointConstraint(point, tangent));
        } else {
          line.SetValue(i + 1, AppDef_MultiPointConstraint(point));
        }
      }
      // Degree 3 on uniform knots, their number doubled until the curve
      // measures within tolerance — measured here, point to curve, not by
      // the fitter's parametric criterion. Uniform knots are what keep a loft
      // through many fitted sections affordable: ThruSections unifies every
      // section's knots, and nested uniform vectors unify into the finest of
      // them, where the adaptive knots of GeomAPI_PointsToBSpline unify into
      // their union and a fifteen-section lamp ran past ten minutes.
      const int pinned = closed ? 2 : 0;
      const AppParCurves_Constraint ends = closed ? AppParCurves_TangencyPoint : AppParCurves_PassPoint;
      Handle(Geom_BSplineCurve) curve;
      for (int spans = 4;; spans *= 2) {
        // Cubic C2 on `spans` spans is `spans + 3` poles; the constrained
        // tangents count as two more against the points.
        if (spans + 3 + pinned > count) {
          throw std::runtime_error("holding " + std::to_string(tolerance) +
                                   " mm would need a pole per point, which is interpolation rather than a fit");
        }
        NCollection_Array1<double> knots(1, spans + 1);
        for (int i = 1; i <= spans + 1; ++i) {
          knots.SetValue(i, double(i - 1) / spans);
        }
        // The fitter's own tolerance is set out of reach so it keeps its one
        // least-squares answer; whether that answer holds is measured below.
        AppDef_BSplineCompute fit(params, 3, 3, 1e100, 1e100, 0, false, true);
        fit.SetKnots(knots);
        fit.SetContinuity(2);
        fit.SetConstraints(ends, ends);
        fit.Perform(line);
        if (!fit.IsAllApproximated()) {
          throw std::runtime_error("the least squares on " + std::to_string(spans) + " spans did not solve");
        }
        const AppParCurves_MultiBSpCurve& result = fit.Value();
        NCollection_Array1<gp_Pnt> poles(1, result.NbPoles());
        result.Curve(1, poles);
        curve = new Geom_BSplineCurve(poles, result.Knots(), result.Multiplicities(), result.Degree());
        const double first = curve->FirstParameter();
        const double last = curve->LastParameter();
        deviation_ = 0.0;
        for (const gp_Pnt& p : given) {
          deviation_ = std::max(deviation_, parcad_distance_to_curve(p, curve, first, last));
        }
        if (deviation_ <= tolerance) {
          break;
        }
      }

      const double first = curve->FirstParameter();
      const double last = curve->LastParameter();
      poles_ = curve->NbPoles();
      degree_ = curve->Degree();
      // Dense samples of the curve, several per span between two points,
      // for the caller to check the loops a fit can make between them.
      const int per_span = 8;
      const int spans = count - 1;
      samples_.reserve(static_cast<size_t>(spans * per_span + 1) * 3);
      for (int s = 0; s <= spans * per_span; ++s) {
        const gp_Pnt at = curve->Value(first + (last - first) * s / (spans * per_span));
        samples_.push_back(at.X());
        samples_.push_back(at.Y());
        samples_.push_back(at.Z());
      }
      curve_ = curve;
      BRepBuilderAPI_MakeEdge make_edge(curve);
      if (!make_edge.IsDone()) {
        throw std::runtime_error("the fitted curve could not be made into an edge");
      }
      edge_ = make_edge.Edge();
    } catch (const Standard_Failure& raised) {
      // An OCCT raise often carries no text; its type is the only clue.
      const char* what = raised.GetMessageString();
      std::string message = what ? what : "";
      if (message.empty()) {
        message = "the kernel raised ";
        message += typeid(raised).name();
      }
      throw std::runtime_error(message);
    }
  }

  std::unique_ptr<TopoDS_Edge> edge() const { return std::unique_ptr<TopoDS_Edge>(new TopoDS_Edge(edge_)); }
  double deviation() const { return deviation_; }
  int poles() const { return poles_; }
  int degree() const { return degree_; }
  // The fitted curve itself: its poles as x, y, z triples, and its full knot
  // vector, each knot repeated by its multiplicity. Empty when periodic.
  rust::Vec<double> curve_poles() const {
    rust::Vec<double> out;
    if (curve_.IsNull() || curve_->IsPeriodic()) {
      return out;
    }
    for (int i = 1; i <= curve_->NbPoles(); ++i) {
      const gp_Pnt& p = curve_->Pole(i);
      out.push_back(p.X());
      out.push_back(p.Y());
      out.push_back(p.Z());
    }
    return out;
  }
  rust::Vec<double> curve_knots() const {
    rust::Vec<double> out;
    if (curve_.IsNull() || curve_->IsPeriodic()) {
      return out;
    }
    for (int i = 1; i <= curve_->NbKnots(); ++i) {
      for (int k = 0; k < curve_->Multiplicity(i); ++k) {
        out.push_back(curve_->Knot(i));
      }
    }
    return out;
  }
  rust::Vec<double> samples() const {
    rust::Vec<double> out;
    out.reserve(samples_.size());
    for (double v : samples_) {
      out.push_back(v);
    }
    return out;
  }

 private:
  TopoDS_Edge edge_;
  Handle(Geom_BSplineCurve) curve_;
  double deviation_ = 0.0;
  int poles_ = 0;
  int degree_ = 0;
  std::vector<double> samples_;
};

inline std::unique_ptr<ParcadFit> parcad_fit(rust::Slice<const double> points, double tolerance,
                                             bool closed) {
  return std::unique_ptr<ParcadFit>(new ParcadFit(points, tolerance, closed));
}

// The edges of a wire in the order it runs through them, each oriented the
// way the wire traverses it.
inline std::vector<TopoDS_Edge> parcad_ordered_edges(const TopoDS_Wire& wire) {
  std::vector<TopoDS_Edge> out;
  for (BRepTools_WireExplorer it(wire); it.More(); it.Next()) {
    out.push_back(it.Current());
  }
  if (out.empty()) {
    throw std::runtime_error("the wire has no edges");
  }
  return out;
}

// Where an oriented edge starts, and which way it heads from there.
inline void parcad_edge_start(const TopoDS_Edge& edge, gp_Pnt& at, gp_Vec& heading) {
  double first = 0.0;
  double last = 0.0;
  Handle(Geom_Curve) curve = BRep_Tool::Curve(edge, first, last);
  if (curve.IsNull()) {
    throw std::runtime_error("an edge of the inset carries no 3D curve");
  }
  if (edge.Orientation() == TopAbs_REVERSED) {
    curve->D1(last, at, heading);
    heading.Reverse();
  } else {
    curve->D1(first, at, heading);
  }
}

// The inset's edges begun at the corner nearest where `outline` begins and
// run the way the outline runs. A loft pairs sections by their first vertex
// and direction, taken literally, so an offset that starts wherever the
// builder left it would pair a wall with a twist.
inline std::vector<TopoDS_Edge> parcad_aligned_edges(const TopoDS_Wire& inset,
                                                     const TopoDS_Wire& outline) {
  gp_Pnt origin;
  gp_Vec heading;
  parcad_edge_start(parcad_ordered_edges(outline).front(), origin, heading);
  std::vector<TopoDS_Edge> edges = parcad_ordered_edges(inset);
  gp_Pnt at;
  gp_Vec along;
  parcad_edge_start(edges.front(), at, along);
  if (along.Dot(heading) < 0.0) {
    std::reverse(edges.begin(), edges.end());
    for (TopoDS_Edge& edge : edges) {
      edge.Reverse();
    }
  }
  size_t nearest = 0;
  double best = std::numeric_limits<double>::infinity();
  for (size_t i = 0; i < edges.size(); ++i) {
    parcad_edge_start(edges[i], at, along);
    const double d = at.Distance(origin);
    if (d < best) {
      best = d;
      nearest = i;
    }
  }
  std::rotate(edges.begin(), edges.begin() + static_cast<std::ptrdiff_t>(nearest), edges.end());
  return edges;
}

// One closed edge with the geometry of a whole wire: every edge's curve as a
// B-spline, concatenated in wire order, then begun at the point nearest
// where `outline` begins. A B-spline may be C0 at a knot, so a corner the
// wire had is a corner the edge has.
inline TopoDS_Wire parcad_one_edge(const TopoDS_Wire& inset, const TopoDS_Wire& outline) {
  GeomConvert_CompCurveToBSplineCurve joined;
  for (const TopoDS_Edge& edge : parcad_aligned_edges(inset, outline)) {
    double first = 0.0;
    double last = 0.0;
    Handle(Geom_Curve) curve = BRep_Tool::Curve(edge, first, last);
    if (curve.IsNull()) {
      throw std::runtime_error("an edge of the inset carries no 3D curve");
    }
    Handle(Geom_BSplineCurve) piece =
        GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(curve, first, last));
    if (edge.Orientation() == TopAbs_REVERSED) {
      piece->Reverse();
    }
    if (!joined.Add(piece, 1e-6, true, true, 0)) {
      throw std::runtime_error("the inset's edges do not meet end to end");
    }
  }
  Handle(Geom_BSplineCurve) curve = joined.BSplineCurve();

  gp_Pnt origin;
  gp_Vec heading;
  parcad_edge_start(parcad_ordered_edges(outline).front(), origin, heading);
  const double first = curve->FirstParameter();
  const double last = curve->LastParameter();
  {
    double distance = 0.0;
    const double u = parcad_nearest_on_curve(origin, curve, first, last, distance);
    const double slack = 1e-6 * (last - first);
    if (u - first > slack && last - u > slack) {
      Handle(Geom_BSplineCurve) tail = Handle(Geom_BSplineCurve)::DownCast(curve->Copy());
      Handle(Geom_BSplineCurve) head = Handle(Geom_BSplineCurve)::DownCast(curve->Copy());
      tail->Segment(u, last);
      head->Segment(first, u);
      GeomConvert_CompCurveToBSplineCurve reset(tail);
      if (!reset.Add(head, 1e-6, true, true, 0)) {
        throw std::runtime_error("the inset curve could not be begun at the outline's start");
      }
      curve = reset.BSplineCurve();
    }
  }

  // The joined pieces meet within their join tolerance, not to the bit, and
  // an edge whose ends differ by a micron is an open edge to the loft that
  // skins it. Closing it on its own start pole is the guard's business to
  // measure afterwards.
  curve->SetPole(curve->NbPoles(), curve->Pole(1));
  BRepBuilderAPI_MakeEdge make_edge(curve);
  if (!make_edge.IsDone()) {
    throw std::runtime_error("the joined inset curve could not be made into an edge");
  }
  BRepBuilderAPI_MakeWire make_wire(make_edge.Edge());
  if (!make_wire.IsDone()) {
    throw std::runtime_error("the joined inset edge could not be made into a wire");
  }
  return make_wire.Wire();
}

// The inset's edges rewired to begin and run as the outline does.
inline TopoDS_Wire parcad_aligned_wire(const TopoDS_Wire& inset, const TopoDS_Wire& outline) {
  BRepBuilderAPI_MakeWire make_wire;
  for (const TopoDS_Edge& edge : parcad_aligned_edges(inset, outline)) {
    make_wire.Add(edge);
  }
  if (!make_wire.IsDone()) {
    throw std::runtime_error("the inset's edges could not be rewired in the outline's order");
  }
  return make_wire.Wire();
}

// The area a closed planar wire encloses, by Green's theorem over dense
// samples of its edges in wire order. Not BRepGProp::SurfaceProperties: on a
// planar face bounded by a wavy B-spline that integral read 8488 mm² where
// the curve's own samples enclose 8835 (docs/GOTCHAS.md, "The volume integral
// misreads a wavy B-spline wall"), and a guard built on it refused a good
// inset for growing.
inline double parcad_sampled_area(const TopoDS_Wire& wire) {
  double twice = 0.0;
  bool have_first = false;
  gp_Pnt first_point;
  gp_Pnt previous;
  for (BRepTools_WireExplorer it(wire); it.More(); it.Next()) {
    const TopoDS_Edge& edge = it.Current();
    double first = 0.0;
    double last = 0.0;
    Handle(Geom_Curve) curve = BRep_Tool::Curve(edge, first, last);
    if (curve.IsNull()) {
      continue;
    }
    const int samples = parcad_samples_along(curve);
    const bool reversed = edge.Orientation() == TopAbs_REVERSED;
    for (int s = 0; s <= samples; ++s) {
      const double along = reversed ? double(samples - s) / samples : double(s) / samples;
      const gp_Pnt at = curve->Value(first + (last - first) * along);
      if (!have_first) {
        first_point = at;
        have_first = true;
      } else {
        twice += previous.X() * at.Y() - at.X() * previous.Y();
      }
      previous = at;
    }
  }
  if (have_first) {
    twice += previous.X() * first_point.Y() - first_point.X() * previous.Y();
  }
  return std::abs(twice) / 2.0;
}

// A closed planar outline stepped inward by `distance` with
// BRepOffsetAPI_MakeOffset (intersection joins, so a corner stays a corner
// and the edge count of a polygon is kept), and measured: `slip` is the
// furthest any point of the inset is from lying exactly `distance` inside
// the outline, and the two areas say which way it went. Loops the offset
// removed leave corners; loops it failed to remove show up as slip.
class ParcadInset {
 public:
  ParcadInset(const TopoDS_Wire& outline, double distance) {
    try {
      int outline_edges = 0;
      for (TopExp_Explorer it(outline, TopAbs_EDGE); it.More(); it.Next()) {
        ++outline_edges;
      }
      // The offset builder returns nothing for a loop that is one closed
      // edge, at any distance; the same loop as two edges offsets. So a
      // one-edge outline is split at its middle parameter to be offset, and
      // joined back into one edge afterwards.
      TopoDS_Wire spine = outline;
      if (outline_edges == 1) {
        const TopoDS_Edge whole = TopoDS::Edge(TopExp_Explorer(outline, TopAbs_EDGE).Current());
        double first = 0.0;
        double last = 0.0;
        Handle(Geom_Curve) curve = BRep_Tool::Curve(whole, first, last);
        if (curve.IsNull()) {
          throw std::runtime_error("the outline's edge carries no 3D curve");
        }
        const double mid = (first + last) / 2.0;
        BRepBuilderAPI_MakeWire halves(BRepBuilderAPI_MakeEdge(curve, first, mid).Edge(),
                                       BRepBuilderAPI_MakeEdge(curve, mid, last).Edge());
        if (!halves.IsDone()) {
          throw std::runtime_error("the outline could not be split in two to be offset");
        }
        spine = halves.Wire();
      }
      BRepBuilderAPI_MakeFace make_face(spine, true);
      if (!make_face.IsDone()) {
        throw std::runtime_error("the outline is not one closed planar loop");
      }
      const TopoDS_Face face = make_face.Face();
      outline_area_ = parcad_sampled_area(spine);

      BRepOffsetAPI_MakeOffset offset(face, GeomAbs_Intersection, false);
      offset.Perform(-distance);
      if (!offset.IsDone()) {
        throw std::runtime_error("the offset builder could not step this outline inward by " +
                                 std::to_string(distance) + " mm");
      }
      std::vector<TopoDS_Wire> wires;
      for (TopExp_Explorer it(offset.Shape(), TopAbs_WIRE); it.More(); it.Next()) {
        wires.push_back(TopoDS::Wire(it.Current()));
      }
      if (wires.empty()) {
        throw std::runtime_error("nothing is left " + std::to_string(distance) + " mm inside this outline");
      }
      if (wires.size() > 1) {
        throw std::runtime_error("stepping this outline inward by " + std::to_string(distance) +
                                 " mm splits it into " + std::to_string(wires.size()) + " loops");
      }
      TopoDS_Wire inset = outline_edges == 1 ? parcad_one_edge(wires.front(), outline)
                                             : parcad_aligned_wire(wires.front(), outline);

      BRepBuilderAPI_MakeFace inset_face(inset, true);
      if (!inset_face.IsDone()) {
        throw std::runtime_error("the inset is not one closed planar loop");
      }
      area_ = parcad_sampled_area(inset);

      slip_ = 0.0;
      const int samples = 64;
      for (TopExp_Explorer it(inset, TopAbs_EDGE); it.More(); it.Next()) {
        double first = 0.0;
        double last = 0.0;
        Handle(Geom_Curve) curve = BRep_Tool::Curve(TopoDS::Edge(it.Current()), first, last);
        if (curve.IsNull()) {
          throw std::runtime_error("an edge of the inset carries no 3D curve");
        }
        for (int s = 0; s <= samples; ++s) {
          const gp_Pnt p = curve->Value(first + (last - first) * s / samples);
          slip_ = std::max(slip_, std::abs(parcad_distance_to_wire(p, outline) - distance));
        }
      }
      poles_ = 0;
      for (TopExp_Explorer it(inset, TopAbs_EDGE); it.More(); it.Next()) {
        double first = 0.0;
        double last = 0.0;
        Handle(Geom_BSplineCurve) spline =
            Handle(Geom_BSplineCurve)::DownCast(BRep_Tool::Curve(TopoDS::Edge(it.Current()), first, last));
        if (!spline.IsNull()) {
          poles_ += spline->NbPoles();
        }
      }
      wire_ = inset;
    } catch (const Standard_Failure& raised) {
      throw std::runtime_error(std::string("stepping the outline inward raised: ") + raised.what());
    }
  }

  std::unique_ptr<TopoDS_Wire> wire() const { return std::unique_ptr<TopoDS_Wire>(new TopoDS_Wire(wire_)); }
  double slip() const { return slip_; }
  int poles() const { return poles_; }
  double area() const { return area_; }
  double outline_area() const { return outline_area_; }

 private:
  TopoDS_Wire wire_;
  double slip_ = 0.0;
  double area_ = 0.0;
  double outline_area_ = 0.0;
  int poles_ = 0;
};

inline std::unique_ptr<ParcadInset> parcad_inset(const TopoDS_Wire& outline, double distance) {
  return std::unique_ptr<ParcadInset>(new ParcadInset(outline, distance));
}
