use parcad_occt::backend::BuildCache;
use parcad_occt::packet;
use parcad_occt::protocol::breadcrumb;
use std::cell::RefCell;

/// How a reply is tagged in its 8-byte header: kind, then payload length.
const PACKET: u32 = 1;
const REFUSED: u32 = 2;

thread_local! {
    static CACHE: RefCell<BuildCache> = RefCell::new(BuildCache::default());
}

/// A buffer of `len` bytes for the page to write a request into.
#[no_mangle]
pub extern "C" fn parcad_alloc(len: usize) -> *mut u8 {
    let mut buffer = std::mem::ManuallyDrop::new(Vec::<u8>::with_capacity(len));
    buffer.as_mut_ptr()
}

/// Answer the request packet at `ptr`, taking ownership of it. The reply
/// starts with `[kind: u32, len: u32]` and is returned with [`parcad_free`].
#[no_mangle]
pub extern "C" fn parcad_call(ptr: *mut u8, len: usize) -> *mut u8 {
    let input = unsafe { Vec::from_raw_parts(ptr, len, len) };
    let outcome = std::panic::catch_unwind(|| answer(&input))
        .unwrap_or_else(|_| Err("the kernel panicked; the message is on the console".into()));
    let (kind, payload) = match outcome {
        Ok(packet) => (PACKET, packet),
        Err(message) => (REFUSED, message.into_bytes()),
    };
    let mut framed = Vec::with_capacity(8 + payload.len());
    framed.extend_from_slice(&kind.to_le_bytes());
    framed.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    framed.extend_from_slice(&payload);
    Box::into_raw(framed.into_boxed_slice()) as *mut u8
}

#[no_mangle]
pub extern "C" fn parcad_free(ptr: *mut u8) {
    let len = unsafe { u32::from_le_bytes(*(ptr.add(4) as *const [u8; 4])) } as usize;
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, 8 + len)) });
}

/// What a native worker does with one frame, with the host's files written
/// into this module's filesystem first and the ones the request wrote carried
/// back, since the host's filesystem is another module's.
fn answer(input: &[u8]) -> Result<Vec<u8>, String> {
    let (request, carried) = packet::unpack_request(input)?;
    for (path, data) in &carried {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("preparing {}: {e}", parent.display()))?;
        }
        std::fs::write(path, data)
            .map_err(|e| format!("writing {} for the kernel: {e}", path.display()))?;
    }
    let written = packet::files_written(&request);
    for path in &written {
        let _ = std::fs::remove_file(path);
    }

    let response = CACHE.with(|cache| parcad_occt::serve::run(request, &mut cache.borrow_mut()));

    let mut back = Vec::new();
    for path in written {
        if let Ok(data) = std::fs::read(&path) {
            let _ = std::fs::remove_file(&path);
            back.push((path, data));
        }
    }
    for (path, _) in carried {
        let _ = std::fs::remove_file(path);
    }
    breadcrumb("encoding the reply");
    packet::pack_response(response, &back)
}
