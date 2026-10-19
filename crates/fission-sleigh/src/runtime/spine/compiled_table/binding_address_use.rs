//! Dynamic witness for address dependencies of a complete native binding attempt.
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct AddressUse {
    pub pc_value_read: bool,
    pub max_checked_offset: u64,
}
impl AddressUse {
    /// Both the original and replacement address must satisfy all observed
    /// checked additions. An earlier discarded overflow must not be turned
    /// into a successful candidate by reusing a later winner at another PC.
    pub(super) fn allows_reuse_at(self, address: u64) -> bool {
        !self.pc_value_read && address.checked_add(self.max_checked_offset).is_some()
    }
}
thread_local! {
    static CURRENT: Cell<Option<AddressUse>> = const { Cell::new(None) };
}
pub(super) fn read_pc() {
    CURRENT.with(|current| {
        if let Some(mut value) = current.get() {
            value.pc_value_read = true;
            current.set(Some(value));
        }
    });
}
pub(super) fn checked_offset(offset: u64) {
    CURRENT.with(|current| {
        if let Some(mut value) = current.get() {
            value.max_checked_offset = value.max_checked_offset.max(offset);
            current.set(Some(value));
        }
    });
}
struct Scope {
    previous: Option<AddressUse>,
    _local: PhantomData<Rc<()>>,
}
impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.with(|current| {
            let child = current.replace(self.previous);
            if let (Some(mut parent), Some(child)) = (self.previous, child) {
                parent.pc_value_read |= child.pc_value_read;
                parent.max_checked_offset = parent.max_checked_offset.max(child.max_checked_offset);
                current.set(Some(parent));
            }
        });
    }
}
pub(super) fn capture<T>(parse: impl FnOnce() -> T) -> (T, AddressUse) {
    let _scope = Scope {
        previous: CURRENT.with(|current| current.replace(Some(AddressUse::default()))),
        _local: PhantomData,
    };
    let result = parse();
    let witness = CURRENT.with(|current| current.get().expect("active binding witness"));
    (result, witness)
}
