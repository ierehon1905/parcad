#pragma once

// PARCAD: a solid skinned from B-spline surfaces the caller computed, with
// planar ends, and the wall between two such surfaces measured point by
// point. See PARCAD-CHANGES.md.

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRep_Tool.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Geometry.hxx>
#include <Geom_Surface.hxx>
#include <TopAbs_Orientation.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <ShapeFix_Face.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "rust/cxx.h"

#include <algorithm>
#include <cmath>
#include <limits>
#include <memory>
#include <stdexcept>
#include <string>
#include <typeinfo>
#include <vector>

inline std::string parcad_skin_raised(const Standard_Failure& raised) {
  const char* what = raised.what();
  std::string message = what ? what : "";
  if (message.empty()) {
    message = std::string("the kernel raised ") + typeid(raised).name();
  }
  return message;
}

// Two surfaces — the outer skin and, for a wall, the inner — and the faces
// cut from them: bands of either skin between two v values, planar discs
// bounded by one skin's v iso-curve, and planar rings between both skins' iso
// curves at one height. `build` sews them into one solid.
class ParcadSkin {
 public:
  void set_surface(int skin, int nu, int nv, rust::Slice<const double> poles,
                   rust::Slice<const double> uknots, rust::Slice<const int> umults, int udeg,
                   rust::Slice<const double> vknots, rust::Slice<const int> vmults, int vdeg) {
    try {
      if (skin < 0 || skin > 1 || nu < 2 || nv < 2 ||
          poles.size() != static_cast<size_t>(nu) * static_cast<size_t>(nv) * 3) {
        throw std::runtime_error("a skin surface needs nu x nv poles");
      }
      NCollection_Array2<gp_Pnt> grid(1, nu, 1, nv);
      for (int i = 0; i < nu; ++i) {
        for (int j = 0; j < nv; ++j) {
          const size_t at = (static_cast<size_t>(i) * nv + j) * 3;
          grid.SetValue(i + 1, j + 1, gp_Pnt(poles[at], poles[at + 1], poles[at + 2]));
        }
      }
      surfaces_[skin] = new Geom_BSplineSurface(grid, array(uknots), array(vknots), array(umults),
                                                array(vmults), udeg, vdeg, false, false);
    } catch (const Standard_Failure& raised) {
      throw std::runtime_error("building a skin surface raised: " + parcad_skin_raised(raised));
    }
  }

  // The skin between v0 and v1, all the way round, on a surface of its own:
  // faces sharing one surface handle are "same domain" to
  // ShapeUpgrade_UnifySameDomain, which welds a boolean's bands back into one
  // face that meshes many times slower.
  void add_band(int skin, double v0, double v1) {
    try {
      const Handle(Geom_BSplineSurface)& whole = surface(skin);
      double u0 = 0.0;
      double u1 = 0.0;
      double vmin = 0.0;
      double vmax = 0.0;
      whole->Bounds(u0, u1, vmin, vmax);
      Handle(Geom_BSplineSurface) s = Handle(Geom_BSplineSurface)::DownCast(whole->Copy());
      s->CheckAndSegment(u0, u1, v0, v1);
      BRepBuilderAPI_MakeFace make(s, u0, u1, v0, v1, 1e-7);
      if (!make.IsDone()) {
        throw std::runtime_error("a band of the lofted skin could not be made into a face");
      }
      faces_.push_back(make.Face());
      bands_.push_back(Band{faces_.size() - 1, skin, v0, v1});
    } catch (const Standard_Failure& raised) {
      throw std::runtime_error("a band of the lofted skin raised: " + parcad_skin_raised(raised));
    }
  }

  // A flat end: the plane region the skin's iso-curve at v bounds.
  void add_disc(int skin, double v) {
    try {
      BRepBuilderAPI_MakeFace make(iso_wire(skin, v), true);
      if (!make.IsDone()) {
        throw std::runtime_error("the loft's end outline does not bound a flat face");
      }
      faces_.push_back(make.Face());
    } catch (const Standard_Failure& raised) {
      throw std::runtime_error("a flat end of the loft raised: " + parcad_skin_raised(raised));
    }
  }

