//! NTFS on-disk structures: boot sector, file records, attributes, data runs.
//! Pure parsing over byte slices; no I/O and no `unsafe`.
//!
//! Input is untrusted (a crafted USB stick is enough): every offset is
//! bounds-checked, arithmetic is checked, and nothing here panics on any input
//! (see the fuzz-style tests at the bottom).

/// `$STANDARD_INFORMATION`
const ATTR_STANDARD_INFORMATION: u32 = 0x10;
/// `$ATTRIBUTE_LIST`
const ATTR_ATTRIBUTE_LIST: u32 = 0x20;
/// `$FILE_NAME`
const ATTR_FILE_NAME: u32 = 0x30;
/// `$DATA`
const ATTR_DATA: u32 = 0x80;
/// `$REPARSE_POINT`
const ATTR_REPARSE_POINT: u32 = 0xC0;
const ATTR_END: u32 = 0xFFFF_FFFF;

const RECORD_IN_USE: u16 = 0x0001;
const RECORD_IS_DIR: u16 = 0x0002;
/// Attribute flags: compressed or sparse means a `compressed_size` field is present.
const ATTR_FLAG_COMPRESSED: u16 = 0x0001;
const ATTR_FLAG_SPARSE: u16 = 0x8000;
/// `$FILE_NAME` namespace that holds only the 8.3 alias of another name.
const NAMESPACE_DOS: u8 = 2;
const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
const TAG_NAME_SURROGATE: u32 = 0x2000_0000;
/// Largest cluster NTFS supports.
const MAX_CLUSTER: u64 = 2 << 20;
/// NTFS allows 1024 hard links per file; more names means a corrupt record.
const MAX_NAMES: usize = 1024;

/// Volume layout from the boot sector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Geometry {
    pub cluster: u64,
    pub record: u64,
    pub mft_lcn: u64,
    /// 64-bit volume serial number.
    pub serial: u64,
}

impl Geometry {
    pub fn from_boot(b: &[u8]) -> Option<Self> {
        if b.get(3..11)? != b"NTFS    " {
            return None;
        }
        let sector = u64::from(u16_at(b, 0x0B)?);
        let spc = *b.get(0x0D)?;
        // Values above 0x80 encode clusters larger than 64 KiB as a negative power of two.
        let spc = if spc > 0x80 {
            1u64.checked_shl(256 - u32::from(spc))?
        } else {
            u64::from(spc)
        };
        let cluster = sector.checked_mul(spc)?;
        let mft_lcn = u64_at(b, 0x30)?;
        let cpr = *b.get(0x40)? as i8;
        let record = if cpr > 0 {
            cluster.checked_mul(cpr as u64)?
        } else {
            1u64.checked_shl(u32::from(cpr.unsigned_abs()))?
        };
        let sane = sector.is_power_of_two()
            && (512..=4096).contains(&sector)
            && cluster.is_power_of_two()
            && cluster <= MAX_CLUSTER
            && record.is_power_of_two()
            && (1024..=65536).contains(&record)
            // Records must not straddle cluster runs (true for every common format).
            && cluster % record == 0;
        sane.then_some(Self {
            cluster,
            record,
            mft_lcn,
            serial: u64_at(b, 0x48)?,
        })
    }
}

/// One `$FILE_NAME` of a file: its parent directory reference and name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Name {
    /// Parent file reference: record number in the low 48 bits, sequence in the high 16.
    pub parent: u64,
    pub name: Box<[u16]>,
}

/// Everything the scan needs about one file, merged from its base and extension records.
#[derive(Clone, Debug, Default)]
pub(super) struct MftNode {
    pub in_use: bool,
    pub is_dir: bool,
    pub seq: u16,
    /// `FILE_ATTRIBUTE_*` from `$STANDARD_INFORMATION`.
    pub attrs: u32,
    pub modified: i64,
    pub accessed: i64,
    pub logical: u64,
    pub allocated: u64,
    pub reparse_tag: u32,
    /// A mount-point reparse point that targets a volume (not a junction).
    pub volume_mount: bool,
    /// Hard-link names, without 8.3 aliases. At most [`MAX_NAMES`].
    pub names: Vec<Name>,
}

