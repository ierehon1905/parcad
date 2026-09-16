//! 3MF: the file a slicer opens with every body still its own named object.
//!
//! STL has no objects, so a part in several bodies reaches a slicer as one
//! triangle soup and its halves cannot be placed, coloured or printed apart.
//! This writes the 3MF core specification and nothing past it — no slicer's
//! own project settings, which are undocumented and belong to that slicer:
//! <https://github.com/3MFConsortium/spec_core/blob/master/3MF%20Core%20Specification.md>.
//!
//! The package is a ZIP, written here by hand: three entries, deflated, no
//! ZIP64. A crate for it would be most of a ZIP implementation to use a tenth.

use crate::mesh::Tessellation;
use anyhow::{bail, Result};
use std::fmt::Write as _;
use std::io::Write as _;

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#;

const RELATIONSHIPS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>"#;

/// Write named meshes as one 3MF package, one object and one build item each.
///
/// Vertices keep the part's own coordinates, with no build transform, so the
/// bodies arrive where the script put them relative to each other. Every
/// triangle must use three distinct vertices — the spec rejects anything else,
/// which [`Tessellation::weld`] already guarantees.
pub fn write_3mf(objects: &[(&str, &Tessellation)]) -> Result<Vec<u8>> {
    if objects.is_empty() {
        bail!("a 3MF needs at least one object, and the part has no bodies");
    }
    let mut model = String::new();
    model.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <model unit=\"millimeter\" xml:lang=\"en-US\" \
         xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n\
         <metadata name=\"Application\">ParCAD</metadata>\n<resources>\n",
    );
    for (i, (name, tess)) in objects.iter().enumerate() {
        if tess.triangles.is_empty() {
            bail!("body {name:?} has no triangles, and a 3MF object must have at least one");
        }
        if let Some(t) = tess.triangles.iter().find(|t| t[0] == t[1] || t[1] == t[2] || t[2] == t[0]) {
            bail!("body {name:?} has a triangle with a repeated vertex {t:?}; weld the mesh before writing it");
        }
        // f32's Display is the shortest decimal that reads back to the same
        // f32, so vertices welded together stay bit-identical in the file.
        write!(model, "<object id=\"{}\" type=\"model\" name=\"{}\"><mesh><vertices>", i + 1, escape(name))?;
        for v in &tess.vertices {
            write!(model, "<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>", v[0], v[1], v[2])?;
        }
        model.push_str("</vertices><triangles>");
        for t in &tess.triangles {
            write!(model, "<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>", t[0], t[1], t[2])?;
        }
        model.push_str("</triangles></mesh></object>\n");
    }
    model.push_str("</resources>\n<build>");
    for i in 0..objects.len() {
        write!(model, "<item objectid=\"{}\"/>", i + 1)?;
    }
    model.push_str("</build>\n</model>\n");

    zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", RELATIONSHIPS.as_bytes()),
        ("3D/3dmodel.model", model.as_bytes()),
    ])
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// A ZIP archive of deflated entries: local headers, then the central directory.
fn zip(entries: &[(&str, &[u8])]) -> Result<Vec<u8>> {
    // 1980-01-01 00:00, the earliest time a ZIP can say; a fixed stamp keeps
    // the same part's file byte-identical from one export to the next.
    const DOS_TIME: u16 = 0;
    const DOS_DATE: u16 = (1 << 5) | 1;
    const DEFLATE: u16 = 8;
    const VERSION: u16 = 20;

    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data)?;
        let packed = encoder.finish()?;
        let (Ok(size), Ok(packed_size), Ok(offset)) =
            (u32::try_from(data.len()), u32::try_from(packed.len()), u32::try_from(out.len()))
        else {
            bail!("the 3MF would be over 4 GB, which needs ZIP64; export STL for a mesh this large");
        };
        let crc = crc32fast::hash(data);

        let fields = |buf: &mut Vec<u8>| {
            buf.extend_from_slice(&VERSION.to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes()); // flags
            buf.extend_from_slice(&DEFLATE.to_le_bytes());
            buf.extend_from_slice(&DOS_TIME.to_le_bytes());
            buf.extend_from_slice(&DOS_DATE.to_le_bytes());
            buf.extend_from_slice(&crc.to_le_bytes());
            buf.extend_from_slice(&packed_size.to_le_bytes());
            buf.extend_from_slice(&size.to_le_bytes());
            buf.extend_from_slice(&(name.len() as u16).to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes()); // extra field length
        };

        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        fields(&mut out);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&packed);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&VERSION.to_le_bytes()); // made by
        fields(&mut central);
        central.extend_from_slice(&0u16.to_le_bytes()); // comment length
        central.extend_from_slice(&0u16.to_le_bytes()); // disk number
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        central.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }

    let (Ok(central_offset), Ok(central_size)) = (u32::try_from(out.len()), u32::try_from(central.len())) else {
        bail!("the 3MF would be over 4 GB, which needs ZIP64; export STL for a mesh this large");
    };
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // this disk
    out.extend_from_slice(&0u16.to_le_bytes()); // disk with the directory
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment length
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read as _;

    fn cube(side: f32, x: f32) -> Tessellation {
        let h = side / 2.0;
        let vertices = (0..8)
            .map(|i| {
                let s = |bit: usize| if i & bit != 0 { h } else { -h };
                [s(1) + x, s(2), s(4)]
            })
            .collect();
        let triangles = vec![
            [0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4],
            [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5],
        ];
        Tessellation { vertices, triangles, resolution_mm: 0.0 }
    }

    /// Read every entry back out of the archive by walking its central directory.
    fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let end = bytes.len() - 22;
        assert_eq!(u32_at(end), 0x0605_4b50, "end of central directory");
        let mut at = u32_at(end + 16);
        (0..u16_at(end + 10))
            .map(|_| {
                assert_eq!(u32_at(at), 0x0201_4b50, "central directory entry");
                let (crc, packed, size) = (u32_at(at + 16), u32_at(at + 20), u32_at(at + 24));
                let name_len = u16_at(at + 28);
                let name = String::from_utf8(bytes[at + 46..at + 46 + name_len].to_vec()).unwrap();
                let local = u32_at(at + 42);
                assert_eq!(u32_at(local), 0x0403_4b50, "local header for {name}");
                let data_at = local + 30 + u16_at(local + 26) + u16_at(local + 28);
                let mut data = Vec::new();
                flate2::read::DeflateDecoder::new(&bytes[data_at..data_at + packed])
                    .read_to_end(&mut data)
                    .unwrap();
                assert_eq!((data.len(), crc32fast::hash(&data) as usize), (size, crc), "{name}");
                at += 46 + name_len;
                (name, data)
            })
            .collect()
    }

    fn attr(tag: &str, name: &str) -> String {
        let tag = format!(" {tag}");
        let start = tag.find(&format!(" {name}=\"")).unwrap() + name.len() + 3;
        tag[start..start + tag[start..].find('"').unwrap()].to_string()
    }

    #[test]
    fn two_bodies_read_back_as_two_named_objects_with_their_own_volume() {
        let (left, right) = (cube(10.0, -20.0), cube(4.0, 20.0));
        let bytes = write_3mf(&[("base & <lid>", &left), ("lid", &right)]).unwrap();

        let entries = unzip(&bytes);
        let names: Vec<_> = entries.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["[Content_Types].xml", "_rels/.rels", "3D/3dmodel.model"]);
        let model = String::from_utf8(entries[2].1.clone()).unwrap();
        assert!(model.contains("unit=\"millimeter\""));

        let objects: Vec<&str> = model.split("<object ").skip(1).collect();
        assert_eq!(objects.len(), 2);
        assert_eq!(attr(objects[0], "name"), "base &amp; &lt;lid&gt;");
        assert_eq!(model.matches("<item objectid=").count(), 2);

        for (object, expected) in objects.iter().zip([1000.0, 64.0]) {
            let number = |v: &str, a: &str| attr(v, a).parse::<f32>().unwrap();
            let vertices: Vec<[f32; 3]> = object
                .split("<vertex ")
                .skip(1)
                .map(|v| [number(v, "x"), number(v, "y"), number(v, "z")])
                .collect();
            let triangles: Vec<[usize; 3]> = object
                .split("<triangle ")
                .skip(1)
                .map(|t| ["v1", "v2", "v3"].map(|a| attr(t, a).parse().unwrap()))
                .collect();
            let read = Tessellation { vertices, triangles, resolution_mm: 0.0 };
            let volume = crate::measure::mass_properties(&read.vertices, &read.triangles).volume_mm3;
            assert!((volume - expected).abs() < 1e-3, "volume {volume}, expected {expected}");
            assert!(read.stats().watertight);
        }
    }

    #[test]
    fn the_same_meshes_write_the_same_bytes() {
        let a = cube(3.0, 0.0);
        assert_eq!(write_3mf(&[("a", &a)]).unwrap(), write_3mf(&[("a", &a)]).unwrap());
    }

    #[test]
    fn a_collapsed_triangle_is_refused_by_name() {
        let mut a = cube(3.0, 0.0);
        a.triangles[0] = [1, 1, 2];
        let err = write_3mf(&[("peg", &a)]).unwrap_err().to_string();
        assert!(err.contains("\"peg\"") && err.contains("weld"), "{err}");
    }
}
