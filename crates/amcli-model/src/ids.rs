//! Identifier generation, matching what Archi writes.

/// A fresh id in Archi's own format: `UUIDFactory.java` produces
/// `"id-" + UUID.randomUUID().toString().replace("-", "")`.
///
/// Reading is deliberately more permissive than writing. The ecore types `id`
/// as a plain string, and real models carry short hex (`be0eecc1`), bare
/// integers (`650`) and dashed UUIDs, so nothing in amcli validates an id
/// against this shape — only the generator uses it.
pub fn new_id() -> String {
    let mut out = String::with_capacity(35);
    out.push_str("id-");
    for byte in random_bytes() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// 16 random bytes, seeded from the OS.
///
/// Rolling this by hand rather than taking a dependency: ids only need to not
/// collide, and the OS entropy source is right there.
fn random_bytes() -> [u8; 16] {
    let mut buf = [0u8; 16];
    if getrandom(&mut buf) {
        return buf;
    }
    // Fallback for the case where the OS refuses: mix the clock, the pid and
    // the address of a stack local. Weaker, but still collision-free in
    // practice for ids within one model.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    let stack = &buf as *const _ as u128;
    let mut state = nanos ^ (pid << 64) ^ stack;
    for b in buf.iter_mut() {
        // xorshift, sufficient for spreading the seed across the bytes
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *b = (state & 0xff) as u8;
    }
    buf
}

#[cfg(unix)]
fn getrandom(buf: &mut [u8; 16]) -> bool {
    use std::io::Read;
    std::fs::File::open("/dev/urandom").map(|mut f| f.read_exact(buf).is_ok()).unwrap_or(false)
}

#[cfg(not(unix))]
fn getrandom(_buf: &mut [u8; 16]) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_look_like_archi_ids() {
        let id = new_id();
        assert_eq!(id.len(), 35, "`id-` plus 32 hex characters");
        assert!(id.starts_with("id-"));
        assert!(id[3..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn ids_do_not_collide() {
        let set: std::collections::HashSet<String> = (0..10_000).map(|_| new_id()).collect();
        assert_eq!(set.len(), 10_000);
    }
}
