#pragma once

// PARCAD: helical spines and law-driven sweeps (BRepOffsetAPI_MakePipeShell).
// A separate bridge from history.hxx so the additions merge as one new file.
// See PARCAD-CHANGES.md.

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Tool.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_ConicalSurface.hxx>
#include <Geom_Curve.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Law_Linear.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax3.hxx>
#include <gp_Dir.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pnt2d.hxx>

#include "rust/cxx.h"

#include <cmath>
#include <memory>
#include <stdexcept>
#include <string>

// The analytic helix both functions below agree on: axis +Z, starting at
// (start_radius, 0, -height/2), rising pitch per turn, radius varying
// linearly with the turn angle to end_radius.
struct ParcadHelix {
  double theta;   // total turn angle, radians
  double height;  // pitch * turns
  double v_span;  // the parameter-space rise: height, or height / cos(semi-angle) on a cone
  double length;  // length of the parameter-space line
  double sign;    // +1 right-handed, -1 left-handed

  ParcadHelix(double start_radius, double end_radius, double pitch, double turns, bool left_handed) {
    theta = 2.0 * M_PI * turns;
    height = pitch * turns;
    sign = left_handed ? -1.0 : 1.0;
    v_span = height;
    if (std::abs(end_radius - start_radius) > 1e-12) {
      v_span = height / std::cos(std::atan((end_radius - start_radius) / height));
    }
    length = std::hypot(theta, v_span);
  }
};

// The exact helix is a straight line in the (u, v) parameter space of a
// cylinder — or of a cone, when the radius changes, which keeps both radius
// and height linear in the turn angle. The 3D curve the sweep needs is then
// approximated from that line by BRepLib::BuildCurve3d; how far it strays is
// measured by parcad_helix_deviation, not assumed.
inline std::unique_ptr<TopoDS_Wire> parcad_helix_spine(double start_radius, double end_radius,
                                                       double pitch, double turns,
                                                       bool left_handed) {
  try {
    ParcadHelix helix(start_radius, end_radius, pitch, turns, left_handed);
    gp_Ax3 frame(gp_Pnt(0.0, 0.0, -helix.height / 2.0), gp_Dir(0.0, 0.0, 1.0),
                 gp_Dir(1.0, 0.0, 0.0));
    Handle(Geom_Surface) surface;
    if (std::abs(end_radius - start_radius) > 1e-12) {
      double semi_angle = std::atan((end_radius - start_radius) / helix.height);
      surface = new Geom_ConicalSurface(frame, semi_angle, start_radius);
    } else {
      surface = new Geom_CylindricalSurface(frame, start_radius);
    }
    Handle(Geom2d_Line) line =
        new Geom2d_Line(gp_Pnt2d(0.0, 0.0), gp_Dir2d(helix.sign * helix.theta, helix.v_span));
    BRepBuilderAPI_MakeEdge make_edge(line, surface, 0.0, helix.length);
    if (!make_edge.IsDone()) {
      throw std::runtime_error("the helix edge could not be made on its surface");
    }
    TopoDS_Edge edge = make_edge.Edge();
    if (!BRepLib::BuildCurve3d(edge, 1.0e-7, GeomAbs_C2, 14, 1000)) {
      throw std::runtime_error("the helix's 3D curve could not be approximated");
    }
    BRepBuilderAPI_MakeWire make_wire(edge);
    return std::unique_ptr<TopoDS_Wire>(new TopoDS_Wire(make_wire.Wire()));
  } catch (const Standard_Failure& raised) {
    throw std::runtime_error(std::string("building the helix raised: ") +
                             raised.what());
  }
}

// The largest distance, over `samples` evenly spaced parameters, between the
// spine's approximated 3D curve and the analytic helix at the same parameter.
// Matching parameters make it an upper bound on the geometric distance.
inline double parcad_helix_deviation(const TopoDS_Wire& spine, double start_radius,
                                     double end_radius, double pitch, double turns,
                                     bool left_handed, int samples) {
  ParcadHelix helix(start_radius, end_radius, pitch, turns, left_handed);
  TopExp_Explorer explorer(spine, TopAbs_EDGE);
  if (!explorer.More()) {
    return INFINITY;
  }
  double first = 0.0, last = 0.0;
  Handle(Geom_Curve) curve = BRep_Tool::Curve(TopoDS::Edge(explorer.Current()), first, last);
  if (curve.IsNull()) {
    return INFINITY;
  }
  double worst = 0.0;
  for (int k = 0; k <= samples; ++k) {
    double s = double(k) / double(samples);
    gp_Pnt built = curve->Value(first + (last - first) * s);
    double angle = helix.sign * helix.theta * s;
    double radius = start_radius + (end_radius - start_radius) * s;
    gp_Pnt exact(radius * std::cos(angle), radius * std::sin(angle),
                 -helix.height / 2.0 + helix.height * s);
    worst = std::max(worst, built.Distance(exact));
  }
  return worst;
}

