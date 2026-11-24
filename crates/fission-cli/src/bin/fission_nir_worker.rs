use fission_decompiler::{
    NirWorkerRequest, NirWorkerResponse, execute_nir_worker, worker::MAX_WORKER_REQUEST_BYTES,
};
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

    let mut input = Vec::new();
    let limit = (MAX_WORKER_REQUEST_BYTES as u64).saturating_add(1);
    io::stdin().take(limit).read_to_end(&mut input)?;

    let response = if input.len() > MAX_WORKER_REQUEST_BYTES {
        NirWorkerResponse {
            request_id: None,
            address: None,
            success: false,
            code: None,
            build_stats: None,
            hint_stats: None,
            error: Some(format!(
                "Fission NIR worker request exceeded {MAX_WORKER_REQUEST_BYTES} byte limit"
            )),
        }
    } else {
        match serde_json::from_slice::<NirWorkerRequest>(&input) {
            Ok(request) => execute_nir_worker(&request),
            Err(err) => NirWorkerResponse {
                request_id: None,
                address: None,
                success: false,
                code: None,
                build_stats: None,
                hint_stats: None,
                error: Some(format!("Fission NIR worker request parse failed: {err}")),
            },
        }
    };

    serde_json::to_writer(io::stdout().lock(), &response)?;
    io::stdout().write_all(b"\n")?;
    io::stdout().flush()?;

    if response.success {
        Ok(())
    } else {
        std::process::exit(2);
    }
}
