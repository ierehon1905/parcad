#pragma once

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_BuilderAlgo.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <Message_ProgressRange.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Shape.hxx>
// OCCT 8.0 moved the NCollection typedef aliases to src/Deprecated and stopped
// pulling them in transitively; each one now needs including where it is used.
#include <TopTools_ListOfShape.hxx>

#include <memory>
#include <vector>

class ParcadBoolean {
 public:
  ParcadBoolean(const TopoDS_Shape& base, const TopoDS_Shape& tool, bool is_cut)
      : cut_(is_cut ? std::unique_ptr<BRepAlgoAPI_Cut>(new BRepAlgoAPI_Cut(base, tool)) : nullptr),
        fuse_(is_cut ? nullptr : std::unique_ptr<BRepAlgoAPI_Fuse>(new BRepAlgoAPI_Fuse(base, tool))) {
    algorithm().SetToFillHistory(Standard_True);
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
  BRepAlgoAPI_BuilderAlgo& algorithm() const {
    return cut_ ? static_cast<BRepAlgoAPI_BuilderAlgo&>(*cut_)
                : static_cast<BRepAlgoAPI_BuilderAlgo&>(*fuse_);
  }

  static std::unique_ptr<std::vector<TopoDS_Shape>> shapes(const TopTools_ListOfShape& shapes) {
    return std::unique_ptr<std::vector<TopoDS_Shape>>(
        new std::vector<TopoDS_Shape>(shapes.begin(), shapes.end()));
  }

  mutable std::unique_ptr<BRepAlgoAPI_Cut> cut_;
  mutable std::unique_ptr<BRepAlgoAPI_Fuse> fuse_;
};

inline std::unique_ptr<ParcadBoolean> parcad_cut_with_history(const TopoDS_Shape& base, const TopoDS_Shape& tool) {
  return std::unique_ptr<ParcadBoolean>(new ParcadBoolean(base, tool, true));
}

inline std::unique_ptr<ParcadBoolean> parcad_fuse_with_history(const TopoDS_Shape& base, const TopoDS_Shape& tool) {
  return std::unique_ptr<ParcadBoolean>(new ParcadBoolean(base, tool, false));
}

// The local-operation API has the same history contract as booleans: it can
// tell us which result shapes were generated from a selected input edge. Keep
// the builder alive through that query; Shape() alone discards the history.
class ParcadEdgeTreatment {
 public:
  ParcadEdgeTreatment(const TopoDS_Shape& base, bool chamfer)
      : fillet_(chamfer ? nullptr : std::unique_ptr<BRepFilletAPI_MakeFillet>(new BRepFilletAPI_MakeFillet(base))),
        chamfer_(chamfer ? std::unique_ptr<BRepFilletAPI_MakeChamfer>(new BRepFilletAPI_MakeChamfer(base)) : nullptr) {}

  void add(double distance, const TopoDS_Edge& edge) {
    if (fillet_) fillet_->Add(distance, edge);
    else chamfer_->Add(distance, edge);
  }

  bool build() {
    if (fillet_) {
      fillet_->Build(Message_ProgressRange());
      return fillet_->IsDone();
    }
    chamfer_->Build(Message_ProgressRange());
    return chamfer_->IsDone();
  }

  const TopoDS_Shape& result() {
    return fillet_ ? fillet_->Shape() : chamfer_->Shape();
  }

  std::unique_ptr<std::vector<TopoDS_Shape>> generated(const TopoDS_Edge& original) {
    return fillet_ ? shapes(fillet_->Generated(original)) : shapes(chamfer_->Generated(original));
  }

 private:
  static std::unique_ptr<std::vector<TopoDS_Shape>> shapes(const TopTools_ListOfShape& shapes) {
    return std::unique_ptr<std::vector<TopoDS_Shape>>(
        new std::vector<TopoDS_Shape>(shapes.begin(), shapes.end()));
  }

  std::unique_ptr<BRepFilletAPI_MakeFillet> fillet_;
  std::unique_ptr<BRepFilletAPI_MakeChamfer> chamfer_;
};

inline std::unique_ptr<ParcadEdgeTreatment> parcad_fillet_with_history(const TopoDS_Shape& base) {
  return std::unique_ptr<ParcadEdgeTreatment>(new ParcadEdgeTreatment(base, false));
}

inline std::unique_ptr<ParcadEdgeTreatment> parcad_chamfer_with_history(const TopoDS_Shape& base) {
  return std::unique_ptr<ParcadEdgeTreatment>(new ParcadEdgeTreatment(base, true));
}
