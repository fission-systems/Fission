mod byte_indices;
mod copy_constraints;
mod integer_constraints;
mod pointer_roles;
mod return_types;

use crate::prelude::*;

fn make_binding(name: &str) -> PreHirBinding {
    PreHirBinding {
        name: name.to_owned(),
        ty: NirType::Unknown,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    }
}

fn make_typed_binding(name: &str, ty: NirType, origin: NirBindingOrigin) -> PreHirBinding {
    PreHirBinding {
        name: name.to_owned(),
        ty,
        surface_type_name: None,
        origin: Some(origin),
        initializer: None,
    }
}

fn make_func(
    locals: Vec<PreHirBinding>,
    body: Vec<PreHirStmt>,
    return_type: NirType,
) -> PreHirFunction {
    PreHirFunction {
        name: "test".to_owned(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals,
        return_type,
        surface_return_type_name: None,
        body,
        ..Default::default()
    }
}