/// One parsed record (base or extension).
#[derive(Debug, Default)]
pub(super) struct Parsed {
    pub number: u64,
    /// Record number of the base record (equal to `number` for base records).
    pub base: u64,
    pub seq: u16,
    pub is_dir: bool,
    pub std_info: Option<(u32, i64, i64)>,
    /// (name, namespace)
    pub names: Vec<(Name, u8)>,
    /// (logical, allocated) from the unnamed `$DATA` attribute's first segment.
    pub data: Option<(u64, u64)>,
    pub reparse: Option<(u32, bool)>,
}

impl MftNode {
    /// A symlink, junction or mount point: never entered.
    pub fn is_link(&self) -> bool {
        self.reparse_tag & TAG_NAME_SURROGATE != 0
    }

    /// Merges a parsed record into this node. `base` is true for the base record.
    pub fn absorb(&mut self, p: Parsed, base: bool) {
        if base {
            self.in_use = true;
            self.is_dir = p.is_dir;
            self.seq = p.seq;
        }
        if let Some((attrs, modified, accessed)) = p.std_info {
            self.attrs = attrs;
            self.modified = modified;
            self.accessed = accessed;
        }
        if let Some((logical, allocated)) = p.data {
            self.logical = logical;
            self.allocated = allocated;
        }
        if let Some((tag, volume_mount)) = p.reparse {
            self.reparse_tag = tag;
            self.volume_mount = volume_mount;
        }
        // A DOS-namespace name is only the 8.3 alias of a long name in the same record.
        let room = MAX_NAMES.saturating_sub(self.names.len());
        self.names.extend(
            p.names
                .into_iter()
                .filter(|(_, ns)| *ns != NAMESPACE_DOS)
                .map(|(n, _)| n)
                .take(room),
        );
    }
}

/// Applies the update-sequence fixups that protect multi-sector records.
/// Returns false if the record is torn or malformed.
pub(super) fn fixup(rec: &mut [u8]) -> bool {
    let (Some(usa_off), Some(usa_count)) = (u16_at(rec, 4), u16_at(rec, 6)) else {
        return false;
    };
    let (usa_off, usa_count) = (usa_off as usize, usa_count as usize);
    if usa_count < 2 || usa_off + usa_count * 2 > rec.len() {
        return false;
    }
    let stride = rec.len() / (usa_count - 1);
    if stride < 512 {
        return false;
    }
    let check = [rec[usa_off], rec[usa_off + 1]];
    for i in 1..usa_count {
        let end = i * stride;
        if end > rec.len() || rec[end - 2..end] != check {
            return false;
        }
        rec[end - 2] = rec[usa_off + i * 2];
        rec[end - 1] = rec[usa_off + i * 2 + 1];
    }
    true
}

/// Parses one file record. Returns `None` for free, empty or damaged records.
pub(super) fn record(rec: &mut [u8], number: u64) -> Option<Parsed> {
    if rec.get(0..4)? != b"FILE" || !fixup(rec) {
        return None;
    }
    let rec = &*rec;
    let flags = u16_at(rec, 0x16)?;
    if flags & RECORD_IN_USE == 0 {
        return None;
    }
    let base = u64_at(rec, 0x20)? & 0x0000_FFFF_FFFF_FFFF;
    let mut p = Parsed {
        number,
        base: if base == 0 { number } else { base },
        seq: u16_at(rec, 0x10)?,
        is_dir: flags & RECORD_IS_DIR != 0,
        ..Parsed::default()
    };
    for attr in attributes(rec) {
        match attr.kind {
            ATTR_STANDARD_INFORMATION => {
                if let Some(v) = attr.resident_value() {
                    p.std_info = Some((u32_at(v, 32)?, i64_at(v, 8)?, i64_at(v, 24)?));
                }
            }
            ATTR_FILE_NAME => {
                if let Some(v) = attr.resident_value() {
                    let len = *v.get(64)? as usize;
                    let ns = *v.get(65)?;
                    let name = v
                        .get(66..66 + len * 2)?
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    p.names.push((
                        Name {
                            parent: u64_at(v, 0)?,
                            name,
                        },
                        ns,
                    ));
                }
            }
            ATTR_DATA if attr.name_len == 0 => {
                if let Some(v) = attr.resident_value() {
                    // Resident data lives inside the MFT record: no clusters allocated.
                    p.data = Some((v.len() as u64, 0));
                } else if let Some(nr) = attr.non_resident() {
                    if nr.lowest_vcn == 0 {
                        p.data = Some((nr.data_size, nr.on_disk));
                    }
                }
            }
            ATTR_REPARSE_POINT => {
                if let Some(v) = attr.resident_value() {
                    let tag = u32_at(v, 0)?;
                    p.reparse = Some((tag, tag == IO_REPARSE_TAG_MOUNT_POINT && targets_volume(v)));
                } else if attr.non_resident().is_some() {
                    // Large reparse buffers (rare) are non-resident; the tag is then
                    // also stored in the $FILE_NAME attribute's reparse field.
                    p.reparse = None;
                }
            }
            _ => {}
        }
    }
    Some(p)
}