  // An open wall end: the flat ring between the outer skin at v_outer and
  // the inner at v_inner, which must lie in one plane.
  void add_ring(double v_outer, double v_inner) {
    try {
      const TopoDS_Wire outer = iso_wire(0, v_outer);
      const TopoDS_Wire inner = iso_wire(1, v_inner);
      BRepBuilderAPI_MakeFace make(outer, true);
      if (!make.IsDone()) {
        throw std::runtime_error("the wall's end outline does not bound a flat face");
      }
      make.Add(inner);
      if (!make.IsDone()) {
        throw std::runtime_error("the wall's inner end outline could not be cut from its outer one");
      }
      // The inner loop runs the way the outer does; the fix turns it round.
      ShapeFix_Face fix(make.Face());
      fix.FixOrientation();
      fix.Perform();
      faces_.push_back(fix.Face());
    } catch (const Standard_Failure& raised) {
      throw std::runtime_error("an open end of the wall raised: " + parcad_skin_raised(raised));
    }
  }

  // Which way is out: at (u, v) of `skin`, the outside of the part lies
  // along (x, y, z). `build` turns the solid to match and checks it did.
  void set_outward(int skin, double u, double v, double x, double y, double z) {
    surface(skin);
    outward_skin_ = skin;
    outward_uv_[0] = u;
    outward_uv_[1] = v;
    outward_ = gp_Vec(x, y, z);
    has_outward_ = outward_.Magnitude() > 0.0;
  }

