//! Checks the built helper's link-time hardening (see build.rs), by reading the PE:
//! - load config `DependentLoadFlags == LOAD_LIBRARY_SEARCH_SYSTEM32`
//!   (`dumpbin /loadconfig`: "Dependent Load Flag 0x800");
//! - an embedded `RT_MANIFEST` asking for `requireAdministrator` with no
//!   `<dependency>` (so `.local` folders and external manifests are ignored);
//! - every DLL import (normal and delay-load) is an `api-ms-win-*` / `ext-ms-win-*`
//!   API set or a DLL that exists in System32; in particular no `VCRUNTIME*`/`MSVCP*`.
//!
//! Checked on every helper exe that exists (see [`helpers`]): this build's, the
//! other profile's in `target/` (debug and release), any bundled sidecar in
//! `app/src-tauri/binaries/`, and `LUSTRO_HELPER_EXE` if set. The shipped exe is a release
//! build, so `cargo test --release` (or a prior release build) covers it too.
#![cfg(all(windows, target_env = "msvc"))]

use std::path::{Path, PathBuf};

const LOAD_LIBRARY_SEARCH_SYSTEM32: u16 = 0x800;
const DIR_IMPORT: usize = 1;
const DIR_RESOURCE: usize = 2;
const DIR_LOAD_CONFIG: usize = 10;
const DIR_DELAY_IMPORT: usize = 13;
const RT_MANIFEST: u32 = 24;

struct Pe {
    bytes: Vec<u8>,
    pe32_plus: bool,
    dirs: usize,
    sections: Vec<(usize, usize, usize)>, // (rva, size, file offset)
}

/// Every helper exe to check. Always includes this build's.
fn helpers() -> Vec<PathBuf> {
    let this = PathBuf::from(env!("CARGO_BIN_EXE_lustro-scan-helper"));
    let mut out = vec![this.clone()];
    // The same exe in the other profile's folder (target/debug <-> target/release).
    if let (Some(profile_dir), Some(name)) = (this.parent(), this.file_name()) {
        if let (Some(target), Some(profile)) = (profile_dir.parent(), profile_dir.file_name()) {
            let other = if profile == "release" {
                "debug"
            } else {
                "release"
            };
            out.push(target.join(other).join(name));
        }
    }
    // Sidecars bundled by the app (named lustro-scan-helper-<target triple>.exe).
    let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src-tauri/binaries");
    if let Ok(dir) = std::fs::read_dir(bundled) {
        for e in dir.flatten() {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            if name.starts_with("lustro-scan-helper") && name.ends_with(".exe") {
                out.push(e.path());
            }
        }
    }
    if let Some(p) = std::env::var_os("LUSTRO_HELPER_EXE") {
        out.push(PathBuf::from(p));
    }
    out.retain(|p| p.is_file());
    out.dedup();
    out
}

/// Runs `check` on every helper exe; reports how many were checked (no paths).
fn for_each_helper(label: &str, check: impl Fn(&Pe)) {
    let all = helpers();
    for path in &all {
        check(&Pe::load(path));
    }
    println!("{label}: checked {} helper exe(s)", all.len());
}

impl Pe {
    fn load(path: &Path) -> Self {
        let bytes = std::fs::read(path).unwrap();
        let nt = u32_at(&bytes, 0x3C) as usize;
        assert_eq!(&bytes[nt..nt + 4], b"PE\0\0");
        let coff = nt + 4;
        let count = u16_at(&bytes, coff + 2) as usize;
        let opt_size = u16_at(&bytes, coff + 16) as usize;
        let opt = coff + 20;
        let pe32_plus = match u16_at(&bytes, opt) {
            0x20B => true,
            0x10B => false,
            m => panic!("unknown optional header magic {m:#x}"),
        };
        let sections = (0..count)
            .map(|i| opt + opt_size + i * 40)
            .map(|s| {
                (
                    u32_at(&bytes, s + 12) as usize,
                    (u32_at(&bytes, s + 8).max(u32_at(&bytes, s + 16))) as usize,
                    u32_at(&bytes, s + 20) as usize,
                )
            })
            .collect();
        Self {
            dirs: opt + if pe32_plus { 112 } else { 96 },
            bytes,
            pe32_plus,
            sections,
        }
    }

    fn dir(&self, i: usize) -> (usize, usize) {
        let d = self.dirs + i * 8;
        (
            u32_at(&self.bytes, d) as usize,
            u32_at(&self.bytes, d + 4) as usize,
        )
    }

