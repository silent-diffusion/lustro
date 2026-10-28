//! Wire format between the app and the elevated helper. See `crates/lustro-scan-helper/PROTOCOL.md`.
//!
//! Every frame is `len: u32 LE` followed by `len` bytes: a kind byte and its body.
//! All integers are little-endian. Decoding never panics, rejects unknown kinds,
//! unknown flag bits, out-of-range lengths and trailing bytes.

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};

use lustro_core::model::{EntryFlags, EntryMeta, FileId, NodeKind, ScanRecord};

use crate::windows_mft::RawRecord;

pub const VERSION: u16 = 1;
pub const MAGIC: [u8; 4] = *b"SVSH";
/// Largest frame body accepted, in bytes.
pub const MAX_FRAME: usize = 4 << 20;
/// Most records in one `Records` frame.
pub const MAX_RECORDS: usize = 4096;
/// Longest UTF-8 text (reasons, messages), in bytes.
pub const MAX_TEXT: usize = 1024;
/// Longest UTF-16 string (names, paths), in units.
pub const MAX_WIDE: usize = 32_767;

const NO_PARENT: u32 = u32::MAX;
const NO_TIME: i64 = i64::MIN;

mod kind {
    pub const HELLO: u8 = 1;
    pub const RECORDS: u8 = 2;
    pub const WARNING: u8 = 3;
    pub const PING: u8 = 4;
    pub const DONE: u8 = 5;
    pub const UNAVAILABLE: u8 = 6;
    pub const FAILED: u8 = 7;
    pub const REQUEST: u8 = 0x81;
}

/// Helper → app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToParent {
    /// First frame. `pid` is the helper's process id.
    Hello {
        version: u16,
        pid: u32,
    },
    Records(Vec<RawRecord>),
    Warning {
        path: OsString,
        message: String,
    },
    /// Liveness, sent every ~250 ms. Also how the helper notices the app has gone.
    Ping,
    Done {
        cancelled: bool,
    },
    /// The MFT can't be read (not NTFS, ...). Sent before any records.
    Unavailable(String),
    Failed(String),
}

