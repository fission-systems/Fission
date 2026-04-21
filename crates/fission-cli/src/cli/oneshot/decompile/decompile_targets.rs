use super::*;
use crate::cli::oneshot::function_select::{
    BatchFunctionSelection, prefer_function_name, select_batch_functions,
    select_explicit_functions, select_function_by_address, select_functions_from_addresses_file,
};

pub(crate) fn select_candidate_functions<'a>(
    cli: &OneShotArgs,
    binary: &'a LoadedBinary,
) -> io::Result<BatchFunctionSelection<'a>> {
    if let Some(address_file) = &cli.addresses_file {
        let functions = select_functions_from_addresses_file(binary, address_file)?;
        return Ok(select_explicit_functions(
            functions,
            cli.include_nonuser_functions,
        ));
    }

    if let Some(address) = cli.address {
        let functions = select_function_by_address(binary, address)
            .into_iter()
            .collect();
        return Ok(select_explicit_functions(
            functions,
            cli.include_nonuser_functions,
        ));
    }

    Ok(select_batch_functions(
        binary,
        cli.include_nonuser_functions,
        cli.functions_limit,
    ))
}

pub(crate) fn collect_target_functions<'a>(
    cli: &OneShotArgs,
    binary: &'a LoadedBinary,
) -> BatchFunctionSelection<'a> {
    if let Some(address_file) = cli.addresses_file.as_deref() {
        let functions =
            select_functions_from_addresses_file(binary, address_file).unwrap_or_default();
        return select_explicit_functions(functions, cli.include_nonuser_functions);
    }

    if cli.decomp_all {
        return select_batch_functions(binary, cli.include_nonuser_functions, cli.decomp_limit);
    }

    if let Some(addr) = cli.address {
        let functions = select_function_by_address(binary, addr)
            .into_iter()
            .collect();
        return select_explicit_functions(functions, cli.include_nonuser_functions);
    }

    select_explicit_functions(vec![], cli.include_nonuser_functions)
}
