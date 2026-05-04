# Stage parity benchmark

Mid-stage parity tooling belongs **only** under this directory. Do **not** wire it into default `cargo test` / workspace unit tests; fixture paths and fixed addresses live in manifests here (or sibling benchmark manifests), not in `crates/*/src/**/*.rs`.