/// Data runs and data size of `$MFT` from its own (first) record.
/// Returns `None` if the record is damaged or `$MFT` needs an attribute list.
pub(super) fn mft_runs(rec: &mut [u8]) -> Option<(Vec<(u64, u64)>, u64)> {
    if rec.get(0..4)? != b"FILE" || !fixup(rec) {
        return None;
    }
    let rec = &*rec;
    let mut found = None;
    for attr in attributes(rec) {
        if attr.kind == ATTR_ATTRIBUTE_LIST {
            return None;
        }
        if attr.kind == ATTR_DATA && attr.name_len == 0 {
            let nr = attr.non_resident()?;
            found = Some((runs(nr.runs)?, nr.data_size));
        }
    }
    found
}

/// Decodes a data-run list into (start cluster, length in clusters).
/// Sparse runs make it `None` (never the case for `$MFT`).
pub(super) fn runs(b: &[u8]) -> Option<Vec<(u64, u64)>> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut lcn: i64 = 0;
    while let Some(&h) = b.get(i) {
        if h == 0 {
            break;
        }
        let (len_size, off_size) = ((h & 0x0F) as usize, (h >> 4) as usize);
        i += 1;
        if len_size == 0 || len_size > 8 || off_size > 8 || off_size == 0 {
            return None;
        }
        let len = le_uint(b.get(i..i + len_size)?);
        i += len_size;
        let off = le_int(b.get(i..i + off_size)?);
        i += off_size;
        lcn = lcn.checked_add(off)?;
        out.push((u64::try_from(lcn).ok()?, len));
    }
    Some(out)
}

/// Does this mount-point reparse buffer target `\??\Volume{...}`?
fn targets_volume(v: &[u8]) -> bool {
    let (Some(off), Some(len)) = (u16_at(v, 8), u16_at(v, 10)) else {
        return false;
    };
    let start = 16 + off as usize;
    let Some(name) = v.get(start..start + len as usize) else {
        return false;
    };
    let prefix = r"\??\Volume{".encode_utf16().flat_map(u16::to_le_bytes);
    name.len() >= 22 && name.iter().copied().zip(prefix).all(|(a, b)| a == b)
}

struct Attr<'a> {
    kind: u32,
    name_len: u8,
    flags: u16,
    body: &'a [u8],
}

struct NonResident<'a> {
    lowest_vcn: u64,
    data_size: u64,
    /// Clusters actually in use: `compressed_size` for compressed/sparse streams,
    /// otherwise `allocated_size`.
    on_disk: u64,
    runs: &'a [u8],
}

impl<'a> Attr<'a> {
    fn resident_value(&self) -> Option<&'a [u8]> {
        if *self.body.get(8)? != 0 {
            return None;
        }
        let len = u32_at(self.body, 0x10)? as usize;
        let off = u16_at(self.body, 0x14)? as usize;
        self.body.get(off..off.checked_add(len)?)
    }

    fn non_resident(&self) -> Option<NonResident<'a>> {
        if *self.body.get(8)? == 0 {
            return None;
        }
        let allocated = u64_at(self.body, 0x28)?;
        let packed = self.flags & (ATTR_FLAG_COMPRESSED | ATTR_FLAG_SPARSE) != 0;
        let lowest_vcn = u64_at(self.body, 0x10)?;
        let on_disk = if packed && lowest_vcn == 0 {
            u64_at(self.body, 0x40)?
        } else {
            allocated
        };
        let runs_off = u16_at(self.body, 0x20)? as usize;
        Some(NonResident {
            lowest_vcn,
            data_size: u64_at(self.body, 0x30)?,
            on_disk,
            runs: self.body.get(runs_off..)?,
        })
    }
}

