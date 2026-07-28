use rand::prelude::*;
use zeroize::Zeroize;

/// A 32-byte secret held in a heap allocation that:
/// - is best-effort `mlock`'d (excluded from swap) for as long as it lives,
/// - is zeroized in place on drop, before being `munlock`'d.
///
/// Uses `Box<[u8; 32]>` rather than an inline array so the underlying address
/// stays stable even if the owning struct itself is moved (moving a `Box`
/// only moves the pointer, not the heap allocation it points to) a plain
/// `[u8; 32]` field would silently invalidate its `mlock` on every move.
pub struct LockedSecret {
    bytes: Box<[u8; 32]>,
    locked: bool,
}

impl LockedSecret {
    /// Allocates a zeroed, page-locked buffer, then copies `src` into it in place.
    pub fn from_bytes(src: &[u8; 32]) -> Self {
        let mut boxed: Box<[u8; 32]> = Box::new([0u8; 32]);
        let locked = Self::lock(&mut boxed);
        boxed.copy_from_slice(src);
        Self { bytes: boxed, locked }
    }

    /// Allocates a zeroed, page-locked buffer, then fills it directly from the
    /// CSPRNG — avoids the extra unlocked stack copy that generating the
    /// bytes first and copying them in afterward would leave behind.
    pub fn generate_random() -> Self {
        let mut boxed: Box<[u8; 32]> = Box::new([0u8; 32]);
        let locked = Self::lock(&mut boxed);
        rand::rng().fill(boxed.as_mut());
        Self { bytes: boxed, locked }
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    fn lock(boxed: &mut Box<[u8; 32]>) -> bool {
        // Best-effort: mlock can fail (e.g. RLIMIT_MEMLOCK exceeded). That must
        // never be fatal — swap protection is defense-in-depth, not something
        // the rest of the app can depend on being available.
        let locked = unsafe { memsec::mlock(boxed.as_mut_ptr(), boxed.len()) };
        if !locked {
            eprintln!("warning: failed to mlock identity secret (RLIMIT_MEMLOCK?); continuing without swap protection");
        }
        locked
    }
}

impl Drop for LockedSecret {
    fn drop(&mut self) {
        self.bytes.zeroize();
        if self.locked {
            unsafe { memsec::munlock(self.bytes.as_mut_ptr(), self.bytes.len()) };
        }
    }
}

impl std::fmt::Debug for LockedSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LockedSecret(REDACTED)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_bytes_round_trips() {
        let src = [42u8; 32];
        let secret = LockedSecret::from_bytes(&src);
        assert_eq!(secret.as_bytes(), &src);
    }

    #[test]
    fn generate_random_is_not_all_zero() {
        let secret = LockedSecret::generate_random();
        assert_ne!(secret.as_bytes(), &[0u8; 32]);
    }

    #[test]
    fn debug_is_redacted() {
        let secret = LockedSecret::from_bytes(&[1u8; 32]);
        assert_eq!(format!("{secret:?}"), "LockedSecret(REDACTED)");
    }

    #[test]
    fn lock_failure_is_non_fatal() {
        // mlock may or may not actually succeed in a sandboxed/CI environment
        // (RLIMIT_MEMLOCK) — either outcome must be safe to construct and drop.
        let secret = LockedSecret::from_bytes(&[7u8; 32]);
        drop(secret);
    }
}
