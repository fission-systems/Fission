use fission_static::analysis::decomp::{NirWorkerRequest, NirWorkerResponse, execute_nir_worker};
use std::io::{self, Read, Write};

#[cfg(feature = "allocator-mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    feature = "allocator-jemallocator",
    not(feature = "allocator-mimalloc")
))]
#[global_allocator]
static GLOBAL_ALLOCATOR: jemallocator::Jemalloc = jemallocator::Jemalloc;

fn main() -> io::Result<()> {
    fission_core::logging::try_init_tracing("warn");

    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let response = match serde_json::from_str::<NirWorkerRequest>(&input) {
        Ok(request) => execute_nir_worker(&request),
        Err(err) => NirWorkerResponse {
            success: false,
            code: None,
            build_stats: None,
            hint_stats: None,
            error: Some(format!("Fission NIR worker request parse failed: {err}")),
        },
    };

    serde_json::to_writer(io::stdout().lock(), &response)?;
    io::stdout().write_all(b"\n")?;

    if response.success {
        Ok(())
    } else {
        std::process::exit(2);
    }
}
