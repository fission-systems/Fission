//! eBPF processor adapter skeleton.

pub const SKELETON: super::ProcessorSkeleton = super::ProcessorSkeleton {
    ghidra_processor: "eBPF",
    module_name: "ebpf",
    executable_candidate: false,
};
