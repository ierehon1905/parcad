//! A request and its reply as one buffer, for a kernel that is not a process.
//!
//! The native host and worker share a filesystem and speak JSON over pipes. In
//! a browser tab the host and the kernel are two WebAssembly modules in two Web
//! Workers, each with a filesystem of its own, and a reply's mesh is millions
//! of numbers that JSON would spell out digit by digit. So a packet is the JSON
//! with the mesh arrays taken out and appended as raw little-endian values, and
//! with the files the request reads or writes carried along by path: the STEP
//! a `step_path` asks for, the file a `probe_step` names.

use crate::protocol::{Request, Response};
use std::path::PathBuf;

/// A file a packet carries, by the path both sides use for it.
pub type Carried = (PathBuf, Vec<u8>);

/// The files a request reads, which the kernel's side needs before it runs.
pub fn files_read(request: &Request) -> Vec<PathBuf> {
    request.probe_step.iter().cloned().collect()
}

/// The files a request writes, which the host's side needs afterwards.
pub fn files_written(request: &Request) -> Vec<PathBuf> {
    request
        .step_path
        .iter()
        .chain(&request.stl_path)
        .cloned()
        .collect()
}

pub fn pack_request(request: &Request, files: &[Carried]) -> Result<Vec<u8>, String> {
    let json =
        serde_json::to_vec(request).map_err(|e| format!("encoding the kernel request: {e}"))?;
    Ok(pack(&json, &[], &[], &[], files))
}

pub fn unpack_request(bytes: &[u8]) -> Result<(Request, Vec<Carried>), String> {
    let parts = unpack(bytes)?;
    let request = serde_json::from_slice(parts.json)
        .map_err(|e| format!("the kernel could not read the request it was sent: {e}"))?;
    Ok((request, parts.files))
}

pub fn pack_response(mut response: Response, files: &[Carried]) -> Result<Vec<u8>, String> {
    let (positions, normals, indices) = match &mut response {
        Response::Ok(success) => (
            std::mem::take(&mut success.positions),
            std::mem::take(&mut success.normals),
            std::mem::take(&mut success.indices),
        ),
        _ => Default::default(),
    };
    let json =
        serde_json::to_vec(&response).map_err(|e| format!("encoding the kernel reply: {e}"))?;
    Ok(pack(&json, &positions, &normals, &indices, files))
}

pub fn unpack_response(bytes: &[u8]) -> Result<(Response, Vec<Carried>), String> {
    let parts = unpack(bytes)?;
    let mut response: Response = serde_json::from_slice(parts.json)
        .map_err(|e| format!("the host could not read the kernel's reply: {e}"))?;
    if let Response::Ok(success) = &mut response {
        success.positions = parts.positions;
        success.normals = parts.normals;
        success.indices = parts.indices;
    }
    Ok((response, parts.files))
}

