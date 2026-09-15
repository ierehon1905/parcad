//! Pictures a case holds to hand-derived numbers, drawn the way the app
//! draws them: the same rasteriser, the same per-triangle faces and bodies,
//! the same tag each face is coloured for.

use crate::case::{pct_check, Mismatch, RenderExpect};
use parcad_core::graph::Doc;
use parcad_core::render::{self, RenderOptions, Surface};
use parcad_core::view::{Axis, Keep, Section, View};
use parcad_occt::drawing::{owner_of_face, tag_names, TriangleOwners};

pub fn check(doc: &Doc, renders: &[RenderExpect]) -> Vec<Mismatch> {
    let mut out = Vec::new();
    let s = match parcad_occt::evaluate(doc, &parcad_occt::Options::default()) {
        Ok(s) => s,
        Err(e) => {
            out.push(Mismatch { field: "renders".into(), detail: e.to_string() });
            return out;
        }
    };
    let points: Vec<[f32; 3]> = s.positions.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    let Some(bounds) = parcad_core::measure::Aabb::from_points(&points) else {
        out.push(Mismatch { field: "renders".into(), detail: "the kernel returned no vertices".into() });
        return out;
    };
    let owners = TriangleOwners::of(&s);
    let surface = Surface {
        positions: &s.positions,
        normals: &s.normals,
        indices: &s.indices,
        faces: &owners.faces,
        bodies: &owners.bodies,
    };
    let names = tag_names(doc);
    let owner = owner_of_face(&s.faces, &names);

    for (i, want) in renders.iter().enumerate() {
        let field = |f: &str| format!("renders[{i}].{f}");
        let mut fail = |f: &str, detail: String| out.push(Mismatch { field: field(f), detail });
        let Some(view) = View::parse(&want.view) else {
            fail("view", format!("no view called {:?}", want.view));
            continue;
        };
        let section = match &want.section {
            None => None,
            Some(sec) => {
                let axis = Axis::parse(&sec.axis);
                let keep = sec.keep.as_deref().map(Keep::parse);
                match (axis, keep) {
                    (Some(axis), None) => Some(Section { axis, at_mm: sec.at_mm, keep: None }),
                    (Some(axis), Some(Some(keep))) => Some(Section { axis, at_mm: sec.at_mm, keep: Some(keep) }),
                    _ => {
                        fail("section", format!("{sec:?} is not a section: axis x, y or z, keep above or below"));
                        continue;
                    }
                }
            }
        };
        let opts = RenderOptions { size: want.size, depth_samples: want.size, section, ..Default::default() };
        let buf = match render::raster(&surface, bounds, view, &opts) {
            Ok(buf) => buf,
            Err(e) => {
                fail("view", format!("{e:#}"));
                continue;
            }
        };

        if !want.tag_fraction.is_empty() {
            match parcad_core::tags::regions_by_face(&buf, &owner, &names, &opts) {
                Err(e) => fail("tag_fraction", format!("{e:#}")),
                Ok(map) => {
                    for (tag, [lo, hi]) in &want.tag_fraction {
                        match map.legend.iter().find(|e| &e.tag == tag) {
                            None => fail(&format!("tag_fraction.{tag}"), "no such tag in the legend".into()),
                            Some(e) if e.fraction < *lo || e.fraction > *hi => fail(
                                &format!("tag_fraction.{tag}"),
                                format!("expected {lo:.3} to {hi:.3}, measured {:.4} ({} pixels)", e.fraction, e.pixels),
                            ),
                            Some(_) => {}
                        }
                    }
                }
            }
        }

        if let Some(area) = want.cut_area_mm2 {
            match buf.cut_plane {
                None => fail("cut_area_mm2", "the view has no section".into()),
                Some(cut) => {
                    let mm_per_px = buf.screen_to_model.column(0).norm() as f64;
                    let facing = cut.normal().dot(&view.axes().2).abs();
                    let pixels = buf.cut.iter().filter(|c| **c).count();
                    let got = pixels as f64 * mm_per_px * mm_per_px / facing.max(1e-9);
                    let mut bad = Vec::new();
                    pct_check(&mut bad, &field("cut_area_mm2"), area, got, want.cut_area_pct.unwrap_or(2.0));
                    out.extend(bad);
                }
            }
        }

        for (b, region) in want.cut_within.iter().enumerate() {
            let inside = |p: [f32; 3]| (0..3).all(|k| (region[k]..=region[k + 3]).contains(&(p[k] as f64)));
            let (mut seen, mut open) = (0usize, Vec::new());
            for y in 0..buf.size {
                for x in 0..buf.size {
                    if buf.plane_point(x, y).is_some_and(inside) {
                        seen += 1;
                        if !buf.is_cut(x, y) {
                            open.push((x, y));
                        }
                    }
                }
            }
            let f = format!("cut_within[{b}]");
            if seen == 0 {
                out.push(Mismatch { field: field(&f), detail: "no pixel of the plane lies in this box".into() });
            } else if !open.is_empty() {
                out.push(Mismatch {
                    field: field(&f),
                    detail: format!(
                        "{} of {seen} pixels on the plane here are not cut face, e.g. {:?}",
                        open.len(),
                        &open[..open.len().min(4)]
                    ),
                });
            }
        }
    }
    out
}
