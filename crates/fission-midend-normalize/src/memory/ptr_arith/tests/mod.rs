mod field_access;
mod index_access;
mod offsets;
mod pointer_types;

use crate::prelude::*;

pub(super) fn make_binding_with_ty(name: &str, ty: NirType) -> PreHirBinding {
    PreHirBinding {
        name: name.to_owned(),
        ty,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    }
}

pub(super) fn make_func(locals: Vec<PreHirBinding>, body: Vec<PreHirStmt>) -> PreHirFunction {
    PreHirFunction {
        name: "test".to_owned(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals,
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body,
        ..Default::default()
    }
}
