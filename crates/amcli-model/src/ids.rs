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

/// 16 random bytes from the OS.
///
/// This used to read `/dev/urandom` directly under `#[cfg(unix)]` and return
/// nothing at all otherwise, which meant every id on Windows — and in any
/// container that closes `/dev/urandom` — came from the fallback below. That
/// fallback is seeded from the clock, the pid and a stack address; within one
/// process the last two are constant, so two calls landing in the same clock
/// tick produced *the same id*, and `amcli apply` wrote duplicates into the
/// model. `getrandom` is the portable form of what the old code was reaching
/// for, and taking it deleted more lines than it added.
fn random_bytes() -> [u8; 16] {
    let mut buf = [0u8; 16];
    if getrandom::fill(&mut buf).is_ok() {
        return buf;
    }
    fallback_bytes()
}

/// Only for a system with no entropy source at all.
///
/// Kept separate so it can be tested: the previous version of this code was
/// unreachable on the platform where it was wrong, so no test ran it.
fn fallback_bytes() -> [u8; 16] {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    mix(nanos)
}

/// The clock is a parameter so a test can hold it still.
///
/// That is the whole point: the defect only appeared when two calls read the
/// same tick, which never happens on a machine whose clock is fine-grained
/// enough — so a test that calls this in a loop and trusts the real clock
/// passes on macOS whether or not the bug is present.
fn mix(nanos: u128) -> [u8; 16] {
    // The counter is what keeps two calls in the same tick apart. Neither the
    // pid nor the stack address varies within a process, and xorshift is a
    // bijection on the state, so without it identical input means identical id.
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);

    let mut buf = [0u8; 16];
    let pid = std::process::id() as u128;
    let seq = SEQ.fetch_add(1, Ordering::Relaxed) as u128;
    let mut state = nanos ^ (pid << 64) ^ (seq << 32) ^ (&buf as *const _ as u128);
    for b in buf.iter_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *b = (state & 0xff) as u8;
    }
    buf
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

    /// The regression that motivated taking `getrandom`.
    ///
    /// The clock is pinned to one value, which is exactly the condition the
    /// old code got wrong: on Windows every id came from this path, and two
    /// calls in the same 100 ns tick produced byte-identical ids that then
    /// went into the model. Remove the `seq` term and this collapses to one
    /// distinct value out of ten thousand.
    #[test]
    fn the_fallback_does_not_collide_within_one_clock_tick() {
        let set: std::collections::HashSet<[u8; 16]> =
            (0..10_000).map(|_| mix(1_723_000_000_000_000_000)).collect();
        assert_eq!(set.len(), 10_000, "ids collided when the clock did not move");
    }
}
