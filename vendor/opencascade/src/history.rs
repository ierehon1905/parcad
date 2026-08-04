//! Thin access to OCCT's operation history.
//!
//! `TopoDS` array positions are transient, but an OCCT builder can tell us how
//! a concrete input sub-shape evolved through the operation that just ran.
//! Keeping this bridge here lets the high-level wrapper expose that fact without
//! leaking C++ handles to parcad itself.

#[cxx::bridge]
pub(crate) mod ffi {
    unsafe extern "C++" {
        include!("include/history.hxx");

        type TopoDS_Shape = opencascade_sys::ffi::TopoDS_Shape;
        type ParcadBoolean;

        fn parcad_cut_with_history(
            base: &TopoDS_Shape,
            tool: &TopoDS_Shape,
        ) -> UniquePtr<ParcadBoolean>;
        fn parcad_fuse_with_history(
            base: &TopoDS_Shape,
            tool: &TopoDS_Shape,
        ) -> UniquePtr<ParcadBoolean>;

        fn result(self: &ParcadBoolean) -> &TopoDS_Shape;
        fn section_edges(self: &ParcadBoolean) -> UniquePtr<CxxVector<TopoDS_Shape>>;
        fn modified(
            self: &ParcadBoolean,
            original: &TopoDS_Shape,
        ) -> UniquePtr<CxxVector<TopoDS_Shape>>;
        fn is_deleted(self: &ParcadBoolean, original: &TopoDS_Shape) -> bool;
    }
}
