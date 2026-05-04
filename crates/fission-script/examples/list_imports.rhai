// Emit one finding per imported symbol (inventory parity helpers run in Rust).
for imp in binary.imports() {
    if imp.is_import {
        emit(#{
            kind: "import",
            message: imp.name,
            address: imp.address,
            data: #{
                library: imp.library,
                is_export: imp.is_export,
            },
        });
    }
}