/// App → helper. Exactly one, right after `Hello`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToHelper {
    Request { version: u16, root: OsString },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeError(pub &'static str);

/// Reads the 4-byte length prefix and checks its bounds.
pub fn frame_len(prefix: [u8; 4]) -> Result<usize, DecodeError> {
    let len = u32::from_le_bytes(prefix) as usize;
    if len == 0 || len > MAX_FRAME {
        return Err(DecodeError("frame length out of range"));
    }
    Ok(len)
}

pub fn encode_to_parent(msg: &ToParent) -> Vec<u8> {
    let mut w = Writer::frame();
    match msg {
        ToParent::Hello { version, pid } => {
            w.u8(kind::HELLO);
            w.bytes(&MAGIC);
            w.u16(*version);
            w.u32(*pid);
        }
        ToParent::Records(records) => {
            w.u8(kind::RECORDS);
            debug_assert!(!records.is_empty() && records.len() <= MAX_RECORDS);
            w.u16(records.len() as u16);
            for r in records {
                record(&mut w, r);
            }
        }
        ToParent::Warning { path, message } => {
            w.u8(kind::WARNING);
            w.wide(path);
            w.text(message);
        }
        ToParent::Ping => w.u8(kind::PING),
        ToParent::Done { cancelled } => {
            w.u8(kind::DONE);
            w.u8(u8::from(*cancelled));
        }
        ToParent::Unavailable(reason) => {
            w.u8(kind::UNAVAILABLE);
            w.text(reason);
        }
        ToParent::Failed(message) => {
            w.u8(kind::FAILED);
            w.text(message);
        }
    }
    w.finish()
}

pub fn encode_to_helper(msg: &ToHelper) -> Vec<u8> {
    let mut w = Writer::frame();
    match msg {
        ToHelper::Request { version, root } => {
            w.u8(kind::REQUEST);
            w.u16(*version);
            w.wide(root);
        }
    }
    w.finish()
}

/// Decodes a frame body (without the length prefix).
pub fn decode_to_parent(body: &[u8]) -> Result<ToParent, DecodeError> {
    let mut r = Reader(body);
    let msg = match r.u8()? {
        kind::HELLO => {
            if r.take(4)? != MAGIC {
                return Err(DecodeError("bad magic"));
            }
            ToParent::Hello {
                version: r.u16()?,
                pid: r.u32()?,
            }
        }
        kind::RECORDS => {
            let n = r.u16()? as usize;
            if n == 0 || n > MAX_RECORDS {
                return Err(DecodeError("record count out of range"));
            }
            let mut out = Vec::with_capacity(n);
            for _ in 0..n {
                out.push(read_record(&mut r)?);
            }
            ToParent::Records(out)
        }
        kind::WARNING => ToParent::Warning {
            path: r.wide()?,
            message: r.text()?,
        },
        kind::PING => ToParent::Ping,
        kind::DONE => ToParent::Done {
            cancelled: match r.u8()? {
                0 => false,
                1 => true,
                _ => return Err(DecodeError("bad bool")),
            },
        },
        kind::UNAVAILABLE => ToParent::Unavailable(r.text()?),
        kind::FAILED => ToParent::Failed(r.text()?),
        _ => return Err(DecodeError("unknown kind")),
    };
    r.end()?;
    Ok(msg)
}

pub fn decode_to_helper(body: &[u8]) -> Result<ToHelper, DecodeError> {
    let mut r = Reader(body);
    let msg = match r.u8()? {
        kind::REQUEST => ToHelper::Request {
            version: r.u16()?,
            root: r.wide()?,
        },
        _ => return Err(DecodeError("unknown kind")),
    };
    r.end()?;
    Ok(msg)
}

fn record(w: &mut Writer, raw: &RawRecord) {
    let r = &raw.record;
    let m = &r.meta;
    w.u32(r.id);
    w.u32(r.parent.unwrap_or(NO_PARENT));
    w.u8(match m.kind {
        NodeKind::File => 0,
        NodeKind::Dir => 1,
        NodeKind::Link => 2,
        NodeKind::Other => 3,
    });
    w.u32(m.flags.bits());
    w.u32(raw.win_attributes);
    w.u64(m.logical_size);
    w.u64(m.allocated_size);
    w.i64(m.modified.unwrap_or(NO_TIME));
    w.i64(m.accessed.unwrap_or(NO_TIME));
    match m.file_id {
        Some(id) => {
            w.u8(1);
            w.u64(id.volume);
            w.u64(id.index);
        }
        None => w.u8(0),
    }
    w.u32(m.link_count);
    w.wide(&m.name);
}

fn read_record(r: &mut Reader) -> Result<RawRecord, DecodeError> {
    let id = r.u32()?;
    let parent = match r.u32()? {
        NO_PARENT => None,
        p => Some(p),
    };
    let kind = match r.u8()? {
        0 => NodeKind::File,
        1 => NodeKind::Dir,
        2 => NodeKind::Link,
        3 => NodeKind::Other,
        _ => return Err(DecodeError("bad node kind")),
    };
    let flags = EntryFlags::from_bits(r.u32()?).ok_or(DecodeError("unknown flags"))?;
    let win_attributes = r.u32()?;
    let logical_size = r.u64()?;
    let allocated_size = r.u64()?;
    let time = |t: i64| (t != NO_TIME).then_some(t);
    let modified = time(r.i64()?);
    let accessed = time(r.i64()?);
    let file_id = match r.u8()? {
        0 => None,
        1 => Some(FileId {
            volume: r.u64()?,
            index: r.u64()?,
        }),
        _ => return Err(DecodeError("bad bool")),
    };
    let link_count = r.u32()?;
    let name = r.wide()?;
    Ok(RawRecord {
        record: ScanRecord {
            id,
            parent,
            meta: EntryMeta {
                name,
                kind,
                logical_size,
                allocated_size,
                modified,
                accessed,
                file_id,
                link_count,
                cloud: None,
                flags,
            },
        },
        win_attributes,
    })
}

struct Writer(Vec<u8>);

impl Writer {
    fn frame() -> Self {
        Self(vec![0; 4])
    }
    fn finish(mut self) -> Vec<u8> {
        let len = (self.0.len() - 4) as u32;
        self.0[..4].copy_from_slice(&len.to_le_bytes());
        self.0
    }
    fn bytes(&mut self, b: &[u8]) {
        self.0.extend_from_slice(b);
    }
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.bytes(&v.to_le_bytes());
    }
    /// Truncates to [`MAX_WIDE`] units (never hit by real names or paths).
    fn wide(&mut self, s: &std::ffi::OsStr) {
        let units: Vec<u16> = s.encode_wide().take(MAX_WIDE).collect();
        self.u16(units.len() as u16);
        for u in units {
            self.u16(u);
        }
    }
    /// Truncates to [`MAX_TEXT`] bytes at a char boundary.
    fn text(&mut self, s: &str) {
        let mut end = s.len().min(MAX_TEXT);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        self.u16(end as u16);
        self.bytes(&s.as_bytes()[..end]);
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if self.0.len() < n {
            return Err(DecodeError("truncated"));
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }
    fn end(&self) -> Result<(), DecodeError> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(DecodeError("trailing bytes"))
        }
    }
    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().unwrap_or_default(),
        ))
    }
    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().unwrap_or_default(),
        ))
    }
    fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().unwrap_or_default(),
        ))
    }
    fn i64(&mut self) -> Result<i64, DecodeError> {
        Ok(self.u64()? as i64)
    }
    fn wide(&mut self) -> Result<OsString, DecodeError> {
        let n = self.u16()? as usize;
        if n > MAX_WIDE {
            return Err(DecodeError("string too long"));
        }
        let units: Vec<u16> = self
            .take(n * 2)?
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Ok(OsString::from_wide(&units))
    }
    fn text(&mut self) -> Result<String, DecodeError> {
        let n = self.u16()? as usize;
        if n > MAX_TEXT {
            return Err(DecodeError("text too long"));
        }
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| DecodeError("bad utf-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: u32, parent: Option<u32>, name: &str) -> RawRecord {
        RawRecord {
            record: ScanRecord {
                id,
                parent,
                meta: EntryMeta {
                    name: name.into(),
                    kind: if parent.is_none() {
                        NodeKind::Dir
                    } else {
                        NodeKind::File
                    },
                    logical_size: 123_456_789_012,
                    allocated_size: 4096,
                    modified: Some(1_700_000_000),
                    accessed: None,
                    file_id: Some(FileId {
                        volume: 0xDEAD_BEEF,
                        index: (7 << 48) | 42,
                    }),
                    link_count: 2,
                    cloud: None,
                    flags: EntryFlags::HIDDEN | EntryFlags::SPARSE,
                },
            },
            win_attributes: 0x0040_0220,
        }
    }

    fn all_parent_messages() -> Vec<ToParent> {
        vec![
            ToParent::Hello {
                version: VERSION,
                pid: 4242,
            },
            ToParent::Records(vec![
                sample(0, None, ""),
                sample(1, Some(0), "report.docx"),
                sample(2, Some(0), "日本語-🙂"),
            ]),
            ToParent::Warning {
                path: r"C:\Users\William\locked".into(),
                message: "could not open".into(),
            },
            ToParent::Ping,
            ToParent::Done { cancelled: true },
            ToParent::Unavailable("not NTFS".into()),
            ToParent::Failed("volume read failed".into()),
        ]
    }

    #[test]
    fn round_trips() {
        for msg in all_parent_messages() {
            let frame = encode_to_parent(&msg);
            let len = frame_len(frame[..4].try_into().unwrap()).unwrap();
            assert_eq!(len, frame.len() - 4);
            assert_eq!(decode_to_parent(&frame[4..]).unwrap(), msg);
        }
        let req = ToHelper::Request {
            version: VERSION,
            root: r"C:\Users\William".into(),
        };
        let frame = encode_to_helper(&req);
        assert_eq!(decode_to_helper(&frame[4..]).unwrap(), req);
    }

    #[test]
    fn unpaired_surrogates_survive() {
        let name = OsString::from_wide(&[0x61, 0xD800, 0x62]);
        let mut r = sample(1, Some(0), "");
        r.record.meta.name = name.clone();
        let frame = encode_to_parent(&ToParent::Records(vec![r]));
        let ToParent::Records(back) = decode_to_parent(&frame[4..]).unwrap() else {
            panic!("wrong kind");
        };
        assert_eq!(back[0].record.meta.name, name);
    }

    #[test]
    fn full_batch_fits_in_a_frame() {
        let long = "x".repeat(255);
        let batch = (0..MAX_RECORDS as u32)
            .map(|i| sample(i + 1, Some(0), &long))
            .collect();
        let frame = encode_to_parent(&ToParent::Records(batch));
        assert!(frame.len() - 4 <= MAX_FRAME);
    }

    #[test]
    fn rejects_bad_lengths() {
        assert!(frame_len([0, 0, 0, 0]).is_err());
        assert!(frame_len(((MAX_FRAME + 1) as u32).to_le_bytes()).is_err());
        assert!(frame_len(u32::MAX.to_le_bytes()).is_err());
        assert_eq!(frame_len(1u32.to_le_bytes()), Ok(1));
    }

    #[test]
    fn rejects_malformed_bodies() {
        let bad: Vec<Vec<u8>> = vec![
            vec![],
            vec![0],
            vec![99],
            vec![kind::REQUEST, 1, 0, 0, 0], // a to-helper kind sent to the parent
            vec![kind::HELLO, b'X', b'V', b'S', b'H', 1, 0, 1, 0, 0, 0],
            vec![kind::PING, 0],                  // trailing byte
            vec![kind::DONE, 2],                  // bad bool
            vec![kind::RECORDS, 0, 0],            // zero records
            vec![kind::RECORDS, 0x01, 0x10],      // 4097 records
            vec![kind::RECORDS, 1, 0, 1, 2],      // truncated record
            vec![kind::UNAVAILABLE, 0xFF, 0xFF],  // text too long
            vec![kind::FAILED, 2, 0, 0xC3, 0x28], // invalid UTF-8
        ];
        for (i, b) in bad.iter().enumerate() {
            assert!(decode_to_parent(b).is_err(), "case {i} accepted");
        }
        assert!(decode_to_helper(&[kind::PING]).is_err());
        assert!(decode_to_helper(&[kind::REQUEST, 1, 0, 0xFF, 0xFF]).is_err());
    }

    #[test]
    fn rejects_bad_record_fields() {
        let good = encode_to_parent(&ToParent::Records(vec![sample(1, Some(0), "a")]));
        let body = &good[4..];
        assert!(decode_to_parent(body).is_ok());
        // Offsets inside the record (after kind byte + u16 count = 3 bytes).
        let kind_at = 3 + 8;
        let flags_at = kind_at + 1;
        let mut b = body.to_vec();
        b[kind_at] = 9;
        assert!(decode_to_parent(&b).is_err(), "bad kind accepted");
        let mut b = body.to_vec();
        b[flags_at + 3] = 0x80;
        assert!(decode_to_parent(&b).is_err(), "unknown flag accepted");
        // Every truncation fails cleanly.
        for cut in 0..body.len() {
            assert!(decode_to_parent(&body[..cut]).is_err());
        }
    }

    /// Deterministic pseudo-random mutation: decoding must never panic.
    #[test]
    fn fuzzish_never_panics() {
        let mut seed = 0x1234_5678_9ABC_DEF0u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let frames: Vec<Vec<u8>> = all_parent_messages()
            .iter()
            .map(|m| encode_to_parent(m)[4..].to_vec())
            .collect();
        for _ in 0..20_000 {
            let mut b = frames[(next() % frames.len() as u64) as usize].clone();
            for _ in 0..(next() % 6 + 1) {
                match next() % 3 {
                    0 if !b.is_empty() => {
                        let i = (next() % b.len() as u64) as usize;
                        b[i] = next() as u8;
                    }
                    1 if !b.is_empty() => b.truncate((next() % b.len() as u64) as usize),
                    _ => b.push(next() as u8),
                }
            }
            let _ = decode_to_parent(&b);
            let _ = decode_to_helper(&b);
        }
        for _ in 0..5_000 {
            let len = (next() % 64) as usize;
            let b: Vec<u8> = (0..len).map(|_| next() as u8).collect();
            let _ = decode_to_parent(&b);
            let _ = decode_to_helper(&b);
        }
    }
}