fn attributes(rec: &[u8]) -> impl Iterator<Item = Attr<'_>> {
    let used = u32_at(rec, 0x18).map_or(0, |u| (u as usize).min(rec.len()));
    let mut off = u16_at(rec, 0x14).map_or(usize::MAX, usize::from);
    std::iter::from_fn(move || {
        let kind = u32_at(rec, off)?;
        if kind == ATTR_END {
            return None;
        }
        let len = u32_at(rec, off.checked_add(4)?)? as usize;
        let end = off.checked_add(len)?;
        if len < 0x18 || end > used {
            return None;
        }
        let body = &rec[off..end];
        off = end;
        Some(Attr {
            kind,
            name_len: *body.get(9)?,
            flags: u16_at(body, 0x0C)?,
            body,
        })
    })
}

fn le_uint(b: &[u8]) -> u64 {
    b.iter().rev().fold(0, |acc, &x| (acc << 8) | u64::from(x))
}

fn le_int(b: &[u8]) -> i64 {
    let mut v = le_uint(b) as i64;
    let bits = b.len() * 8;
    if bits < 64 && b.last().is_some_and(|&x| x & 0x80 != 0) {
        v -= 1i64 << bits;
    }
    v
}

fn field<const N: usize>(b: &[u8], o: usize) -> Option<[u8; N]> {
    b.get(o..o.checked_add(N)?)?.try_into().ok()
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    field(b, o).map(u16::from_le_bytes)
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    field(b, o).map(u32::from_le_bytes)
}

fn u64_at(b: &[u8], o: usize) -> Option<u64> {
    field(b, o).map(u64::from_le_bytes)
}

