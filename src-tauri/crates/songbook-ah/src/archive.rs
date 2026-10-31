//! Reading members out of show archives (an SQ show zip, an A&H show tar)
//! without believing them. Either can come from someone else, and both say
//! how big a member is before it is read: a crafted header can claim
//! exabytes, and a deflate bomb can unpack to far more than it claims. So a
//! member is read through a limit, never into a buffer its header sized.

use std::io::{self, Read};

/// The most one member may unpack to. The largest in a real show is a dLive
/// scene blob of about 470 KB; an SQ image is 128 KiB.
pub const MEMBER_LIMIT: u64 = 32 * 1024 * 1024;

/// The most one archive may unpack to in all. A full SQ show (300 scenes) is
/// about 40 MB; a dLive factory show is about 1 MB, and each scene adds some
/// 25 KB.
pub const ARCHIVE_LIMIT: u64 = 256 * 1024 * 1024;

const MIB: u64 = 1024 * 1024;

fn too_big(what: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn archive_too_big() -> io::Error {
    too_big(format!("the archive unpacks to more than {} MiB, which no real show comes near", ARCHIVE_LIMIT / MIB))
}

/// Read the member `name` from `r`: at most [`MEMBER_LIMIT`] and at most
/// `left`, which it is charged against. Past either it is an error, never a
/// member cut short. What size the member declares is not asked: the buffer
/// grows with what arrives, so a claim reserves nothing.
pub fn read_member(name: &str, r: impl Read, left: &mut u64) -> io::Result<Vec<u8>> {
    let limit = MEMBER_LIMIT.min(*left);
    let mut data = Vec::new();
    r.take(limit + 1).read_to_end(&mut data)?;
    let len = data.len() as u64;
    if len > limit {
        return Err(if limit < MEMBER_LIMIT {
            archive_too_big()
        } else {
            too_big(format!("{name}: unpacks to more than {} MiB, which no part of a real show comes near", MEMBER_LIMIT / MIB))
        });
    }
    data.shrink_to_fit();
    *left -= len;
    Ok(data)
}

/// A whole decompressed stream, failing rather than ending once it has given
/// [`ARCHIVE_LIMIT`] bytes and has more. A tar reads its own headers, long
/// names and the members it skips through this too, so they count.
pub struct Capped<R> {
    inner: R,
    left: u64,
}

impl<R> Capped<R> {
    pub fn new(inner: R) -> Self {
        Capped { inner, left: ARCHIVE_LIMIT }
    }
}

impl<R: Read> Read for Capped<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.left == 0 {
            return match self.inner.read(&mut [0u8; 1])? {
                0 => Ok(0),
                _ => Err(archive_too_big()),
            };
        }
        let want = buf.len().min(usize::try_from(self.left).unwrap_or(usize::MAX));
        let n = self.inner.read(&mut buf[..want])?;
        self.left -= n as u64;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_member_up_to_the_limit_is_read_whole() {
        let mut left = ARCHIVE_LIMIT;
        let d = read_member("SCENE001.DAT", io::repeat(7).take(MEMBER_LIMIT), &mut left).unwrap();
        assert_eq!(d.len() as u64, MEMBER_LIMIT);
        assert_eq!(d.capacity(), d.len());
        assert_eq!(left, ARCHIVE_LIMIT - MEMBER_LIMIT);
    }

    #[test]
    fn a_member_past_the_limit_is_refused_by_name() {
        let mut left = ARCHIVE_LIMIT;
        let e = read_member("SCENE001.DAT", io::repeat(0).take(MEMBER_LIMIT + 1), &mut left).unwrap_err();
        assert!(e.to_string().starts_with("SCENE001.DAT: unpacks to more than 32 MiB"), "{e}");
    }

    #[test]
    fn members_share_the_archive_limit() {
        let mut left = ARCHIVE_LIMIT;
        for i in 0..ARCHIVE_LIMIT / MEMBER_LIMIT {
            read_member(&format!("{i}"), io::repeat(0).take(MEMBER_LIMIT), &mut left).unwrap();
        }
        assert_eq!(left, 0);
        read_member("empty", io::empty(), &mut left).unwrap();
        let e = read_member("one more", &b"x"[..], &mut left).unwrap_err();
        assert!(e.to_string().starts_with("the archive unpacks to more than 256 MiB"), "{e}");
    }

    #[test]
    fn a_capped_stream_ends_at_the_limit_and_fails_past_it() {
        let mut exact = Capped::new(io::repeat(0).take(ARCHIVE_LIMIT));
        assert_eq!(io::copy(&mut exact, &mut io::sink()).unwrap(), ARCHIVE_LIMIT);
        let mut over = Capped::new(io::repeat(0).take(ARCHIVE_LIMIT + 1));
        let e = io::copy(&mut over, &mut io::sink()).unwrap_err();
        assert!(e.to_string().starts_with("the archive unpacks to more than 256 MiB"), "{e}");
    }
}
