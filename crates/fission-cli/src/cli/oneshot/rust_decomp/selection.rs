use crate::cli::args::OneShotArgs;
use crate::cli::oneshot::function_select::{
    select_batch_functions, select_explicit_functions, select_function_by_address,
    select_functions_from_addresses_file,
};
use fission_loader::loader::LoadedBinary;

pub(crate) fn collect_target_functions(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
) -> crate::cli::oneshot::function_select::BatchFunctionSelection {
    if let Some(addr) = cli.address {
        if let Some(func) =
            select_function_by_address(binary, addr).or_else(|| binary.function_at(addr))
        {
            return select_explicit_functions(vec![func.clone()], cli.include_nonuser_functions);
        }
        return select_explicit_functions(vec![], cli.include_nonuser_functions);
    }

    if cli.decomp_all {
        if let Some(address_file) = &cli.addresses_file {
            // Swallowing the error here decompiled nothing, said nothing, and
            // exited zero: a file holding one bad line among good addresses
            // looked exactly like a binary with no functions.
            match select_functions_from_addresses_file(binary, address_file) {
                Ok(functions) => {
                    return select_explicit_functions(functions, cli.include_nonuser_functions);
                }
                Err(error) => {
                    eprintln!("error: {error}");
                    std::process::exit(1);
                }
            }
        }
        return select_batch_functions(binary, cli.include_nonuser_functions, cli.decomp_limit);
    }

    select_explicit_functions(vec![], cli.include_nonuser_functions)
}
