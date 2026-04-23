//! BPF processor adapter skeleton.

pub const SKELETON: super::ProcessorSkeleton = super::ProcessorSkeleton {
    ghidra_processor: "BPF",
    module_name: "bpf",
    executable_candidate: false,
};
