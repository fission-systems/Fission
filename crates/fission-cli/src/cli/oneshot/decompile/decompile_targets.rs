use super::*;
use crate::cli::oneshot::function_select::{
    canonical_functions_sorted, prefer_function_name, select_function_by_address,
    select_functions_from_addresses_file,
};

pub(crate) fn select_candidate_functions<'a>(
    cli: &OneShotArgs,
    binary: &'a LoadedBinary,
) -> io::Result<Vec<&'a FunctionInfo>> {
    if let Some(address_file) = &cli.addresses_file {
        return select_functions_from_addresses_file(binary, address_file);
    }

    let mut functions = canonical_functions_sorted(binary);
    if let Some(address) = cli.address {
        functions.retain(|func| func.address == address);
    } else if let Some(limit) = cli.functions_limit {
        functions.truncate(limit);
    }

    Ok(functions)
}

pub(crate) fn collect_target_functions<'a>(
    binary: &'a LoadedBinary,
    address: Option<u64>,
    addresses_file: Option<&std::path::Path>,
    decomp_all: bool,
    decomp_limit: Option<usize>,
) -> Vec<&'a FunctionInfo> {
    if let Some(address_file) = addresses_file {
        return select_functions_from_addresses_file(binary, address_file).unwrap_or_default();
    }

    if decomp_all {
        let collected = canonical_functions_sorted(binary);
        if let Some(n) = decomp_limit {
            return collected.into_iter().take(n).collect();
        }
        return collected;
    }

    if let Some(addr) = address {
        return select_function_by_address(binary, addr)
            .into_iter()
            .collect();
    }

    vec![]
}
