use fission_signatures::win_api::WIN_API_DB;
use std::io::{self, Write};

fn main() -> io::Result<()> {
    let mut sigs: Vec<_> = WIN_API_DB.iter().collect();
    sigs.sort_by(|a, b| a.name.cmp(&b.name));

    let mut out = io::BufWriter::new(io::stdout());
    writeln!(out, "# name|return_type|param_name:type,param_name:type")?;

    for sig in sigs {
        let mut params = String::new();
        for (idx, param) in sig.params.iter().enumerate() {
            if idx > 0 {
                params.push(',');
            }
            params.push_str(&param.name);
            params.push(':');
            params.push_str(&param.type_name);
        }

        writeln!(out, "{}|{}|{}", sig.name, sig.return_type, params)?;
    }

    Ok(())
}