fn i64_at(b: &[u8], o: usize) -> Option<i64> {
    u64_at(b, o).map(|v| v as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_data_runs() {
        // 0x21: 1-byte length, 2-byte offset. Second run's offset is negative.
        let b = [0x21, 0x10, 0x00, 0x01, 0x21, 0x08, 0x00, 0xFF, 0x00];
        assert_eq!(runs(&b), Some(vec![(0x100, 0x10), (0x0, 0x08)]));
    }

    #[test]
    fn boot_geometry() {
        let mut b = vec![0u8; 512];
        b[3..11].copy_from_slice(b"NTFS    ");
        b[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes());
        b[0x0D] = 8;
        b[0x30..0x38].copy_from_slice(&786_432u64.to_le_bytes());
        b[0x40] = 0xF6; // -10 → 1024-byte records
        b[0x48..0x50].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
        assert_eq!(
            Geometry::from_boot(&b),
            Some(Geometry {
                cluster: 4096,
                record: 1024,
                mft_lcn: 786_432,
                serial: 0x1122_3344_5566_7788,
            })
        );
        // Shift and size extremes are rejected, not panics.
        // 0x81/0xC0: cluster shift >= 64; 0x80: record shift 128; 0x7F: record too big.
        for (spc, cpr) in [
            (0x81u8, 0xF6u8),
            (0xC0, 0xF6),
            (8, 0x80),
            (8, 0x7F),
            (0, 0xF6),
        ] {
            let mut c = b.clone();
            c[0x0D] = spc;
            c[0x40] = cpr;
            assert_eq!(Geometry::from_boot(&c), None, "spc {spc:#x} cpr {cpr:#x}");
        }
    }

    /// A minimal valid in-use file record with a name, std info and a data run.
    fn sample_record() -> Vec<u8> {
        let mut r = vec![0u8; 1024];
        r[0..4].copy_from_slice(b"FILE");
        r[4..6].copy_from_slice(&0x30u16.to_le_bytes()); // update sequence array
        r[6..8].copy_from_slice(&3u16.to_le_bytes());
        r[0x30..0x32].copy_from_slice(&[0x01, 0x00]);
        r[510..512].copy_from_slice(&[0x01, 0x00]);
        r[1022..1024].copy_from_slice(&[0x01, 0x00]);
        r[0x10..0x12].copy_from_slice(&1u16.to_le_bytes()); // sequence
        r[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes()); // first attribute
        r[0x16..0x18].copy_from_slice(&RECORD_IN_USE.to_le_bytes());
        r[0x18..0x1C].copy_from_slice(&1024u32.to_le_bytes());
        let mut off = 0x38;
        let mut attr = |kind: u32, value: &[u8], r: &mut Vec<u8>| {
            let len = (0x18 + value.len() + 7) & !7;
            r[off..off + 4].copy_from_slice(&kind.to_le_bytes());
            r[off + 4..off + 8].copy_from_slice(&(len as u32).to_le_bytes());
            r[off + 0x10..off + 0x14].copy_from_slice(&(value.len() as u32).to_le_bytes());
            r[off + 0x14..off + 0x16].copy_from_slice(&0x18u16.to_le_bytes());
            r[off + 0x18..off + 0x18 + value.len()].copy_from_slice(value);
            off += len;
        };
        attr(ATTR_STANDARD_INFORMATION, &[0u8; 48], &mut r);
        let mut name = vec![0u8; 66];
        name[0..8].copy_from_slice(&((1u64 << 48) | 5).to_le_bytes());
        name[64] = 3;
        name[65] = 1;
        name.extend("a.b".encode_utf16().flat_map(u16::to_le_bytes));
        attr(ATTR_FILE_NAME, &name, &mut r);
        attr(ATTR_DATA, b"hello", &mut r);
        r[off..off + 4].copy_from_slice(&ATTR_END.to_le_bytes());
        r
    }

    #[test]
    fn parses_a_sample_record() {
        let p = record(&mut sample_record(), 40).expect("valid record");
        assert_eq!(p.names.len(), 1);
        assert_eq!(p.names[0].0.parent & 0xFFFF_FFFF_FFFF, 5);
        assert_eq!(p.data, Some((5, 0)));
    }

    /// Deterministic random mutations of a valid record, boot sector and run list:
    /// parsing must never panic, whatever the bytes.
    #[test]
    fn fuzzish_never_panics() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let good = sample_record();
        for _ in 0..20_000 {
            let mut r = good.clone();
            for _ in 0..(next() % 8 + 1) {
                let i = (next() % r.len() as u64) as usize;
                r[i] = next() as u8;
            }
            if next() % 4 == 0 {
                // Lie about field sizes and offsets specifically.
                let at = [0x14usize, 0x18, 0x38 + 4, 0x38 + 0x10, 0x38 + 0x14, 0x80];
                let i = at[(next() % at.len() as u64) as usize];
                r[i..i + 2].copy_from_slice(&(next() as u16).to_le_bytes());
            }
            let _ = record(&mut r, next() % 1000);
            let _ = mft_runs(&mut r.clone());
        }
        for _ in 0..20_000 {
            let mut b = vec![0u8; 512];
            b[3..11].copy_from_slice(b"NTFS    ");
            for i in [0x0B, 0x0C, 0x0D, 0x30, 0x31, 0x37, 0x40, 0x48] {
                b[i] = next() as u8;
            }
            let _ = Geometry::from_boot(&b);
            let runs_len = (next() % 40) as usize;
            let rl: Vec<u8> = (0..runs_len).map(|_| next() as u8).collect();
            let _ = runs(&rl);
        }
    }

    #[test]
    fn fixup_restores_sector_tails() {
        let mut rec = vec![0u8; 1024];
        rec[4..6].copy_from_slice(&0x30u16.to_le_bytes());
        rec[6..8].copy_from_slice(&3u16.to_le_bytes());
        rec[0x30..0x36].copy_from_slice(&[0xAB, 0xCD, 1, 2, 3, 4]);
        rec[510..512].copy_from_slice(&[0xAB, 0xCD]);
        rec[1022..1024].copy_from_slice(&[0xAB, 0xCD]);
        assert!(fixup(&mut rec));
        assert_eq!(&rec[510..512], &[1, 2]);
        assert_eq!(&rec[1022..1024], &[3, 4]);
        rec[510] = 0;
        rec[1022..1024].copy_from_slice(&[0xAB, 0xCD]);
        assert!(!fixup(&mut rec));
    }
}
