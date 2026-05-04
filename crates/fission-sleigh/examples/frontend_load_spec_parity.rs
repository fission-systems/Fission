//! Benchmark lane helper: compare `RuntimeSleighFrontend::new_for_load_spec` vs
//! `new_for_language("x86-64")` on the same byte window (canonical raw-P-code rows).

use anyhow::{bail, Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn usage() -> ! {
    eprintln!(
        "usage: frontend_load_spec_parity --binary PATH --addr HEX\n\
\n\
Loads the PE/ELF binary, reads 16 bytes at addr, and asserts load-spec vs entry-id\n\
frontends agree on decode_and_lift_with_len (same contract as historical crate test)."
    );
    std::process::exit(2);
}

fn parse_hex_u64(raw: &str) -> Result<u64> {
    let s = raw.trim();
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u64::from_str_radix(s, 16).with_context(|| format!("invalid hex address: {raw:?}"))
}

fn main() -> Result<()> {
    let mut binary_path = None::<std::path::PathBuf>;
    let mut addr = None::<u64>;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--binary" => binary_path = Some(args.next().context("--binary requires PATH")?.into()),
            "--addr" => addr = Some(parse_hex_u64(&args.next().context("--addr requires HEX")?)?),
            "--help" | "-h" => usage(),
            other => bail!("unknown argument: {other}"),
        }
    }
    let binary_path = binary_path.context("missing --binary")?;
    let entry_address = addr.context("missing --addr")?;

    let binary = LoadedBinary::from_file(&binary_path)
        .with_context(|| format!("load binary {}", binary_path.display()))?;
    let bytes = binary
        .view_bytes(entry_address, 16)
        .with_context(|| format!("no bytes at 0x{entry_address:x}"))?;
    let load_spec = binary.load_spec().context("missing BinaryLoadSpec")?.clone();

    let from_load_spec = RuntimeSleighFrontend::new_for_load_spec(&load_spec)?;
    let from_entry_id = RuntimeSleighFrontend::new_for_language("x86-64")?;

    let load_spec_result = from_load_spec.decode_and_lift_with_len(bytes, entry_address);
    let entry_id_result = from_entry_id.decode_and_lift_with_len(bytes, entry_address);

    match (load_spec_result, entry_id_result) {
        (Ok((lhs_ops, lhs_len)), Ok((rhs_ops, rhs_len))) => {
            if lhs_len != rhs_len || lhs_ops != rhs_ops {
                eprintln!(
                    "load_spec vs entry-id mismatch at 0x{entry_address:x}: len {lhs_len} vs {rhs_len}"
                );
                std::process::exit(1);
            }
            println!("ok load_spec_parity 0x{entry_address:x}");
        }
        (Err(lhs_err), Err(rhs_err)) => {
            let lhs_s = format!("{lhs_err:#}");
            let rhs_s = format!("{rhs_err:#}");
            if lhs_s != rhs_s {
                eprintln!("load_spec vs entry-id error text mismatch at 0x{entry_address:x}");
                eprintln!("lhs: {lhs_s}");
                eprintln!("rhs: {rhs_s}");
                std::process::exit(1);
            }
            println!("ok load_spec_parity (both errors match) 0x{entry_address:x}");
        }
        (Ok(_), Err(err)) => {
            eprintln!("load_spec ok but entry-id failed at 0x{entry_address:x}: {err:#}");
            std::process::exit(1);
        }
        (Err(err), Ok(_)) => {
            eprintln!("entry-id ok but load_spec failed at 0x{entry_address:x}: {err:#}");
            std::process::exit(1);
        }
    }

    Ok(())
}
