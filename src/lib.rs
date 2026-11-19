//! WAL crash recovery. A page counts only when all 4096 bytes are present and the CRC32C at the tail matches.

use std::fs::File;
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::Path;

pub const PAGE: usize = 4096;
pub const DATA: u8 = 1;
pub const COMMIT: u8 = 2;

pub fn assemble(kind: u8, seq: u64) -> [u8; PAGE] {
    let mut page = [0u8; PAGE];
    page[0] = kind;
    page[1..9].copy_from_slice(&seq.to_le_bytes());
    let crc = crc32c::crc32c(&page[..PAGE - 4]);
    page[PAGE - 4..].copy_from_slice(&crc.to_le_bytes());
    page
}

pub fn page_ok(page: &[u8]) -> bool {
    if page.len() != PAGE {
        return false;
    }
    let stored = u32::from_le_bytes(page[PAGE - 4..].try_into().unwrap());
    crc32c::crc32c(&page[..PAGE - 4]) == stored
}

/// Scan only full pages. A short tail or a bad CRC stops the scan.
/// Commit sequences come only from valid commit pages. Bytes after the last
/// valid commit page are not committed, even when those pages are intact.
pub fn committed_seqs(log: &[u8]) -> Vec<u64> {
    let mut seqs = Vec::new();
    let mut offset = 0;
    while offset + PAGE <= log.len() {
        let page = &log[offset..offset + PAGE];
        if !page_ok(page) {
            break;
        }
        if page[0] == COMMIT {
            seqs.push(u64::from_le_bytes(page[1..9].try_into().unwrap()));
        }
        offset += PAGE;
    }
    seqs
}

pub fn durable(file: &File) -> io::Result<()> {
    let fd = file.as_raw_fd();
    let rc = unsafe {
        #[cfg(target_os = "linux")]
        {
            libc::fdatasync(fd)
        }
        #[cfg(not(target_os = "linux"))]
        {
            libc::fsync(fd)
        }
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn durable_dir(path: &Path) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    durable(&File::open(dir)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_tail_is_discarded() {
        let commit = assemble(COMMIT, 1);
        let mut log = commit.to_vec();
        log.extend_from_slice(&[0xab; 100]);
        assert_eq!(committed_seqs(&log), vec![1]);
    }

    #[test]
    fn torn_page_does_not_validate() {
        let page = assemble(DATA, 7);
        assert!(!page_ok(&page[..100]));
        assert!(!page_ok(&page[..PAGE - 1]));
    }

    #[test]
    fn bad_crc_stops_before_that_page() {
        let mut log = assemble(COMMIT, 1).to_vec();
        let mut broken = assemble(COMMIT, 2);
        broken[PAGE - 1] ^= 0xff;
        log.extend_from_slice(&broken);
        assert_eq!(committed_seqs(&log), vec![1]);
    }

    #[test]
    fn data_after_commit_is_not_a_new_commit() {
        let mut log = assemble(COMMIT, 4).to_vec();
        log.extend_from_slice(&assemble(DATA, 9));
        assert_eq!(committed_seqs(&log), vec![4]);
    }

    #[test]
    fn two_scans_match() {
        let mut log = assemble(COMMIT, 1).to_vec();
        log.extend_from_slice(&assemble(COMMIT, 2));
        log.extend_from_slice(&[1, 2, 3]);
        assert_eq!(committed_seqs(&log), committed_seqs(&log));
    }
}
