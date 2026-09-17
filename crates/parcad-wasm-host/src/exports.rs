use parcad_host::page::{self, Input, Outcome, Reply, Setup};
use serde_json::{json, Value};

/// How a reply is tagged: `[kind, header length, payload length]` as u32,
/// then the JSON header, then the payload.
const JSON: u32 = 0;
const BYTES: u32 = 1;
const MESHED: u32 = 2;
const HTTP: u32 = 3;
const REFUSED: u32 = 4;
const KERNEL: u32 = 5;
const PENDING: u32 = 6;

/// A buffer of `len` bytes for the worker to write an argument into.
#[no_mangle]
pub extern "C" fn host_alloc(len: usize) -> *mut u8 {
    let mut buffer = std::mem::ManuallyDrop::new(Vec::<u8>::with_capacity(len));
    buffer.as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn host_free(ptr: *mut u8) {
    let word = |at: usize| unsafe { u32::from_le_bytes(*(ptr.add(at) as *const [u8; 4])) } as usize;
    let len = 12 + word(4) + word(8);
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) });
}

fn take(ptr: *mut u8, len: usize) -> Vec<u8> {
    unsafe { Vec::from_raw_parts(ptr, len, len) }
}

fn framed(kind: u32, header: &Value, payload: &[u8]) -> *mut u8 {
    let header = serde_json::to_vec(header).unwrap_or_default();
    let mut out = Vec::with_capacity(12 + header.len() + payload.len());
    for word in [kind, header.len() as u32, payload.len() as u32] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.extend_from_slice(&header);
    out.extend_from_slice(payload);
    Box::into_raw(out.into_boxed_slice()) as *mut u8
}

fn refused(status: u16, error: String) -> *mut u8 {
    framed(REFUSED, &json!({ "status": status, "error": error }), &[])
}

fn reply(reply: Reply) -> *mut u8 {
    match reply {
        Reply::Json(value) => framed(JSON, &value, &[]),
        Reply::Bytes {
            content_type,
            bytes,
        } => framed(BYTES, &json!({ "content_type": content_type }), &bytes),
        Reply::Meshed(bytes) => framed(MESHED, &Value::Null, &bytes),
        Reply::Http(out) => framed(HTTP, &serde_json::to_value(out).unwrap_or_default(), &[]),
        Reply::Refused { status, error } => refused(status, error),
        Reply::Kernel {
            call,
            ticket,
            packet,
            timeout_ms,
        } => framed(
            KERNEL,
            &json!({ "call": call, "ticket": ticket, "timeout_ms": timeout_ms }),
            &packet,
        ),
        Reply::Pending { call, wake_ms } => {
            framed(PENDING, &json!({ "call": call, "wake_ms": wake_ms }), &[])
        }
    }
}

#[no_mangle]
pub extern "C" fn host_start(ptr: *mut u8, len: usize) -> *mut u8 {
    let setup: Setup = match serde_json::from_slice(&take(ptr, len)) {
        Ok(setup) => setup,
        Err(e) => {
            return refused(
                400,
                format!("the page started its host with a setup it cannot read: {e}"),
            )
        }
    };
    match page::start(setup) {
        Ok(()) => framed(JSON, &json!({}), &[]),
        Err(error) => refused(500, error),
    }
}

/// The link clients use, as a JSON string, or `null` while there is none.
#[no_mangle]
pub extern "C" fn host_link(ptr: *mut u8, len: usize) {
    page::set_link(serde_json::from_slice(&take(ptr, len)).unwrap_or(None));
}

#[no_mangle]
pub extern "C" fn host_viewer(ptr: *mut u8, len: usize) {
    page::set_viewer(take(ptr, len));
}

#[no_mangle]
pub extern "C" fn host_call(ptr: *mut u8, len: usize) -> *mut u8 {
    match serde_json::from_slice::<Input>(&take(ptr, len)) {
        Ok(input) => reply(page::call(input)),
        Err(e) => refused(
            400,
            format!("the page sent its host a call it cannot read: {e}"),
        ),
    }
}

#[no_mangle]
pub extern "C" fn host_retry(call: u32) -> *mut u8 {
    reply(page::retry(call.into()))
}

#[no_mangle]
pub extern "C" fn host_poll(call: u32) -> *mut u8 {
    reply(page::poll(call.into()))
}

#[no_mangle]
pub extern "C" fn host_forget(call: u32) {
    page::forget(call.into());
}

/// The kernel worker's end of a ticket: `kind` 0 is its reply packet, run in
/// `ms` milliseconds, 1 a
/// crash and 2 a timeout, each as `{ stage, detail | seconds }`, and 3 a
/// kernel that could not start, as its message.
#[no_mangle]
pub extern "C" fn host_answer(
    ticket: u32,
    kind: u32,
    ms: u32,
    ptr: *mut u8,
    len: usize,
) -> *mut u8 {
    let bytes = take(ptr, len);
    let ended = |bytes: &[u8]| serde_json::from_slice::<Value>(bytes).unwrap_or_default();
    let outcome = match kind {
        0 => Outcome::Replied(&bytes, std::time::Duration::from_millis(ms.into())),
        1 => {
            let ended = ended(&bytes);
            Outcome::Crashed {
                stage: ended["stage"]
                    .as_str()
                    .unwrap_or("an unknown operation")
                    .to_string(),
                detail: ended["detail"].as_str().unwrap_or("no detail").to_string(),
            }
        }
        3 => Outcome::Unavailable(String::from_utf8_lossy(&bytes).into_owned()),
        _ => {
            let ended = ended(&bytes);
            Outcome::TimedOut {
                stage: ended["stage"]
                    .as_str()
                    .unwrap_or("an unknown operation")
                    .to_string(),
                seconds: ended["seconds"].as_u64().unwrap_or(0),
            }
        }
    };
    match page::answer(ticket.into(), outcome) {
        Ok(()) => framed(JSON, &json!({}), &[]),
        Err(error) => refused(500, error),
    }
}

#[no_mangle]
pub extern "C" fn host_events() -> *mut u8 {
    framed(
        JSON,
        &serde_json::to_value(page::events()).unwrap_or_default(),
        &[],
    )
}
