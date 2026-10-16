//! Use the already enabled Windows CNG SHA256 provider; no new dependency or IO backend.
use super::Rejected;
use std::path::Path;

#[cfg(not(windows))]
pub(super) fn file(_path: &Path, _limit: u64) -> Result<String, Rejected> {
    Err(Rejected::Gate)
}

#[cfg(windows)]
pub(super) fn file(path: &Path, limit: u64) -> Result<String, Rejected> {
    use std::io::Read;
    use windows::{Win32::Security::Cryptography::*, core::PCWSTR};
    let mut algorithm = BCRYPT_ALG_HANDLE::default();
    unsafe {
        BCryptOpenAlgorithmProvider(
            &mut algorithm,
            BCRYPT_SHA256_ALGORITHM,
            PCWSTR::null(),
            BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0),
        )
    }
    .ok()
    .map_err(|_| Rejected::Path)?;
    let mut hash = BCRYPT_HASH_HANDLE::default();
    let result = (|| {
        unsafe { BCryptCreateHash(algorithm, &mut hash, None, None, 0) }
            .ok()
            .map_err(|_| Rejected::Path)?;
        let mut source = std::fs::File::open(path).map_err(|_| Rejected::Path)?;
        let mut buffer = [0; 65536];
        let mut total = 0u64;
        loop {
            let n = source.read(&mut buffer).map_err(|_| Rejected::Path)?;
            if n == 0 {
                break;
            }
            total = total.checked_add(n as u64).ok_or(Rejected::Path)?;
            if total > limit {
                return Err(Rejected::Path);
            }
            unsafe { BCryptHashData(hash, &buffer[..n], 0) }
                .ok()
                .map_err(|_| Rejected::Path)?;
        }
        let mut digest = [0; 32];
        unsafe { BCryptFinishHash(hash, &mut digest, 0) }
            .ok()
            .map_err(|_| Rejected::Path)?;
        Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
    })();
    unsafe {
        if !hash.0.is_null() {
            let _ = BCryptDestroyHash(hash);
        }
        let _ = BCryptCloseAlgorithmProvider(algorithm, 0);
    }
    result
}
