#pragma once

#include <Standard_NoSuchObject.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_BooleanOperation.hxx>
#include <BRepAlgoAPI_BuilderAlgo.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <Message_ProgressRange.hxx>
#include <Standard_Failure.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Shape.hxx>
// OCCT 8.0 deprecated TopTools_ListOfShape along with the header that defines
// it; this is the NCollection template it named as the replacement.
#include <NCollection_List.hxx>

#include "rust/cxx.h"

#include <memory>
#include <sstream>
#include <string>
#include <vector>
#include <BRepTools_History.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <ChFi3d_FilletShape.hxx>
#include <ChFiDS_ErrorStatus.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Surface.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_MapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pnt2d.hxx>

class ParcadBoolean {
 public:
  // The (base, tool) constructors run the boolean themselves, so building
  // after them did the whole operation twice; start empty and build once.
  ParcadBoolean(const TopoDS_Shape& base, bool is_cut)
      : cut_(is_cut ? std::unique_ptr<BRepAlgoAPI_Cut>(new BRepAlgoAPI_Cut()) : nullptr),
        fuse_(is_cut ? nullptr : std::unique_ptr<BRepAlgoAPI_Fuse>(new BRepAlgoAPI_Fuse())) {
    arguments_.Append(base);
  }

  // Several tools are one boolean against the group of them: base minus
  // their union for a cut, all of them fused for a fuse. Tools may overlap.
  void add_tool(const TopoDS_Shape& tool) { tools_.Append(tool); }

  void build() {
    algorithm().SetArguments(arguments_);
    algorithm().SetTools(tools_);
    algorithm().SetToFillHistory(true);
    // OCCT's global default is serial; the face/face and edge/face loops are
    // written for OSD_Parallel, and matched serial on every part measured.
    algorithm().SetRunParallel(true);
    algorithm().Build();
  }

  const TopoDS_Shape& result() const { return algorithm().Shape(); }

  std::unique_ptr<std::vector<TopoDS_Shape>> section_edges() const {
    return shapes(algorithm().SectionEdges());
  }

  std::unique_ptr<std::vector<TopoDS_Shape>> modified(const TopoDS_Shape& original) const {
    return shapes(algorithm().Modified(original));
  }

  bool is_deleted(const TopoDS_Shape& original) const { return algorithm().IsDeleted(original); }

  // The kernel's own errors and warnings, one alert name per line; empty when
  // it raised none. A result is returned either way.
  rust::String alerts() const {
    std::ostringstream out;
    algorithm().DumpErrors(out);
    algorithm().DumpWarnings(out);
    return out.str();
  }

 private:
  BRepAlgoAPI_BooleanOperation& algorithm() const {
    return cut_ ? static_cast<BRepAlgoAPI_BooleanOperation&>(*cut_)
                : static_cast<BRepAlgoAPI_BooleanOperation&>(*fuse_);
  }

  static std::unique_ptr<std::vector<TopoDS_Shape>> shapes(const NCollection_List<TopoDS_Shape>& shapes) {
    return std::unique_ptr<std::vector<TopoDS_Shape>>(
        new std::vector<TopoDS_Shape>(shapes.begin(), shapes.end()));
  }

  mutable std::unique_ptr<BRepAlgoAPI_Cut> cut_;
  mutable std::unique_ptr<BRepAlgoAPI_Fuse> fuse_;
  NCollection_List<TopoDS_Shape> arguments_, tools_;
};

inline std::unique_ptr<ParcadBoolean> parcad_boolean_with_history(const TopoDS_Shape& base, bool is_cut) {
  return std::unique_ptr<ParcadBoolean>(new ParcadBoolean(base, is_cut));
}

inline std::unique_ptr<ParcadBoolean> parcad_cut_with_history(const TopoDS_Shape& base, const TopoDS_Shape& tool) {
  auto boolean = parcad_boolean_with_history(base, true);
  boolean->add_tool(tool);
  boolean->build();
  return boolean;
}

inline std::unique_ptr<ParcadBoolean> parcad_fuse_with_history(const TopoDS_Shape& base, const TopoDS_Shape& tool) {
  auto boolean = parcad_boolean_with_history(base, false);
  boolean->add_tool(tool);
  boolean->build();
  return boolean;
}