/// `[json, positions, normals, indices, files]` as five u32 counts, then each
/// in that order; a file is a u32-prefixed path and a u32-prefixed body.
fn pack(
    json: &[u8],
    positions: &[f32],
    normals: &[f32],
    indices: &[u32],
    files: &[Carried],
) -> Vec<u8> {
    let carried: usize = files
        .iter()
        .map(|(path, data)| 8 + path.as_os_str().len() + data.len())
        .sum();
    let mut out = Vec::with_capacity(
        20 + json.len() + 4 * (positions.len() + normals.len() + indices.len()) + carried,
    );
    for count in [
        json.len(),
        positions.len(),
        normals.len(),
        indices.len(),
        files.len(),
    ] {
        out.extend_from_slice(&(count as u32).to_le_bytes());
    }
    out.extend_from_slice(json);
    positions
        .iter()
        .chain(normals)
        .for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
    indices
        .iter()
        .for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
    for (path, data) in files {
        let path = path.to_string_lossy();
        out.extend_from_slice(&(path.len() as u32).to_le_bytes());
        out.extend_from_slice(path.as_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

struct Parts<'a> {
    json: &'a [u8],
    positions: Vec<f32>,
    normals: Vec<f32>,
    indices: Vec<u32>,
    files: Vec<Carried>,
}

fn unpack(bytes: &[u8]) -> Result<Parts<'_>, String> {
    let mut reader = Reader { bytes, at: 0 };
    let json_len = reader.count()?;
    let positions_len = reader.count()?;
    let normals_len = reader.count()?;
    let indices_len = reader.count()?;
    let files_len = reader.count()?;
    let json = reader.take(json_len)?;
    let positions = reader
        .words(positions_len)?
        .map(f32::from_le_bytes)
        .collect();
    let normals = reader.words(normals_len)?.map(f32::from_le_bytes).collect();
    let indices = reader.words(indices_len)?.map(u32::from_le_bytes).collect();
    let mut files = Vec::with_capacity(files_len);
    for _ in 0..files_len {
        let path_len = reader.count()?;
        let path = String::from_utf8_lossy(reader.take(path_len)?).into_owned();
        let data_len = reader.count()?;
        files.push((PathBuf::from(path), reader.take(data_len)?.to_vec()));
    }
    if reader.at != bytes.len() {
        return Err(format!(
            "a kernel packet has {} bytes past its end",
            bytes.len() - reader.at
        ));
    }
    Ok(Parts {
        json,
        positions,
        normals,
        indices,
        files,
    })
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| {
                format!(
                    "a kernel packet of {} bytes ends before its contents do",
                    self.bytes.len()
                )
            })?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn count(&mut self) -> Result<usize, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")) as usize)
    }

    fn words(&mut self, count: usize) -> Result<impl Iterator<Item = [u8; 4]> + 'a, String> {
        let len = count
            .checked_mul(4)
            .ok_or("a kernel packet counts more values than fit in memory")?;
        Ok(self
            .take(len)?
            .chunks_exact(4)
            .map(|word| word.try_into().expect("four bytes")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Success;

    fn request() -> Request {
        Request {
            doc: None,
            probe_step: Some("/tmp/in.step".into()),
            fit_against: None,
            inspect_target: None,
            perceive: None,
            deflection: 0.05,
            step_path: Some("/tmp/out.step".into()),
            stl_path: None,
        }
    }

    #[test]
    fn a_request_survives_the_trip_with_its_files() {
        let files = vec![(PathBuf::from("/tmp/in.step"), b"ISO-10303-21;".to_vec())];
        let (back, carried) = unpack_request(&pack_request(&request(), &files).unwrap()).unwrap();
        assert_eq!(back.probe_step, request().probe_step);
        assert_eq!(carried, files);
        assert_eq!(files_read(&back), vec![PathBuf::from("/tmp/in.step")]);
        assert_eq!(files_written(&back), vec![PathBuf::from("/tmp/out.step")]);
    }

    #[test]
    fn a_mesh_travels_as_values_and_comes_back_exact() {
        let success: Success = serde_json::from_value(serde_json::json!({
            "positions": [0.0, 1.5, -2.25, 1e-7, 3.0, 4.0],
            "normals": [0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            "indices": [0, 1, 1],
            "deflection_mm": 0.01,
            "edges": [],
            "topology": { "faces": 1, "edges": 3 },
            "timings": { "build_ms": 1, "mesh_ms": 1, "export_ms": 0 },
            "step_path": null,
            "stl_path": null
        }))
        .unwrap_or_else(|e| panic!("a minimal success parses: {e}"));
        let expected = (
            success.positions.clone(),
            success.normals.clone(),
            success.indices.clone(),
        );
        let packed = pack_response(Response::Ok(Box::new(success)), &[]).unwrap();
        let (back, files) = unpack_response(&packed).unwrap();
        let Response::Ok(back) = back else {
            panic!("the kind survives")
        };
        assert_eq!((back.positions, back.normals, back.indices), expected);
        assert!(files.is_empty());
    }

    #[test]
    fn a_truncated_packet_is_refused_rather_than_read_short() {
        let packed = pack_request(&request(), &[(PathBuf::from("/a"), vec![1, 2, 3])]).unwrap();
        let error = unpack_request(&packed[..packed.len() - 1])
            .map(|_| ())
            .unwrap_err();
        assert!(error.contains("ends before"), "{error}");
    }
}
