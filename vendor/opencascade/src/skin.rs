//! A solid sewn from B-spline surfaces the caller computed, with flat ends,
//! and the wall between two of them measured. Added for parcad; see
//! PARCAD-CHANGES.md.

use crate::primitives::Shape;
use glam::DVec3;

/// A B-spline surface given by its poles, `poles[i * nv + j]` for the `i`th
/// in u and `j`th in v, and its knot vectors as distinct values with
/// multiplicities.
pub struct SkinSurface<'a> {
    pub nu: usize,
    pub nv: usize,
    pub poles: &'a [DVec3],
    pub uknots: &'a [f64],
    pub umults: &'a [i32],
    pub udegree: usize,
    pub vknots: &'a [f64],
    pub vmults: &'a [i32],
    pub vdegree: usize,
}

/// Which surface a face is cut from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skin {
    Outer = 0,
    Inner = 1,
}

/// What [`Skinner::measure_wall`] found.
#[derive(Debug, Clone, Copy)]
pub struct WallReading {
    pub min_mm: f64,
    pub max_mm: f64,
    pub thinnest_at: DVec3,
    pub thickest_at: DVec3,
}

/// Faces cut from one or two surfaces, sewn into a solid by [`Skinner::build`].
pub struct Skinner {
    inner: cxx::UniquePtr<ffi::ParcadSkin>,
}

impl Default for Skinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Skinner {
    pub fn new() -> Self {
        Self { inner: ffi::parcad_skin() }
    }

    pub fn set_surface(&mut self, skin: Skin, surface: &SkinSurface) -> Result<(), String> {
        let flat: Vec<f64> = surface.poles.iter().flat_map(|p| [p.x, p.y, p.z]).collect();
        self.inner
            .pin_mut()
            .set_surface(
                skin as i32,
                surface.nu as i32,
                surface.nv as i32,
                &flat,
                surface.uknots,
                surface.umults,
                surface.udegree as i32,
                surface.vknots,
                surface.vmults,
                surface.vdegree as i32,
            )
            .map_err(|e| e.what().to_string())
    }

    /// The skin between `v0` and `v1`, all the way round, as one face.
    pub fn add_band(&mut self, skin: Skin, v0: f64, v1: f64) -> Result<(), String> {
        self.inner.pin_mut().add_band(skin as i32, v0, v1).map_err(|e| e.what().to_string())
    }

    /// The flat region the skin's iso-curve at `v` bounds.
    pub fn add_disc(&mut self, skin: Skin, v: f64) -> Result<(), String> {
        self.inner.pin_mut().add_disc(skin as i32, v).map_err(|e| e.what().to_string())
    }

    /// The flat ring between the outer skin's iso-curve at `v_outer` and the
    /// inner's at `v_inner`, which lie in one plane.
    pub fn add_ring(&mut self, v_outer: f64, v_inner: f64) -> Result<(), String> {
        self.inner.pin_mut().add_ring(v_outer, v_inner).map_err(|e| e.what().to_string())
    }

    /// State which way is out: at `(u, v)` of `skin` the outside of the part
    /// lies along `outward`. [`Self::build`] needs it.
    pub fn set_outward(&mut self, skin: Skin, u: f64, v: f64, outward: DVec3) -> Result<(), String> {
        self.inner
            .pin_mut()
            .set_outward(skin as i32, u, v, outward.x, outward.y, outward.z)
            .map_err(|e| e.what().to_string())
    }

    /// Every face added, sewn at `tolerance`, as one valid solid facing the
    /// way [`Self::set_outward`] said, checked on the solid's own face there.
    pub fn build(&mut self, tolerance: f64) -> Result<Shape, String> {
        let inner = self.inner.pin_mut().build(tolerance).map_err(|e| e.what().to_string())?;
        Ok(Shape { inner })
    }

    /// The distance from `per_u` by `per_v + 1` points of the inner skin over
    /// `[v0, v1]` to the outer skin, each found from the same parameters.
    pub fn measure_wall(&self, v0: f64, v1: f64, per_u: usize, per_v: usize) -> Result<WallReading, String> {
        self.measure_wall_reaching(v0, v1, per_u, per_v, 0.0)
    }

    /// [`Self::measure_wall`], with each foot searched at least `v_reach`
    /// either side of the inner point's `v` as well as a knot span: for an
    /// inner skin whose point `(u, v)` is the offset of the outer at another
    /// `v`.
    pub fn measure_wall_reaching(&self, v0: f64, v1: f64, per_u: usize, per_v: usize, v_reach: f64) -> Result<WallReading, String> {
        let r = self
            .inner
            .measure_wall(v0, v1, per_u as i32, per_v as i32, v_reach)
            .map_err(|e| e.what().to_string())?;
        Ok(WallReading {
            min_mm: r[0],
            max_mm: r[1],
            thinnest_at: DVec3::new(r[2], r[3], r[4]),
            thickest_at: DVec3::new(r[5], r[6], r[7]),
        })
    }
}

#[cxx::bridge]
pub(crate) mod ffi {
    unsafe extern "C++" {
        include!("include/skin.hxx");

        type TopoDS_Shape = opencascade_sys::ffi::TopoDS_Shape;
        type ParcadSkin;

        fn parcad_skin() -> UniquePtr<ParcadSkin>;
        #[allow(clippy::too_many_arguments)]
        fn set_surface(
            self: Pin<&mut ParcadSkin>,
            skin: i32,
            nu: i32,
            nv: i32,
            poles: &[f64],
            uknots: &[f64],
            umults: &[i32],
            udeg: i32,
            vknots: &[f64],
            vmults: &[i32],
            vdeg: i32,
        ) -> Result<()>;
        fn add_band(self: Pin<&mut ParcadSkin>, skin: i32, v0: f64, v1: f64) -> Result<()>;
        fn add_disc(self: Pin<&mut ParcadSkin>, skin: i32, v: f64) -> Result<()>;
        fn add_ring(self: Pin<&mut ParcadSkin>, v_outer: f64, v_inner: f64) -> Result<()>;
        fn set_outward(self: Pin<&mut ParcadSkin>, skin: i32, u: f64, v: f64, x: f64, y: f64, z: f64) -> Result<()>;
        fn build(self: Pin<&mut ParcadSkin>, tolerance: f64) -> Result<UniquePtr<TopoDS_Shape>>;
        fn measure_wall(self: &ParcadSkin, v0: f64, v1: f64, per_u: i32, per_v: i32, v_reach: f64) -> Result<Vec<f64>>;
    }
}