// The local-operation API has the same history contract as booleans: it can
// tell us which result shapes were generated from a selected input edge. Keep
// the builder alive through that query; Shape() alone discards the history.
//
// The builder works on a copy of the input's topology. BRepFilletAPI widens
// tolerances on the vertices and edges it is given, in place, and an attempt
// that builds and is then refused does it too: one probe below a failed blend
// left a vertex of the input at 42 mm tolerance, shared with every later
// attempt and with any cached shape that vertex came from. Geometry and
// triangulations are shared, not copied; the history below answers in terms
// of the caller's own input shapes.
class ParcadEdgeTreatment {
 public:
  // A fillet's section is the rolling ball's circle, or with
  // `curvature_continuous` the quintic that is G2 to the faces it joins
  // (ChFi3d_CurvatureContinuous, vendor/occt-sys/patches/0006).
  ParcadEdgeTreatment(const TopoDS_Shape& base, bool chamfer, bool curvature_continuous)
      : copy_(base, false, true),
        input_(copy_.Shape()),
        fillet_(chamfer ? nullptr
                        : std::unique_ptr<BRepFilletAPI_MakeFillet>(new BRepFilletAPI_MakeFillet(
                              input_, curvature_continuous ? ChFi3d_CurvatureContinuous : ChFi3d_Rational))),
        chamfer_(chamfer ? std::unique_ptr<BRepFilletAPI_MakeChamfer>(new BRepFilletAPI_MakeChamfer(input_)) : nullptr) {}

  void add(double distance, const TopoDS_Edge& edge) {
    const TopoDS_Edge copied = TopoDS::Edge(copy_.ModifiedShape(edge));
    treated_.push_back(copied);
    if (fillet_) fillet_->Add(distance, copied);
    else chamfer_->Add(distance, copied);
  }

  // OCCT signals an unbuildable treatment by raising Standard_Failure, which
  // nothing above this boundary catches: uncaught, it terminates the process.
  // Caught here it is an ordinary refusal, and the kernel's own words survive
  // for the caller (see failure()). A fillet that merely reports not done has
  // marked each contour it gave up on with why, and those words survive too.
  bool build() {
    try {
      if (fillet_) {
        fillet_->Build(Message_ProgressRange());
        if (!fillet_->IsDone()) failure_ = faulty_contours();
        return fillet_->IsDone();
      }
      chamfer_->Build(Message_ProgressRange());
      return chamfer_->IsDone();
    } catch (const Standard_Failure& raised) {
      const char* message = raised.what();
      failure_ = std::string(raised.ExceptionType()) + ": " +
                 ((message && *message) ? message : "no detail");
      return false;
    }
  }

  // What Build raised, verbatim, when build() returned false through the catch
  // above, or what each abandoned contour's status says; empty when the
  // builder reported not done and said nothing more.
  rust::String failure() const { return failure_; }

  // How the blends meet the faces they were rolled on, measured on the result:
  // the widest angle between the two normals, in degrees, and the largest
  // difference in normal curvature, per mm, both taken along one transverse
  // direction at 33 points of every rail. A rail is an edge a generated blend
  // face shares with the image of a face its input edge lay on; the blend's
  // ends, where a third face cuts it, are creases by design and not measured.
  // Returns the number of rails measured; zero for a chamfer or before build.
  size_t join_continuity(double& angle_deg, double& curvature_jump_per_mm) {
    angle_deg = 0.;
    curvature_jump_per_mm = 0.;
    if (!fillet_ || !fillet_->IsDone()) return 0;
    TopTools_IndexedDataMapOfShapeListOfShape input_faces_of, result_faces_of;
    TopExp::MapShapesAndAncestors(input_, TopAbs_EDGE, TopAbs_FACE, input_faces_of);
    TopExp::MapShapesAndAncestors(fillet_->Shape(), TopAbs_EDGE, TopAbs_FACE, result_faces_of);
    size_t rails = 0;
    for (const TopoDS_Edge& treated : treated_) {
      if (!input_faces_of.Contains(treated)) continue;
      TopTools_MapOfShape images;
      for (const TopoDS_Shape& adjacent : input_faces_of.FindFromKey(treated)) {
        for (const TopoDS_Shape& image : fillet_->Modified(adjacent)) images.Add(image);
      }
      for (const TopoDS_Shape& generated : fillet_->Generated(treated)) {
        if (generated.ShapeType() != TopAbs_FACE) continue;
        const TopoDS_Face blend = TopoDS::Face(generated);
        for (TopExp_Explorer along(blend, TopAbs_EDGE); along.More(); along.Next()) {
          const TopoDS_Edge rail = TopoDS::Edge(along.Current());
          if (BRep_Tool::Degenerated(rail) || !result_faces_of.Contains(rail)) continue;
          for (const TopoDS_Shape& other : result_faces_of.FindFromKey(rail)) {
            if (other.IsSame(blend) || !images.Contains(other)) continue;
            join_along(rail, blend, TopoDS::Face(other), angle_deg, curvature_jump_per_mm);
            rails++;
          }
        }
      }
    }
    return rails;
  }

