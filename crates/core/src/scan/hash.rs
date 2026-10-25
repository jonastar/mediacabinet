//! Content hashing for file identity.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

const CHUNK: u64 = 64 * 1024;

/// A fast fingerprint: BLAKE3 over the size and three 64 KiB samples (start, middle,
/// end). Small files are hashed whole. Enough to recognise a moved or copied file
/// without reading whole videos; the `b3p:` prefix leaves room for a full hash later.
pub fn partial_hash(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let mut hasher = blake3::Hasher::new();
    hasher.update(&size.to_le_bytes());

    if size <= 3 * CHUNK {
        io::copy(&mut file, &mut hasher)?;
    } else {
        let mut buf = vec![0; CHUNK as usize];
        for offset in [0, size / 2 - CHUNK / 2, size - CHUNK] {
            file.seek(SeekFrom::Start(offset))?;
            file.read_exact(&mut buf)?;
            hasher.update(&buf);
        }
    }
    Ok(format!("b3p:{}", hasher.finalize().to_hex()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn differs_by_content_and_size() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, data: &[u8]| {
            let path = dir.path().join(name);
            std::fs::write(&path, data).unwrap();
            partial_hash(&path).unwrap()
        };

        let big = vec![7u8; (CHUNK * 4) as usize];
        let mut big_changed_middle = big.clone();
        big_changed_middle[(CHUNK * 2) as usize] = 8;

        assert_eq!(write("a", b"hello"), write("b", b"hello"));
        assert_ne!(write("c", b"hello"), write("d", b"hellp"));
        assert_ne!(write("e", &big), write("f", &big_changed_middle));
        assert_ne!(write("g", &big), write("h", &big[..big.len() - 1]));
    }
}
