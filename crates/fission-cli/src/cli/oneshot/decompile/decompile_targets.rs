use super::*;

pub(super) fn prefer_function_name(candidate: &str, current: &str) -> bool {
    let candidate_is_sub = candidate.starts_with("sub_");
    let current_is_sub = current.starts_with("sub_");
    if candidate_is_sub != current_is_sub {
        return !candidate_is_sub;
    }
    candidate.len() > current.len()
}

pub(crate) fn select_candidate_functions<'a>(
    cli: &OneShotArgs,
    binary: &'a LoadedBinary,
) -> io::Result<Vec<&'a FunctionInfo>> {
    let mut functions = binary.functions.iter().collect::<Vec<_>>();
    functions.sort_by_key(|func| func.address);

    if let Some(address_file) = &cli.addresses_file {
        let contents = fs::read_to_string(address_file)?;
        let mut selected = Vec::new();
        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let address = parse_hex_address(trimmed)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            if let Some(func) = functions
                .iter()
                .copied()
                .find(|func| func.address == address)
            {
                selected.push(func);
            }
        }
        return Ok(selected);
    }

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
    decomp_all: bool,
    decomp_limit: Option<usize>,
) -> Vec<&'a FunctionInfo> {
    if decomp_all {
        let collected: Vec<_> = binary.functions.iter().collect();
        if let Some(n) = decomp_limit {
            return collected.into_iter().take(n).collect();
        }
        return collected;
    }

    if let Some(addr) = address {
        let mut best: Option<&FunctionInfo> = None;
        for func in &binary.functions {
            if func.address != addr {
                continue;
            }
            match best {
                None => best = Some(func),
                Some(current) => {
                    if prefer_function_name(&func.name, &current.name) {
                        best = Some(func);
                    }
                }
            }
        }
        return best.into_iter().collect();
    }

    vec![]
}