  const TopoDS_Shape& result() {
    return fillet_ ? fillet_->Shape() : chamfer_->Shape();
  }

  // The copy the builder treated: the result's untouched faces are its faces.
  const TopoDS_Shape& input() const { return input_; }

  std::unique_ptr<std::vector<TopoDS_Shape>> generated(const TopoDS_Edge& original) {
    TopoDS_Shape copied;
    if (!copy_of(original, copied)) {
      return shapes(NCollection_List<TopoDS_Shape>());
    }
    return fillet_ ? shapes(fillet_->Generated(copied)) : shapes(chamfer_->Generated(copied));
  }

  // The boolean's contract, for a treatment: what a face or edge of the input
  // became, and whether it is gone. What lets a name on a face outlive the
  // fillet that trims it. A shape the treatment left alone became its copy,
  // which is what the result holds.
  std::unique_ptr<std::vector<TopoDS_Shape>> modified(const TopoDS_Shape& original) {
    TopoDS_Shape copied;
    if (!copy_of(original, copied)) {
      return shapes(NCollection_List<TopoDS_Shape>());
    }
    auto out = fillet_ ? shapes(fillet_->Modified(copied)) : shapes(chamfer_->Modified(copied));
    if (out->empty() && !is_deleted(original)) {
      out->push_back(copied);
    }
    return out;
  }

  // A shape that is not part of the treated input is not part of the result.
  bool is_deleted(const TopoDS_Shape& original) {
    TopoDS_Shape copied;
    if (!copy_of(original, copied)) {
      return true;
    }
    return fillet_ ? fillet_->IsDeleted(copied) : chamfer_->IsDeleted(copied);
  }

 private:
  // The copy of `original`, or false when it is not a sub-shape of the input:
  // `BRepTools_Modifier::ModifiedShape` raises for one, and a caller's lineage
  // can still name a face an earlier step replaced.
  bool copy_of(const TopoDS_Shape& original, TopoDS_Shape& copied) const {
    try {
      copied = copy_.ModifiedShape(original);
      return true;
    } catch (const Standard_NoSuchObject&) {
      return false;
    }
  }

  static std::unique_ptr<std::vector<TopoDS_Shape>> shapes(const NCollection_List<TopoDS_Shape>& shapes) {
    return std::unique_ptr<std::vector<TopoDS_Shape>>(
        new std::vector<TopoDS_Shape>(shapes.begin(), shapes.end()));
  }

  // Each contour the fillet builder abandoned, with the status it left on it.
  std::string faulty_contours() const {
    std::ostringstream out;
    for (int i = 1; i <= fillet_->NbFaultyContours(); i++) {
      const int contour = fillet_->FaultyContour(i);
      out << (i > 1 ? "; " : "") << "contour " << contour << ": ";
      switch (fillet_->StripeStatus(contour)) {
        case ChFiDS_WalkingFailure: out << "the rolling ball could not be marched along it"; break;
        case ChFiDS_StartsolFailure: out << "no starting section was found on it"; break;
        case ChFiDS_TwistedSurface: out << "its blend surface would twist"; break;
        case ChFiDS_SectionNotApplicable:
          out << "a curvature-continuous section is built between two faces with one radius, and this "
                 "contour runs along a face boundary";
          break;
        case ChFiDS_Ok:
        case ChFiDS_Error: out << "the kernel gave up on it without saying why"; break;
      }
    }
    return out.str();
  }

  // Normal curvature of S at (u, v) along the unit tangent direction tau,
  // against the reference normal N: the second derivative of the surface
  // along tau, with tau written in the tangent basis through the first
  // fundamental form, projected on N.
  static double normal_curvature(const BRepAdaptor_Surface& S, double u, double v, const gp_Vec& tau,
                                 const gp_Vec& N) {
    gp_Pnt P;
    gp_Vec Du, Dv, Duu, Dvv, Duv;
    S.D2(u, v, P, Du, Dv, Duu, Dvv, Duv);
    const double E = Du.Dot(Du), F = Du.Dot(Dv), G = Dv.Dot(Dv);
    const double a = tau.Dot(Du), b = tau.Dot(Dv), det = E * G - F * F;
    if (std::abs(det) < Precision::SquareConfusion()) return 0.;
    const double alpha = (G * a - F * b) / det, beta = (E * b - F * a) / det;
    gp_Vec Dtt;
    Dtt.SetLinearForm(alpha * alpha, Duu, 2. * alpha * beta, Duv, beta * beta, Dvv);
    const double speed2 = alpha * alpha * E + 2. * alpha * beta * F + beta * beta * G;
    return speed2 < Precision::SquareConfusion() ? 0. : Dtt.Dot(N) / speed2;
  }

