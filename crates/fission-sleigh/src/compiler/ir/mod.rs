use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use super::ast::{AstConstructor, AstItem, SpecAst, WithContextFrame};
use super::preprocessor::{ExpandedSpec, PreprocessedLine};
use super::sla::CompiledSlaTemplateLibrary;

mod types_shim {
    use std::collections::{BTreeMap, BTreeSet};
    use serde::{Deserialize, Serialize};

    include!("types/frontend_layout.rs");
    include!("types/display_decision_pattern.rs");
    include!("types/construct_const_tpl.rs");
    include!("types/tpl_impls_semantic.rs");
}
pub use types_shim::*;

mod lowering_shim {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    use anyhow::{Context, Result, anyhow, bail};

    use super::*;
    use super::super::ast::{AstConstructor, AstItem, SpecAst, WithContextFrame};
    use super::super::decode_metadata;
    use super::super::preprocessor::{ExpandedSpec, PreprocessedLine};
    use super::super::sla::CompiledSlaTemplateLibrary;

    include!("lowering/compile_and_collector_type.rs");
    include!("lowering/collector_impl.rs");
    include!("lowering/lowering_helpers.rs");
}
pub use lowering_shim::{build_frontend_from_sla_native_model, compile_frontend};

include!("template.rs");

#[cfg(test)]
mod tests;