// A cylindrical helix about +Z from angle 0 at height z0, rising `pitch` per
// turn for a whole number of turns, as a wire of one edge per turn: each a
// Geom2d_Line segment on the cylinder with its own fitted 3D curve. Swept by
// an ISO thread profile, one edge fitted across every turn reads up to 3e-5
// from the volume's closed form and one per turn within 1e-5 on every coarse
// size (docs/GOTCHAS.md, "Threads"). FreeCAD's makeLongHelix construction.
inline std::unique_ptr<TopoDS_Wire> parcad_helix_spine_by_turn(double radius, double pitch,
                                                               int turns, bool left_handed,
                                                               double z0) {
  try {
    if (turns < 1) {
      throw std::runtime_error("a helix by turn needs at least one turn");
    }
    gp_Ax3 frame(gp_Pnt(0.0, 0.0, z0), gp_Dir(0.0, 0.0, 1.0), gp_Dir(1.0, 0.0, 0.0));
    Handle(Geom_Surface) surface = new Geom_CylindricalSurface(frame, radius);
    double sign = left_handed ? -1.0 : 1.0;
    Handle(Geom2d_Line) line =
        new Geom2d_Line(gp_Pnt2d(0.0, 0.0), gp_Dir2d(sign * 2.0 * M_PI, pitch));
    double per_turn = std::hypot(2.0 * M_PI, pitch);
    BRepBuilderAPI_MakeWire make_wire;
    for (int i = 0; i < turns; ++i) {
      BRepBuilderAPI_MakeEdge make_edge(line, surface, i * per_turn, (i + 1) * per_turn);
      if (!make_edge.IsDone()) {
        throw std::runtime_error("a helix turn could not be made on its cylinder");
      }
      TopoDS_Edge edge = make_edge.Edge();
      if (!BRepLib::BuildCurve3d(edge, 1.0e-7, GeomAbs_C2, 14, 1000)) {
        throw std::runtime_error("a helix turn's 3D curve could not be approximated");
      }
      make_wire.Add(edge);
      if (!make_wire.IsDone()) {
        throw std::runtime_error("consecutive helix turns did not join into one wire");
      }
    }
    return std::unique_ptr<TopoDS_Wire>(new TopoDS_Wire(make_wire.Wire()));
  } catch (const Standard_Failure& raised) {
    throw std::runtime_error(std::string("building the helix raised: ") + raised.what());
  }
}

// The largest distance between the by-turn spine's fitted 3D curves and the
// analytic helix, `samples` parameters per turn, each edge compared with the
// turn it stands for. An upper bound on the geometric distance, as above.
inline double parcad_helix_by_turn_deviation(const TopoDS_Wire& spine, double radius, double pitch,
                                             bool left_handed, double z0, int samples) {
  double sign = left_handed ? -1.0 : 1.0;
  double worst = 0.0;
  int turn = 0;
  for (BRepTools_WireExplorer explorer(spine); explorer.More(); explorer.Next(), ++turn) {
    double first = 0.0, last = 0.0;
    Handle(Geom_Curve) curve = BRep_Tool::Curve(explorer.Current(), first, last);
    if (curve.IsNull()) {
      return INFINITY;
    }
    bool reversed = explorer.Current().Orientation() == TopAbs_REVERSED;
    for (int k = 0; k <= samples; ++k) {
      double s = double(k) / double(samples);
      gp_Pnt built = curve->Value(reversed ? last - (last - first) * s : first + (last - first) * s);
      double t = turn + s;
      double angle = sign * 2.0 * M_PI * t;
      gp_Pnt exact(radius * std::cos(angle), radius * std::sin(angle), z0 + pitch * t);
      worst = std::max(worst, built.Distance(exact));
    }
  }
  return turn == 0 ? INFINITY : worst;
}

// Sweep a closed profile wire along a spine and close it into a solid.
// `mode`: 0 corrected Frenet (what MakePipe uses), 1 Frenet, 2 a fixed +Z
// binormal. `scale_end` != 1 scales the profile about the spine linearly from
// 1 at the start to `scale_end` at the end, by length along the spine.
inline std::unique_ptr<TopoDS_Shape> parcad_sweep_shell(const TopoDS_Wire& spine,
                                                        const TopoDS_Wire& profile, int mode,
                                                        double scale_end) {
  try {
    BRepOffsetAPI_MakePipeShell pipe(spine);
    switch (mode) {
      case 1:
        pipe.SetMode(true);
        break;
      case 2:
        pipe.SetMode(gp_Dir(0.0, 0.0, 1.0));
        break;
      default:
        pipe.SetMode(false);
        break;
    }
    if (scale_end != 1.0) {
      Handle(Law_Linear) law = new Law_Linear();
      law->Set(0.0, 1.0, 1.0, scale_end);
      pipe.SetLaw(profile, law, false, false);
    } else {
      pipe.Add(profile, false, false);
    }
    pipe.Build();
    if (!pipe.IsDone()) {
      throw std::runtime_error("the sweep builder reported it could not sweep this profile");
    }
    if (!pipe.MakeSolid()) {
      throw std::runtime_error("the swept shell could not be closed into a solid");
    }
    return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape(pipe.Shape()));
  } catch (const Standard_Failure& raised) {
    throw std::runtime_error(std::string("the sweep builder raised: ") +
                             raised.what());
  }
}
