#[derive(Debug, Clone)]
pub struct FidbfLibrary {
    pub key: i64,
    pub family_name: String,
    pub version: String,
    pub variant: String,
    pub ghidra_version: String,
    pub language_id: String,
}

#[derive(Debug, Clone)]
pub struct FidbfFunction {
    pub key: i64,
    pub library_id: i64,
    pub name: String,
    pub full_hash: u64,
    pub specific_hash: u64,
    pub code_unit_size: u32,
    pub entry_point: u64,
    pub has_terminator: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FidbfRelationType {
    Call,
    Jump,
    Unknown(i32),
}

impl From<i32> for FidbfRelationType {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Call,
            1 => Self::Jump,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FidbfRelation {
    pub function_id: i64,
    pub related_id: i64,
    pub relation_type: FidbfRelationType,
}

#[derive(Debug, Clone)]
pub struct FidbfDatabase {
    pub source_path: String,
    pub libraries: Vec<FidbfLibrary>,
    pub functions: Vec<FidbfFunction>,
    pub relations: Vec<FidbfRelation>,
}

impl FidbfDatabase {
    pub fn library_by_id(&self, id: i64) -> Option<&FidbfLibrary> {
        self.libraries.iter().find(|library| library.key == id)
    }

    pub fn functions_by_specific_hash(&self, hash: u64) -> Vec<&FidbfFunction> {
        self.functions
            .iter()
            .filter(|function| function.specific_hash == hash)
            .collect()
    }
}
