use super::*;

// Ghidra ConstructTpl execution is sourced from compiled .sla payloads.
// Handwritten semantic template lowering was intentionally removed from the
// canonical raw P-code path; unsupported constructors must fail closed until
// their .sla ConstructTpl is decoded and mapped.