  std::unique_ptr<TopoDS_Shape> build(double tolerance) {
    try {
      if (!has_outward_) {
        throw std::runtime_error(
            "the lofted skin was sewn without being told which way is out; call set_outward first");
      }
      BRepBuilderAPI_Sewing sewing(tolerance);
      for (const TopoDS_Face& face : faces_) {
        sewing.Add(face);
      }
      sewing.Perform();
      const TopoDS_Shape sewn = sewing.SewedShape();
      if (sewing.NbFreeEdges() > 0) {
        throw std::runtime_error(std::to_string(sewing.NbFreeEdges()) +
                                 " edges of the lofted skin were left unjoined, so it encloses nothing");
      }
      TopExp_Explorer shells(sewn, TopAbs_SHELL);
      if (!shells.More()) {
        throw std::runtime_error("the lofted faces did not sew into a shell");
      }
      const TopoDS_Shell shell = TopoDS::Shell(shells.Current());
      shells.Next();
      if (shells.More()) {
        throw std::runtime_error("the lofted faces sewed into more than one shell");
      }
      BRepBuilderAPI_MakeSolid make(shell);
      if (!make.IsDone()) {
        throw std::runtime_error("the lofted shell could not be made into a solid");
      }
      TopoDS_Solid solid = make.Solid();
      // BRepLib::OrientClosedSolid classifies a point at infinity by one ray,
      // and through a pleated shell that ray misses a crossing and reverses a
      // solid that was right; the side is known here, so it is stated.
      double facing = facing_out(solid, sewing);
      if (facing < 0.0) {
        solid.Reverse();
        facing = facing_out(solid, sewing);
      }
      if (!(facing > 0.0)) {
        throw std::runtime_error(
            "the lofted solid's faces do not turn out where the skin says outside is");
      }
      BRepCheck_Analyzer check(solid);
      if (!check.IsValid()) {
        throw std::runtime_error("the lofted solid does not pass the kernel's validity check");
      }
      return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape(solid));
    } catch (const Standard_Failure& raised) {
      throw std::runtime_error("sewing the lofted skin raised: " + parcad_skin_raised(raised));
    }
  }

  // The wall between the skins, measured from the inner one: at a grid of
  // `per_u` by `per_v` points of the inner skin over [v0, v1], the distance
  // to the nearest point of the outer skin, found by Newton's method from
  // the *same* (u, v) — both skins share their parameterisation, so that is
  // the matching point and the search stays on its own stretch of wall.
  // The search reaches at least v_reach either side in v. Returns [min, max,
  // x, y, z of the min, x, y, z of the max].
  rust::Vec<double> measure_wall(double v0, double v1, int per_u, int per_v, double v_reach) const {
    try {
      const Handle(Geom_BSplineSurface)& outer = surface(0);
      const Handle(Geom_BSplineSurface)& inner = surface(1);
      double u0 = 0.0;
      double u1 = 0.0;
      double vmin = 0.0;
      double vmax = 0.0;
      outer->Bounds(u0, u1, vmin, vmax);
      double lo = std::numeric_limits<double>::infinity();
      double hi = -lo;
      gp_Pnt at_lo;
      gp_Pnt at_hi;
      for (int a = 0; a < per_u; ++a) {
        const double u = u0 + (u1 - u0) * a / per_u;
        for (int b = 0; b <= per_v; ++b) {
          const double v = v0 + (v1 - v0) * b / per_v;
          const gp_Pnt p = inner->Value(u, v);
          const double d = nearest(outer, p, u, v, u0, u1, vmin, vmax, v_reach);
          if (d < lo) {
            lo = d;
            at_lo = p;
          }
          if (d > hi) {
            hi = d;
            at_hi = p;
          }
        }
      }
      rust::Vec<double> out;
      for (double x : {lo, hi, at_lo.X(), at_lo.Y(), at_lo.Z(), at_hi.X(), at_hi.Y(), at_hi.Z()}) {
        out.push_back(x);
      }
      return out;
    } catch (const Standard_Failure& raised) {
      throw std::runtime_error("measuring the wall raised: " + parcad_skin_raised(raised));
    }
  }

 private:
  template <typename T>
  static NCollection_Array1<T> array(rust::Slice<const T> values) {
    NCollection_Array1<T> out(1, static_cast<int>(values.size()));
    for (size_t i = 0; i < values.size(); ++i) {
      out.SetValue(static_cast<int>(i) + 1, values[i]);
    }
    return out;
  }

  const Handle(Geom_BSplineSurface)& surface(int skin) const {
    if (skin < 0 || skin > 1 || surfaces_[skin].IsNull()) {
      throw std::runtime_error("that skin has no surface yet");
    }
    return surfaces_[skin];
  }

  TopoDS_Wire iso_wire(int skin, double v) const {
    Handle(Geom_Curve) iso = surface(skin)->VIso(v);
    Handle(Geom_BSplineCurve) curve = Handle(Geom_BSplineCurve)::DownCast(iso);
    if (!curve.IsNull()) {
      // The skin closes on itself to the bit; its iso-curve must too, or the
      // edge is open by a rounding error.
      curve->SetPole(curve->NbPoles(), curve->Pole(1));
    }
    BRepBuilderAPI_MakeEdge edge(iso);
    if (!edge.IsDone()) {
      throw std::runtime_error("the loft's outline at an end could not be made into an edge");
    }
    BRepBuilderAPI_MakeWire wire(edge.Edge());
    if (!wire.IsDone()) {
      throw std::runtime_error("the loft's outline at an end could not be made into a wire");
    }
    return wire.Wire();
  }

  // Distance from p to the surface near (u, v), square to it. The foot is
  // searched only within a knot span either way of (u, v) — a coarse grid,
  // then Newton on |S - p|² held to that window, Gauss-Newton where the full
  // Hessian is not positive — so it stays on the matching stretch of wall.
  // Measured along the normal at the foot: the distance itself wherever the
  // foot is inside the surface and, at an open end where the foot is held to
  // the edge, the distance to the surface continued.
  static double nearest(const Handle(Geom_BSplineSurface)& s, const gp_Pnt& p, double u, double v,
                        double u0, double u1, double vmin, double vmax, double v_reach) {
    const double du_reach = (u1 - u0) / (s->NbUKnots() - 1);
    const double dv_reach = std::max(v_reach, (vmax - vmin) / (s->NbVKnots() - 1));
    const double period = u1 - u0;
    auto wrap = [&](double t) {
      while (t < u0) t += period;
      while (t > u1) t -= period;
      return t;
    };
    const double ulo = u - du_reach;
    const double uhi = u + du_reach;
    const double vlo = std::max(vmin, v - dv_reach);
    const double vhi = std::min(vmax, v + dv_reach);
    const int grid = 6;
    double best = std::numeric_limits<double>::infinity();
    double bu = u;
    double bv = v;
    for (int a = 0; a <= grid; ++a) {
      for (int b = 0; b <= grid; ++b) {
        const double tu = ulo + (uhi - ulo) * a / grid;
        const double tv = vlo + (vhi - vlo) * b / grid;
        const double d = p.SquareDistance(s->Value(wrap(tu), tv));
        if (d < best) {
          best = d;
          bu = tu;
          bv = tv;
        }
      }
    }
    u = bu;
    v = bv;
    gp_Pnt at;
    gp_Vec su, sv, suu, svv, suv;
    for (int iteration = 0; iteration < 20; ++iteration) {
      s->D2(wrap(u), v, at, su, sv, suu, svv, suv);
      const gp_Vec r(p, at);
      const double gu = r.Dot(su);
      const double gv = r.Dot(sv);
      double a = su.Dot(su) + r.Dot(suu);
      double b = su.Dot(sv) + r.Dot(suv);
      double c = sv.Dot(sv) + r.Dot(svv);
      if (a <= 0.0 || a * c - b * b <= 0.0) {
        a = su.Dot(su);
        b = su.Dot(sv);
        c = sv.Dot(sv);
      }
      const double det = a * c - b * b;
      if (det <= 0.0) {
        break;
      }
      const double du = -(c * gu - b * gv) / det;
      const double dv = -(a * gv - b * gu) / det;
      const double nu = std::min(uhi, std::max(ulo, u + du));
      const double nv = std::min(vhi, std::max(vlo, v + dv));
      const bool still = std::abs(nu - u) < 1e-12 && std::abs(nv - v) < 1e-12;
      u = nu;
      v = nv;
      if (still) {
        break;
      }
    }
    s->D1(wrap(u), v, at, su, sv);
    if (p.SquareDistance(at) > best) {
      s->D1(wrap(bu), bv, at, su, sv);
    }
    gp_Vec normal = su.Crossed(sv);
    if (normal.Magnitude() < 1e-12) {
      return p.Distance(at);
    }
    normal.Normalize();
    return std::abs(gp_Vec(at, p).Dot(normal));
  }

  // The cosine between the stated outward direction and the normal of the
  // solid's face through that point, as the solid presents it; 0 when no
  // band holds the point.
  double facing_out(const TopoDS_Solid& solid, BRepBuilderAPI_Sewing& sewing) const {
    for (const Band& band : bands_) {
      if (band.skin != outward_skin_ || outward_uv_[1] < band.v0 || outward_uv_[1] > band.v1) {
        continue;
      }
      TopoDS_Shape sewn = sewing.Modified(faces_[band.face]);
      for (TopExp_Explorer faces(solid, TopAbs_FACE); faces.More(); faces.Next()) {
        const TopoDS_Face face = TopoDS::Face(faces.Current());
        if (!face.IsSame(sewn)) {
          continue;
        }
        Handle(Geom_Surface) geometry = BRep_Tool::Surface(face);
        gp_Pnt at;
        gp_Vec su, sv;
        geometry->D1(outward_uv_[0], outward_uv_[1], at, su, sv);
        gp_Vec normal = su.Crossed(sv);
        if (face.Orientation() == TopAbs_REVERSED) {
          normal.Reverse();
        }
        if (normal.Magnitude() < 1e-12) {
          return 0.0;
        }
        return normal.Normalized().Dot(outward_.Normalized());
      }
      return 0.0;
    }
    return 0.0;
  }

  struct Band {
    size_t face;
    int skin;
    double v0;
    double v1;
  };

  Handle(Geom_BSplineSurface) surfaces_[2];
  std::vector<TopoDS_Face> faces_;
  std::vector<Band> bands_;
  bool has_outward_ = false;
  int outward_skin_ = 0;
  double outward_uv_[2] = {0.0, 0.0};
  gp_Vec outward_;
};

inline std::unique_ptr<ParcadSkin> parcad_skin() { return std::unique_ptr<ParcadSkin>(new ParcadSkin()); }