  // Widen `angle_deg` and `jump` by what the join of F and G along `rail`
  // measures at 33 points: the angle between the two normals, and the
  // difference of their normal curvatures across the rail, the direction
  // Pegna and Wolter's linkage theorem says decides curvature continuity for
  // two surfaces already tangent along a curve.
  static void join_along(const TopoDS_Edge& rail, const TopoDS_Face& F, const TopoDS_Face& G, double& angle_deg,
                         double& jump) {
    const BRepAdaptor_Surface sF(F), sG(G);
    const BRepAdaptor_Curve along(rail);
    double f, l, f2, l2;
    const opencascade::handle<Geom2d_Curve> onF = BRep_Tool::CurveOnSurface(rail, F, f, l);
    const opencascade::handle<Geom2d_Curve> onG = BRep_Tool::CurveOnSurface(rail, G, f2, l2);
    if (onF.IsNull() || onG.IsNull()) return;
    const int samples = 33;
    for (int k = 0; k < samples; k++) {
      const double t = f + (l - f) * k / (samples - 1);
      const gp_Pnt2d uvF = onF->Value(t), uvG = onG->Value(t);
      gp_Pnt P;
      gp_Vec Du, Dv, T;
      sF.D1(uvF.X(), uvF.Y(), P, Du, Dv);
      gp_Vec nF = Du.Crossed(Dv);
      sG.D1(uvG.X(), uvG.Y(), P, Du, Dv);
      gp_Vec nG = Du.Crossed(Dv);
      along.D1(t, P, T);
      if (nF.SquareMagnitude() < Precision::SquareConfusion() || nG.SquareMagnitude() < Precision::SquareConfusion() ||
          T.SquareMagnitude() < Precision::SquareConfusion()) {
        continue;
      }
      nF.Normalize();
      nG.Normalize();
      angle_deg = std::max(angle_deg, std::acos(std::min(1., std::abs(nF.Dot(nG)))) * 180. / M_PI);
      gp_Vec tau = nG.Crossed(T);
      if (tau.SquareMagnitude() < Precision::SquareConfusion()) continue;
      tau.Normalize();
      jump = std::max(jump, std::abs(normal_curvature(sF, uvF.X(), uvF.Y(), tau, nG) -
                                     normal_curvature(sG, uvG.X(), uvG.Y(), tau, nG)));
    }
  }

  BRepBuilderAPI_Copy copy_;
  TopoDS_Shape input_;
  std::unique_ptr<BRepFilletAPI_MakeFillet> fillet_;
  std::unique_ptr<BRepFilletAPI_MakeChamfer> chamfer_;
  std::vector<TopoDS_Edge> treated_;
  std::string failure_;
};

inline std::unique_ptr<ParcadEdgeTreatment> parcad_fillet_with_history(const TopoDS_Shape& base,
                                                                        bool curvature_continuous) {
  return std::unique_ptr<ParcadEdgeTreatment>(new ParcadEdgeTreatment(base, false, curvature_continuous));
}

inline std::unique_ptr<ParcadEdgeTreatment> parcad_chamfer_with_history(const TopoDS_Shape& base) {
  return std::unique_ptr<ParcadEdgeTreatment>(new ParcadEdgeTreatment(base, true, false));
}

