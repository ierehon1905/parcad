#pragma once

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
#include <string>
#include <vector>
#include <BRepTools_History.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Geom_Surface.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Wire.hxx>

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
  ParcadEdgeTreatment(const TopoDS_Shape& base, bool chamfer)
      : copy_(base, false, true),
        input_(copy_.Shape()),
        fillet_(chamfer ? nullptr : std::unique_ptr<BRepFilletAPI_MakeFillet>(new BRepFilletAPI_MakeFillet(input_))),
        chamfer_(chamfer ? std::unique_ptr<BRepFilletAPI_MakeChamfer>(new BRepFilletAPI_MakeChamfer(input_)) : nullptr) {}

  void add(double distance, const TopoDS_Edge& edge) {
    const TopoDS_Edge copied = TopoDS::Edge(copy_.ModifiedShape(edge));
    if (fillet_) fillet_->Add(distance, copied);
    else chamfer_->Add(distance, copied);
  }

  // OCCT signals an unbuildable treatment by raising Standard_Failure, which
  // nothing above this boundary catches: uncaught, it terminates the process.
  // Caught here it is an ordinary refusal, and the kernel's own words survive
  // for the caller (see failure()).
  bool build() {
    try {
      if (fillet_) {
        fillet_->Build(Message_ProgressRange());
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
  // above; empty when the builder merely reported not done.
  rust::String failure() const { return failure_; }

  const TopoDS_Shape& result() {
    return fillet_ ? fillet_->Shape() : chamfer_->Shape();
  }

  // The copy the builder treated: the result's untouched faces are its faces.
  const TopoDS_Shape& input() const { return input_; }

  std::unique_ptr<std::vector<TopoDS_Shape>> generated(const TopoDS_Edge& original) {
    const TopoDS_Shape& copied = copy_.ModifiedShape(original);
    return fillet_ ? shapes(fillet_->Generated(copied)) : shapes(chamfer_->Generated(copied));
  }

  // The boolean's contract, for a treatment: what a face or edge of the input
  // became, and whether it is gone. What lets a name on a face outlive the
  // fillet that trims it. A shape the treatment left alone became its copy,
  // which is what the result holds.
  std::unique_ptr<std::vector<TopoDS_Shape>> modified(const TopoDS_Shape& original) {
    const TopoDS_Shape& copied = copy_.ModifiedShape(original);
    auto out = fillet_ ? shapes(fillet_->Modified(copied)) : shapes(chamfer_->Modified(copied));
    if (out->empty() && !is_deleted(original)) {
      out->push_back(copied);
    }
    return out;
  }

  bool is_deleted(const TopoDS_Shape& original) {
    const TopoDS_Shape& copied = copy_.ModifiedShape(original);
    return fillet_ ? fillet_->IsDeleted(copied) : chamfer_->IsDeleted(copied);
  }

 private:
  static std::unique_ptr<std::vector<TopoDS_Shape>> shapes(const NCollection_List<TopoDS_Shape>& shapes) {
    return std::unique_ptr<std::vector<TopoDS_Shape>>(
        new std::vector<TopoDS_Shape>(shapes.begin(), shapes.end()));
  }

  BRepBuilderAPI_Copy copy_;
  TopoDS_Shape input_;
  std::unique_ptr<BRepFilletAPI_MakeFillet> fillet_;
  std::unique_ptr<BRepFilletAPI_MakeChamfer> chamfer_;
  std::string failure_;
};

inline std::unique_ptr<ParcadEdgeTreatment> parcad_fillet_with_history(const TopoDS_Shape& base) {
  return std::unique_ptr<ParcadEdgeTreatment>(new ParcadEdgeTreatment(base, false));
}

inline std::unique_ptr<ParcadEdgeTreatment> parcad_chamfer_with_history(const TopoDS_Shape& base) {
  return std::unique_ptr<ParcadEdgeTreatment>(new ParcadEdgeTreatment(base, true));
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