    fn off(&self, rva: usize) -> usize {
        self.sections
            .iter()
            .find(|(va, size, _)| (*va..va + size).contains(&rva))
            .map(|(va, _, raw)| rva - va + raw)
            .expect("rva outside sections")
    }

    fn cstr(&self, rva: usize) -> String {
        let o = self.off(rva);
        let end = self.bytes[o..].iter().position(|&b| b == 0).unwrap();
        String::from_utf8_lossy(&self.bytes[o..o + end]).into_owned()
    }

    fn imports(&self) -> Vec<String> {
        let mut out = Vec::new();
        let (rva, _) = self.dir(DIR_IMPORT);
        if rva != 0 {
            let mut d = self.off(rva);
            while u32_at(&self.bytes, d + 12) != 0 {
                out.push(self.cstr(u32_at(&self.bytes, d + 12) as usize));
                d += 20;
            }
        }
        let (rva, _) = self.dir(DIR_DELAY_IMPORT);
        if rva != 0 {
            let mut d = self.off(rva);
            while u32_at(&self.bytes, d + 4) != 0 {
                out.push(self.cstr(u32_at(&self.bytes, d + 4) as usize));
                d += 32;
            }
        }
        out
    }

    /// Bytes of the first resource of type `ty` (any name, any language).
    fn resource(&self, ty: u32) -> Option<Vec<u8>> {
        let (rva, _) = self.dir(DIR_RESOURCE);
        if rva == 0 {
            return None;
        }
        let base = self.off(rva);
        let b = &self.bytes;
        // Walks type -> name -> language, taking the first entry at the lower levels.
        let entries = |dir: usize| {
            let n = u16_at(b, dir + 12) as usize + u16_at(b, dir + 14) as usize;
            (0..n).map(move |i| (u32_at(b, dir + 16 + i * 8), u32_at(b, dir + 20 + i * 8)))
        };
        let (_, to_names) = entries(base).find(|(id, _)| *id == ty)?;
        let names = base + (to_names & 0x7FFF_FFFF) as usize;
        let (_, to_langs) = entries(names).next()?;
        let langs = base + (to_langs & 0x7FFF_FFFF) as usize;
        let (_, to_data) = entries(langs).next()?;
        let data = base + to_data as usize;
        let (data_rva, size) = (u32_at(b, data) as usize, u32_at(b, data + 4) as usize);
        let o = self.off(data_rva);
        Some(b[o..o + size].to_vec())
    }
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

#[test]
fn loads_dlls_from_system32_only() {
    for_each_helper("load flags", check_load_flags);
}

fn check_load_flags(pe: &Pe) {
    let (rva, _) = pe.dir(DIR_LOAD_CONFIG);
    assert_ne!(rva, 0, "no load-config directory");
    let o = pe.off(rva);
    // IMAGE_LOAD_CONFIG_DIRECTORY{64,32}.DependentLoadFlags
    let field = if pe.pe32_plus { 0x4E } else { 0x36 };
    assert!(
        u32_at(&pe.bytes, o) as usize >= field + 2,
        "load config too old"
    );
    assert_eq!(u16_at(&pe.bytes, o + field), LOAD_LIBRARY_SEARCH_SYSTEM32);
}

#[test]
fn embeds_a_dependency_free_admin_manifest() {
    for_each_helper("manifest", |pe| {
        let manifest = pe.resource(RT_MANIFEST).expect("no embedded manifest");
        let text = String::from_utf8_lossy(&manifest);
        assert!(text.contains(r#"level="requireAdministrator""#));
        assert!(!text.contains("<dependency"), "manifest has dependencies");
    });
}

#[test]
fn imports_only_system_dlls() {
    for_each_helper("imports", check_imports);
}

fn check_imports(pe: &Pe) {
    let system32 = PathBuf::from(std::env::var("SystemRoot").unwrap()).join("System32");
    let imports = pe.imports();
    assert!(!imports.is_empty());
    for dll in &imports {
        let lower = dll.to_ascii_lowercase();
        assert!(
            !lower.starts_with("vcruntime") && !lower.starts_with("msvcp"),
            "imports {dll}"
        );
        let api_set = lower.starts_with("api-ms-win-") || lower.starts_with("ext-ms-win-");
        assert!(
            api_set || system32.join(dll).is_file(),
            "{dll} is not a System32 DLL"
        );
    }
    println!("imports: {}", imports.join(", "));
}
