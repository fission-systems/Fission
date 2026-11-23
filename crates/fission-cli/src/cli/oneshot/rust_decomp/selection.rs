use crate::cli::args::OneShotArgs;
use crate::cli::oneshot::function_select::{
    select_batch_functions, select_explicit_functions, select_function_by_address,
    select_functions_from_addresses_file,
};
use fission_loader::loader::LoadedBinary;
use std::io;

pub(crate) fn collect_target_functions(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
) -> io::Result<crate::cli::oneshot::function_select::BatchFunctionSelection> {
    if let Some(addr) = cli.address {
        if let Some(func) =
            select_function_by_address(binary, addr).or_else(|| binary.function_at(addr))
        {
            return Ok(select_explicit_functions(
                vec![func.clone()],
                cli.include_nonuser_functions,
            ));
        }
        return Ok(select_explicit_functions(
            vec![],
            cli.include_nonuser_functions,
        ));
    }

    if cli.decomp_all {
        if let Some(address_file) = &cli.addresses_file {
            // Swallowing the error here decompiled nothing, said nothing, and
            // exited zero: a file holding one bad line among good addresses
            // looked exactly like a binary with no functions.
            let functions = select_functions_from_addresses_file(binary, address_file)?;
            return Ok(select_explicit_functions(
                functions,
                cli.include_nonuser_functions,
            ));
        }
        return Ok(select_batch_functions(
            binary,
            cli.include_nonuser_functions,
            cli.decomp_limit,
        ));
    }

    Ok(select_explicit_functions(
        vec![],
        cli.include_nonuser_functions,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};
    use std::path::PathBuf;

    #[test]
    fn missing_addresses_file_returns_not_found_error_without_exiting() {
        let binary = LoadedBinaryBuilder::new("test.bin".into(), DataBuffer::Heap(vec![0; 16]))
            .format("ELF")
            .build()
            .expect("build test binary");
        let missing = std::env::temp_dir().join(format!(
            "fission-missing-addrs-{}-{}.txt",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_file(&missing);
        let cli = OneShotArgs {
            decomp_all: true,
            addresses_file: Some(PathBuf::from(&missing)),
            ..OneShotArgs::default()
        };

        let err = collect_target_functions(&cli, &binary).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}
