//! Integration verification for issue #1258:
//! - Bounded transitive dependency closure discovery within configured roots
//! - Full global scope built before relocation (transitive & sibling cross-refs)
//! - Deterministic cycle detection, duplicate identity dedup, conflict tracking,
//!   visibility filtering, and explicit errors on missing/unsupported/corrupt inputs
//!   with untouched original `MachineState` on failure.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct TestTempDir(PathBuf);

impl TestTempDir {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "fission-dynlink-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

use fission_emulator::os::linux::dynlink::{DynlinkMode, prepare_dynlink_with_search_paths};
use fission_emulator::pcode::page_map::prot;
use fission_emulator::pcode::state::MachineState;
use fission_loader::loader::LoadedBinary;

const EM_X86_64: u16 = 62;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;

const DT_NULL: i64 = 0;
const DT_NEEDED: i64 = 1;
const DT_HASH: i64 = 4;
const DT_STRTAB: i64 = 5;
const DT_SYMTAB: i64 = 6;
const DT_RELA: i64 = 7;
const DT_RELASZ: i64 = 8;
const DT_RELAENT: i64 = 9;
const DT_STRSZ: i64 = 10;
const DT_SYMENT: i64 = 11;
const DT_SONAME: i64 = 14;
const DT_FLAGS: i64 = 30;
const DF_BIND_NOW: u64 = 0x8;
const DT_VERNEED: i64 = 0x6fff_fffe;

const R_X86_64_64: u32 = 1;
const R_X86_64_JUMP_SLOT: u32 = 7;
const R_X86_64_IRELATIVE: u32 = 37;

const STB_GLOBAL: u8 = 1;
const STB_WEAK: u8 = 2;
const STT_FUNC: u8 = 2;
const STV_DEFAULT: u8 = 0;
const STV_HIDDEN: u8 = 2;
const SHN_UNDEF: u16 = 0;

#[derive(Clone)]
struct SymSpec {
    name: &'static str,
    bind: u8,
    vis: u8,
    shndx: u16,
    value: u64,
}

#[derive(Clone)]
struct RelaSpec {
    offset: u64,
    sym_idx: usize,
    rtype: u32,
    addend: i64,
}

struct ElfSpec {
    e_type: u16,
    link_base: u64,
    soname: Option<&'static str>,
    needed: Vec<&'static str>,
    syms: Vec<SymSpec>,
    relas: Vec<RelaSpec>,
    extra_dyn_tags: Vec<(i64, u64)>,
}

fn build_elf64(spec: &ElfSpec) -> Vec<u8> {
    let mut img = vec![0u8; 0x2000];
    let base = spec.link_base;

    // 1. String table at 0x0200
    let strtab_off = 0x0200usize;
    let mut strtab = vec![0u8];
    let mut add_str = |s: &str| -> u32 {
        let pos = strtab.len() as u32;
        strtab.extend_from_slice(s.as_bytes());
        strtab.push(0);
        pos
    };

    let soname_str = spec.soname.map(&mut add_str);
    let needed_strs: Vec<u32> = spec.needed.iter().map(|s| add_str(s)).collect();
    let sym_name_offs: Vec<u32> = spec.syms.iter().map(|s| add_str(s.name)).collect();
    assert!(strtab.len() <= 0x0300);
    img[strtab_off..strtab_off + strtab.len()].copy_from_slice(&strtab);

    // 2. .dynsym at 0x0500 (sym 0 = STN_UNDEF)
    let symtab_off = 0x0500usize;
    let sym_count = 1 + spec.syms.len();
    for (idx, sym) in spec.syms.iter().enumerate() {
        let at = symtab_off + (idx + 1) * 24;
        let name_off = sym_name_offs[idx];
        img[at..at + 4].copy_from_slice(&name_off.to_le_bytes());
        img[at + 4] = (sym.bind << 4) | STT_FUNC;
        img[at + 5] = sym.vis;
        img[at + 6..at + 8].copy_from_slice(&sym.shndx.to_le_bytes());
        img[at + 8..at + 16].copy_from_slice(&sym.value.to_le_bytes());
        img[at + 16..at + 24].copy_from_slice(&8u64.to_le_bytes());
    }

    // 3. .hash (SysV DT_HASH) at 0x0700: nbuckets=1, nchain=sym_count
    let hash_off = 0x0700usize;
    img[hash_off..hash_off + 4].copy_from_slice(&1u32.to_le_bytes());
    img[hash_off + 4..hash_off + 8].copy_from_slice(&(sym_count as u32).to_le_bytes());

    // 4. .rela.dyn at 0x0800
    let rela_off = 0x0800usize;
    for (idx, r) in spec.relas.iter().enumerate() {
        let at = rela_off + idx * 24;
        let r_info = ((r.sym_idx as u64) << 32) | u64::from(r.rtype);
        img[at..at + 8].copy_from_slice(&r.offset.to_le_bytes());
        img[at + 8..at + 16].copy_from_slice(&r_info.to_le_bytes());
        img[at + 16..at + 24].copy_from_slice(&r.addend.to_le_bytes());
    }
    let rela_sz = (spec.relas.len() * 24) as u64;

    // 5. .dynamic at 0x0a00
    let dyn_off = 0x0a00usize;
    let mut tags: Vec<(i64, u64)> = Vec::new();
    if let Some(off) = soname_str {
        tags.push((DT_SONAME, u64::from(off)));
    }
    for off in needed_strs {
        tags.push((DT_NEEDED, u64::from(off)));
    }
    tags.push((DT_FLAGS, DF_BIND_NOW));
    tags.push((DT_STRTAB, base + strtab_off as u64));
    tags.push((DT_STRSZ, strtab.len() as u64));
    tags.push((DT_SYMTAB, base + symtab_off as u64));
    tags.push((DT_SYMENT, 24));
    tags.push((DT_HASH, base + hash_off as u64));
    if rela_sz > 0 {
        tags.push((DT_RELA, base + rela_off as u64));
        tags.push((DT_RELASZ, rela_sz));
        tags.push((DT_RELAENT, 24));
    }
    tags.extend_from_slice(&spec.extra_dyn_tags);
    tags.push((DT_NULL, 0));
    let dyn_sz = (tags.len() * 16) as u64;
    for (idx, (t, v)) in tags.iter().enumerate() {
        let at = dyn_off + idx * 16;
        img[at..at + 8].copy_from_slice(&t.to_le_bytes());
        img[at + 8..at + 16].copy_from_slice(&v.to_le_bytes());
    }

    // Minimal ret opcode at 0x1000 so entry is plausible code.
    img[0x1000] = 0xC3;

    // ELF64 header
    img[0..4].copy_from_slice(b"\x7fELF");
    img[4] = 2; // ELFCLASS64
    img[5] = 1; // ELFDATA2LSB
    img[6] = 1; // EV_CURRENT
    img[16..18].copy_from_slice(&spec.e_type.to_le_bytes());
    img[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
    img[20..24].copy_from_slice(&1u32.to_le_bytes());
    img[24..32].copy_from_slice(&(base + 0x1000).to_le_bytes()); // e_entry
    img[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
    img[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize
    img[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
    img[56..58].copy_from_slice(&2u16.to_le_bytes()); // e_phnum

    // PHDR 0: PT_LOAD (RX|W) covering 0..0x2000
    let ph0 = 64usize;
    img[ph0..ph0 + 4].copy_from_slice(&PT_LOAD.to_le_bytes());
    img[ph0 + 4..ph0 + 8].copy_from_slice(&7u32.to_le_bytes());
    img[ph0 + 8..ph0 + 16].copy_from_slice(&0u64.to_le_bytes());
    img[ph0 + 16..ph0 + 24].copy_from_slice(&base.to_le_bytes());
    img[ph0 + 24..ph0 + 32].copy_from_slice(&base.to_le_bytes());
    img[ph0 + 32..ph0 + 40].copy_from_slice(&0x2000u64.to_le_bytes());
    img[ph0 + 40..ph0 + 48].copy_from_slice(&0x2000u64.to_le_bytes());
    img[ph0 + 48..ph0 + 56].copy_from_slice(&0x1000u64.to_le_bytes());

    // PHDR 1: PT_DYNAMIC
    let ph1 = ph0 + 56;
    img[ph1..ph1 + 4].copy_from_slice(&PT_DYNAMIC.to_le_bytes());
    img[ph1 + 4..ph1 + 8].copy_from_slice(&6u32.to_le_bytes());
    img[ph1 + 8..ph1 + 16].copy_from_slice(&(dyn_off as u64).to_le_bytes());
    img[ph1 + 16..ph1 + 24].copy_from_slice(&(base + dyn_off as u64).to_le_bytes());
    img[ph1 + 24..ph1 + 32].copy_from_slice(&(base + dyn_off as u64).to_le_bytes());
    img[ph1 + 32..ph1 + 40].copy_from_slice(&dyn_sz.to_le_bytes());
    img[ph1 + 40..ph1 + 48].copy_from_slice(&dyn_sz.to_le_bytes());
    img[ph1 + 48..ph1 + 56].copy_from_slice(&8u64.to_le_bytes());

    img
}

fn map_main_and_binary(bytes: Vec<u8>, base: u64) -> (MachineState, LoadedBinary) {
    let mut state = MachineState::new();
    let len = bytes.len() as u64;
    state.load_image_range(base, len, &bytes).unwrap();
    state.page_map.map_region(
        base,
        len,
        prot::VALID | prot::READ | prot::WRITE | prot::EXEC,
        false,
    );
    let bin = LoadedBinary::from_bytes(bytes, "main.elf".into()).unwrap();
    (state, bin)
}

fn read_u64(state: &mut MachineState, addr: u64) -> u64 {
    let bytes = state.read_space(state.ram_space(), addr, 8).unwrap();
    u64::from_le_bytes(bytes.try_into().unwrap())
}

#[test]
fn transitive_closure_main_to_a_to_b_relocates_both_after_full_discovery() {
    let dir = TestTempDir::new("transitive");
    let lib_a = build_elf64(&ElfSpec {
        e_type: ET_DYN,
        link_base: 0,
        soname: Some("liba.so"),
        needed: vec!["libb.so"],
        syms: vec![
            SymSpec {
                name: "fn_a",
                bind: STB_GLOBAL,
                vis: STV_DEFAULT,
                shndx: 1,
                value: 0x1000,
            },
            SymSpec {
                name: "fn_b",
                bind: STB_GLOBAL,
                vis: STV_DEFAULT,
                shndx: SHN_UNDEF,
                value: 0,
            },
        ],
        relas: vec![RelaSpec {
            offset: 0x1100,
            sym_idx: 2, // fn_b in libb.so
            rtype: R_X86_64_JUMP_SLOT,
            addend: 0,
        }],
        extra_dyn_tags: vec![],
    });
    let lib_b = build_elf64(&ElfSpec {
        e_type: ET_DYN,
        link_base: 0,
        soname: Some("libb.so"),
        needed: vec![],
        syms: vec![SymSpec {
            name: "fn_b",
            bind: STB_GLOBAL,
            vis: STV_DEFAULT,
            shndx: 1,
            value: 0x1020,
        }],
        relas: vec![],
        extra_dyn_tags: vec![],
    });
    std::fs::write(dir.path().join("liba.so"), &lib_a).unwrap();
    std::fs::write(dir.path().join("libb.so"), &lib_b).unwrap();

    let main_base = 0x40_0000u64;
    let main_bytes = build_elf64(&ElfSpec {
        e_type: ET_EXEC,
        link_base: main_base,
        soname: None,
        needed: vec!["liba.so"],
        syms: vec![SymSpec {
            name: "fn_a",
            bind: STB_GLOBAL,
            vis: STV_DEFAULT,
            shndx: SHN_UNDEF,
            value: 0,
        }],
        relas: vec![RelaSpec {
            offset: main_base + 0x1100,
            sym_idx: 1,
            rtype: R_X86_64_JUMP_SLOT,
            addend: 0,
        }],
        extra_dyn_tags: vec![],
    });

    let (mut state, bin) = map_main_and_binary(main_bytes, main_base);
    let info =
        prepare_dynlink_with_search_paths(&mut state, &bin, &[dir.path().to_path_buf()]).unwrap();
    assert_eq!(info.mode, DynlinkMode::SharedLibs);
    assert_eq!(info.loaded_lib_identities.len(), 2);
    assert_eq!(info.loaded_lib_identities[0].soname, "liba.so");
    assert_eq!(info.loaded_lib_identities[1].soname, "libb.so");

    let base_a = info.loaded_lib_identities[0].base;
    let base_b = info.loaded_lib_identities[1].base;
    assert_eq!(read_u64(&mut state, main_base + 0x1100), base_a + 0x1000);
    assert_eq!(read_u64(&mut state, base_a + 0x1100), base_b + 0x1020);
}

#[test]
fn sibling_cross_references_and_cycles_resolve_cleanly_without_duplicate_loads() {
    let dir = TestTempDir::new("cycle");
    let lib_a = build_elf64(&ElfSpec {
        e_type: ET_DYN,
        link_base: 0,
        soname: Some("liba.so"),
        needed: vec!["libb.so"],
        syms: vec![
            SymSpec {
                name: "sym_a",
                bind: STB_GLOBAL,
                vis: STV_DEFAULT,
                shndx: 1,
                value: 0x1010,
            },
            SymSpec {
                name: "sym_b",
                bind: STB_GLOBAL,
                vis: STV_DEFAULT,
                shndx: SHN_UNDEF,
                value: 0,
            },
        ],
        relas: vec![RelaSpec {
            offset: 0x1100,
            sym_idx: 2,
            rtype: R_X86_64_64,
            addend: 4,
        }],
        extra_dyn_tags: vec![],
    });
    // Cycle: libb depends on liba!
    let lib_b = build_elf64(&ElfSpec {
        e_type: ET_DYN,
        link_base: 0,
        soname: Some("libb.so"),
        needed: vec!["liba.so"],
        syms: vec![
            SymSpec {
                name: "sym_b",
                bind: STB_GLOBAL,
                vis: STV_DEFAULT,
                shndx: 1,
                value: 0x1020,
            },
            SymSpec {
                name: "sym_a",
                bind: STB_GLOBAL,
                vis: STV_DEFAULT,
                shndx: SHN_UNDEF,
                value: 0,
            },
        ],
        relas: vec![RelaSpec {
            offset: 0x1108,
            sym_idx: 2,
            rtype: R_X86_64_64,
            addend: 8,
        }],
        extra_dyn_tags: vec![],
    });
    std::fs::write(dir.path().join("liba.so"), &lib_a).unwrap();
    std::fs::write(dir.path().join("libb.so"), &lib_b).unwrap();

    let main_base = 0x40_0000u64;
    let main_bytes = build_elf64(&ElfSpec {
        e_type: ET_EXEC,
        link_base: main_base,
        soname: None,
        needed: vec!["liba.so", "libb.so"],
        syms: vec![],
        relas: vec![],
        extra_dyn_tags: vec![],
    });

    let (mut state, bin) = map_main_and_binary(main_bytes, main_base);
    let info =
        prepare_dynlink_with_search_paths(&mut state, &bin, &[dir.path().to_path_buf()]).unwrap();
    assert_eq!(info.loaded_libs.len(), 2);
    assert!(
        !info.cycles_detected.is_empty(),
        "expected libb.so -> liba.so cycle recorded: {info:?}"
    );
    let base_a = info.loaded_lib_identities[0].base;
    let base_b = info.loaded_lib_identities[1].base;
    assert_eq!(read_u64(&mut state, base_a + 0x1100), base_b + 0x1020 + 4);
    assert_eq!(read_u64(&mut state, base_b + 0x1108), base_a + 0x1010 + 8);
}

#[test]
fn symbol_conflicts_weak_override_and_hidden_visibility_follow_scope_rules() {
    let dir = TestTempDir::new("conflicts");
    let lib_a = build_elf64(&ElfSpec {
        e_type: ET_DYN,
        link_base: 0,
        soname: Some("liba.so"),
        needed: vec![],
        syms: vec![
            SymSpec {
                name: "dup_strong",
                bind: STB_GLOBAL,
                vis: STV_DEFAULT,
                shndx: 1,
                value: 0x1010,
            },
            SymSpec {
                name: "overridable_weak",
                bind: STB_WEAK,
                vis: STV_DEFAULT,
                shndx: 1,
                value: 0x1020,
            },
            SymSpec {
                name: "hidden_only",
                bind: STB_GLOBAL,
                vis: STV_HIDDEN,
                shndx: 1,
                value: 0x1030,
            },
        ],
        relas: vec![],
        extra_dyn_tags: vec![],
    });
    let lib_b = build_elf64(&ElfSpec {
        e_type: ET_DYN,
        link_base: 0,
        soname: Some("libb.so"),
        needed: vec![],
        syms: vec![
            SymSpec {
                name: "dup_strong",
                bind: STB_GLOBAL,
                vis: STV_DEFAULT,
                shndx: 1,
                value: 0x1040,
            },
            SymSpec {
                name: "overridable_weak",
                bind: STB_GLOBAL,
                vis: STV_DEFAULT,
                shndx: 1,
                value: 0x1050,
            },
        ],
        relas: vec![],
        extra_dyn_tags: vec![],
    });
    std::fs::write(dir.path().join("liba.so"), &lib_a).unwrap();
    std::fs::write(dir.path().join("libb.so"), &lib_b).unwrap();

    let main_base = 0x40_0000u64;
    let main_bytes = build_elf64(&ElfSpec {
        e_type: ET_EXEC,
        link_base: main_base,
        soname: None,
        needed: vec!["liba.so", "libb.so"],
        syms: vec![
            SymSpec {
                name: "dup_strong",
                bind: STB_GLOBAL,
                vis: STV_DEFAULT,
                shndx: SHN_UNDEF,
                value: 0,
            },
            SymSpec {
                name: "overridable_weak",
                bind: STB_GLOBAL,
                vis: STV_DEFAULT,
                shndx: SHN_UNDEF,
                value: 0,
            },
        ],
        relas: vec![
            RelaSpec {
                offset: main_base + 0x1100,
                sym_idx: 1,
                rtype: R_X86_64_JUMP_SLOT,
                addend: 0,
            },
            RelaSpec {
                offset: main_base + 0x1108,
                sym_idx: 2,
                rtype: R_X86_64_JUMP_SLOT,
                addend: 0,
            },
        ],
        extra_dyn_tags: vec![],
    });

    let (mut state, bin) = map_main_and_binary(main_bytes, main_base);
    let info =
        prepare_dynlink_with_search_paths(&mut state, &bin, &[dir.path().to_path_buf()]).unwrap();
    let base_a = info.loaded_lib_identities[0].base;
    let base_b = info.loaded_lib_identities[1].base;

    // First strong wins (`liba.so`), strong in `libb.so` overrides weak in `liba.so`,
    // `hidden_only` is omitted from global scope, and strong collision is recorded.
    assert_eq!(read_u64(&mut state, main_base + 0x1100), base_a + 0x1010);
    assert_eq!(read_u64(&mut state, main_base + 0x1108), base_b + 0x1050);
    assert!(!info.global_symbols.contains_key("hidden_only"));
    assert_eq!(info.symbol_conflicts.len(), 1);
    assert_eq!(info.symbol_conflicts[0].symbol, "dup_strong");
    assert_eq!(info.symbol_conflicts[0].chosen_module, "liba.so");
    assert_eq!(info.symbol_conflicts[0].shadowed_module, "libb.so");
}

#[test]
fn missing_transitive_dep_bad_reloc_versioning_and_traversal_fail_cleanly_preserving_state() {
    let dir = TestTempDir::new("failures");

    // 1. Missing transitive dependency: main -> liba -> missing_b.so
    let lib_a_missing_b = build_elf64(&ElfSpec {
        e_type: ET_DYN,
        link_base: 0,
        soname: Some("liba.so"),
        needed: vec!["missing_b.so"],
        syms: vec![],
        relas: vec![],
        extra_dyn_tags: vec![],
    });
    std::fs::write(dir.path().join("liba.so"), &lib_a_missing_b).unwrap();

    let main_base = 0x40_0000u64;
    let main_missing = build_elf64(&ElfSpec {
        e_type: ET_EXEC,
        link_base: main_base,
        soname: None,
        needed: vec!["liba.so"],
        syms: vec![],
        relas: vec![],
        extra_dyn_tags: vec![],
    });
    let (mut state, bin) = map_main_and_binary(main_missing, main_base);
    state
        .write_space(
            state.ram_space(),
            main_base + 0x1100,
            &0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes(),
        )
        .unwrap();
    let err = prepare_dynlink_with_search_paths(&mut state, &bin, &[dir.path().to_path_buf()])
        .expect_err("missing transitive dep must fail");
    assert!(format!("{err:#}").contains("missing_b.so"), "{err:#}");
    assert_eq!(
        read_u64(&mut state, main_base + 0x1100),
        0xDEAD_BEEF_CAFE_BABE
    );

    // 2. Unsupported R_X86_64_IRELATIVE relocation fails without committing state.
    let lib_irel = build_elf64(&ElfSpec {
        e_type: ET_DYN,
        link_base: 0,
        soname: Some("libirel.so"),
        needed: vec![],
        syms: vec![],
        relas: vec![RelaSpec {
            offset: 0x1100,
            sym_idx: 0,
            rtype: R_X86_64_IRELATIVE,
            addend: 0x1000,
        }],
        extra_dyn_tags: vec![],
    });
    std::fs::write(dir.path().join("libirel.so"), &lib_irel).unwrap();
    let main_irel = build_elf64(&ElfSpec {
        e_type: ET_EXEC,
        link_base: main_base,
        soname: None,
        needed: vec!["libirel.so"],
        syms: vec![],
        relas: vec![],
        extra_dyn_tags: vec![],
    });
    let (mut state2, bin2) = map_main_and_binary(main_irel, main_base);
    let err = prepare_dynlink_with_search_paths(&mut state2, &bin2, &[dir.path().to_path_buf()])
        .expect_err("IRELATIVE must fail");
    assert!(format!("{err:#}").contains("IRELATIVE"), "{err:#}");
    assert_eq!(
        state2.page_map.mappings().len(),
        state.page_map.mappings().len()
    );
    assert!(!state2.page_map.is_mapped(0x0000_7F00_0000_0000));

    // 3. Unsupported GNU symbol versioning (DT_VERNEED) fails explicitly.
    let lib_ver = build_elf64(&ElfSpec {
        e_type: ET_DYN,
        link_base: 0,
        soname: Some("libver.so"),
        needed: vec![],
        syms: vec![],
        relas: vec![],
        extra_dyn_tags: vec![(DT_VERNEED, 0x1200)],
    });
    std::fs::write(dir.path().join("libver.so"), &lib_ver).unwrap();
    let main_ver = build_elf64(&ElfSpec {
        e_type: ET_EXEC,
        link_base: main_base,
        soname: None,
        needed: vec!["libver.so"],
        syms: vec![],
        relas: vec![],
        extra_dyn_tags: vec![],
    });
    let (mut state3, bin3) = map_main_and_binary(main_ver, main_base);
    let err = prepare_dynlink_with_search_paths(&mut state3, &bin3, &[dir.path().to_path_buf()])
        .expect_err("DT_VERNEED must fail");
    assert!(format!("{err:#}").contains("symbol versioning"), "{err:#}");

    // 4. Unsafe path-traversal soname (`../escape.so`) is rejected.
    let main_trav = build_elf64(&ElfSpec {
        e_type: ET_EXEC,
        link_base: main_base,
        soname: None,
        needed: vec!["../escape.so"],
        syms: vec![],
        relas: vec![],
        extra_dyn_tags: vec![],
    });
    let (mut state4, bin4) = map_main_and_binary(main_trav, main_base);
    let err = prepare_dynlink_with_search_paths(&mut state4, &bin4, &[dir.path().to_path_buf()])
        .expect_err("path traversal soname must fail");
    assert!(format!("{err:#}").contains("unsafe DT_NEEDED"), "{err:#}");
}

#[test]
fn pinned_prelinked_fixture_requires_configured_root_and_records_identity() {
    let testdata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata");
    let main_path = testdata.join("x64_needs_prelinked.elf");
    let binary = LoadedBinary::from_file(&main_path).unwrap();

    // Missing from empty search path -> explicit failure, state preserved.
    let mut state_missing = MachineState::new();
    let err = prepare_dynlink_with_search_paths(&mut state_missing, &binary, &[])
        .expect_err("missing non-HLE dependency must fail");
    assert!(err.to_string().contains("x64_prelinked_lib.so"), "{err:#}");

    // Configured target root -> loads at link address 0x700_0000 and records BLAKE3 digest.
    let mut state_ok = MachineState::new();
    for s in &binary.inner().sections {
        if s.virtual_size > 0 {
            state_ok
                .load_image_range(s.virtual_address, s.virtual_size, &[])
                .unwrap();
            state_ok.page_map.map_region(
                s.virtual_address,
                s.virtual_size,
                prot::VALID | prot::READ | prot::WRITE | prot::EXEC,
                false,
            );
        }
    }
    let info = prepare_dynlink_with_search_paths(&mut state_ok, &binary, &[testdata]).unwrap();
    assert_eq!(info.mode, DynlinkMode::SharedLibs);
    assert_eq!(info.loaded_lib_identities.len(), 1);
    assert_eq!(info.loaded_lib_identities[0].soname, "x64_prelinked_lib.so");
    assert_eq!(info.loaded_lib_identities[0].base, 0x700_0000);
    assert!(!info.loaded_lib_identities[0].blake3.is_empty());
}
