#pragma once

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_BuilderAlgo.hxx>
#include <TopoDS_Shape.hxx>

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