// Two face representations BRepMesh cannot triangulate, rewritten without
// touching geometry, with what became of each face and edge in `history`.
//
// - INTERNAL edges: a union of two operands sharing one curved surface (a
//   sphere and its rotated copy) keeps the copy's seam imprinted on the result
//   face as an internal wire. It bounds no material, but BRepMesh meshes only
//   the region it cuts off: 4188.79 mm³ of sphere as a closed 727.70 mm³ piece.
// - No wires at all: UnifySameDomain welds two halves of a torus into a face
//   of the whole surface with no boundary, which BRepMesh skips entirely.
//   Rebuilt with the surface's natural bounds (its seams).
//
// Returns `shape` itself when neither occurs. See docs/GOTCHAS.md, "A correct
// solid can mesh as a closed fragment of itself". Added for parcad.
inline TopoDS_Shape parcad_tidy_faces(const TopoDS_Shape& shape, BRepTools_History& history) {
  BRepTools_ReShape reshape;
  BRep_Builder builder;
  bool changed = false;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current().Oriented(TopAbs_FORWARD));
    if (reshape.IsRecorded(face)) {
      continue;
    }
    if (!TopoDS_Iterator(face).More()) {
      Handle(Geom_Surface) surface = BRep_Tool::Surface(face);
      double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
      surface->Bounds(u0, u1, v0, v1);
      if (Precision::IsInfinite(u0) || Precision::IsInfinite(u1) || Precision::IsInfinite(v0) ||
          Precision::IsInfinite(v1)) {
        continue;
      }
      BRepBuilderAPI_MakeFace bounded(surface, u0, u1, v0, v1, BRep_Tool::Tolerance(face));
      if (!bounded.IsDone()) {
        continue;
      }
      reshape.Replace(face, bounded.Face());
      history.AddModified(face, bounded.Face());
      changed = true;
      continue;
    }
    bool has_internal = false;
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More() && !has_internal; e.Next()) {
      TopAbs_Orientation o = e.Current().Orientation();
      has_internal = o == TopAbs_INTERNAL || o == TopAbs_EXTERNAL;
    }
    if (!has_internal) {
      continue;
    }
    TopoDS_Face rebuilt = TopoDS::Face(face.EmptyCopied());
    for (TopoDS_Iterator w(face); w.More(); w.Next()) {
      if (w.Value().ShapeType() != TopAbs_WIRE) {
        builder.Add(rebuilt, w.Value());
        continue;
      }
      TopoDS_Wire wire;
      builder.MakeWire(wire);
      bool kept = false;
      for (TopoDS_Iterator e(w.Value()); e.More(); e.Next()) {
        TopAbs_Orientation o = e.Value().Orientation();
        if (o == TopAbs_INTERNAL || o == TopAbs_EXTERNAL) {
          history.Remove(e.Value());
          continue;
        }
        builder.Add(wire, e.Value());
        kept = true;
      }
      if (kept) {
        wire.Orientation(w.Value().Orientation());
        wire.Closed(w.Value().Closed());
        builder.Add(rebuilt, wire);
      }
    }
    reshape.Replace(face, rebuilt);
    history.AddModified(face, rebuilt);
    changed = true;
  }
  return changed ? reshape.Apply(shape) : shape;
}

// The same-domain unify pass parcad runs after every boolean, with its
// history kept. Merging the coplanar faces a fuse leaves behind is what makes
// a union read as one part, and it is also where a named face used to vanish:
// the boolean's history stops at the boolean, and the merged face is new.
// Settings match `Shape::clean` exactly; the pcurve pre-pass is the caller's.
class ParcadUnify {
 public:
  explicit ParcadUnify(const TopoDS_Shape& shape) : unify_(shape, true, true, true) {
    unify_.AllowInternalEdges(false);
    unify_.SetLinearTolerance(1.0e-4);
    unify_.SetAngularTolerance(1.0e-4);
    unify_.Build();
    BRepTools_History tidied;
    result_ = parcad_tidy_faces(unify_.Shape(), tidied);
    unify_.History()->Merge(tidied);
  }

  const TopoDS_Shape& result() const { return result_; }

  std::unique_ptr<std::vector<TopoDS_Shape>> modified(const TopoDS_Shape& original) const {
    const NCollection_List<TopoDS_Shape>& list = unify_.History()->Modified(original);
    return std::unique_ptr<std::vector<TopoDS_Shape>>(new std::vector<TopoDS_Shape>(list.begin(), list.end()));
  }

  bool is_deleted(const TopoDS_Shape& original) const { return unify_.History()->IsRemoved(original); }

 private:
  ShapeUpgrade_UnifySameDomain unify_;
  TopoDS_Shape result_;
};

inline std::unique_ptr<ParcadUnify> parcad_unify_with_history(const TopoDS_Shape& shape) {
  return std::unique_ptr<ParcadUnify>(new ParcadUnify(shape));
}

inline std::unique_ptr<TopoDS_Shape> parcad_tidy_faces_of(const TopoDS_Shape& shape) {
  BRepTools_History unused;
  return std::unique_ptr<TopoDS_Shape>(new TopoDS_Shape(parcad_tidy_faces(shape, unused)));
}
