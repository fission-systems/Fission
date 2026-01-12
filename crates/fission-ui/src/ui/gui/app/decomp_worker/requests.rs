use fission_loader::loader::FunctionInfo;
use fission_loader::loader::types::SectionInfo;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DecompileTask {
    pub request_id: u64,
    pub binary_id: String,
    pub address: u64,
}

#[derive(Debug, Clone)]
pub struct LoadBinaryRequest {
    pub binary_id: String,
    pub bytes: Vec<u8>,
    pub image_base: u64,
    pub iat_symbols: HashMap<u64, String>,
    pub global_symbols: HashMap<u64, String>,
    pub functions: Vec<FunctionInfo>,
    #[allow(dead_code)]
    pub gdt_json_path: Option<String>,
    pub sections: Vec<SectionInfo>,
    pub binary_hash: String,
}

#[derive(Debug, Clone)]
pub struct CfgAnalysisRequest {
    pub address: u64,
    pub binary_id: String,
}

#[derive(Debug, Clone)]
pub struct ClearCacheRequest {
    pub binary_id: String,
}

#[derive(Debug, Clone)]
pub enum WorkerRequest {
    Decompile(DecompileTask),
    LoadBinary(LoadBinaryRequest),
    ClearCache(ClearCacheRequest),
    CfgAnalysis(CfgAnalysisRequest),
}

impl WorkerRequest {
    pub fn decompile(request_id: u64, binary_id: String, address: u64) -> Self {
        Self::Decompile(DecompileTask {
            request_id,
            binary_id,
            address,
        })
    }

    pub fn load_binary(
        bytes: Vec<u8>,
        image_base: u64,
        iat_symbols: HashMap<u64, String>,
        global_symbols: HashMap<u64, String>,
        functions: Vec<FunctionInfo>,
        gdt_json_path: Option<String>,
        sections: Vec<SectionInfo>,
        binary_hash: String,
    ) -> Self {
        Self::LoadBinary(LoadBinaryRequest {
            binary_id: binary_hash.clone(),
            bytes,
            image_base,
            iat_symbols,
            global_symbols,
            functions,
            gdt_json_path,
            sections,
            binary_hash,
        })
    }

    pub fn cfg_analysis(address: u64, binary_id: String) -> Self {
        Self::CfgAnalysis(CfgAnalysisRequest { address, binary_id })
    }

    pub fn clear_cache(binary_id: String) -> Self {
        Self::ClearCache(ClearCacheRequest { binary_id })
    }

    pub(crate) fn binary_id(&self) -> &str {
        match self {
            WorkerRequest::Decompile(req) => &req.binary_id,
            WorkerRequest::LoadBinary(req) => &req.binary_id,
            WorkerRequest::ClearCache(req) => &req.binary_id,
            WorkerRequest::CfgAnalysis(req) => &req.binary_id,
        }
    }
}
