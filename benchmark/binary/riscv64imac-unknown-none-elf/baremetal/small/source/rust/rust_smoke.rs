#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

pub enum Action {
    Add(u32),
    Sub(u32),
    Mul(u32),
    None,
}

pub struct Processor {
    state: u32,
}

impl Processor {
    pub fn new(seed: u32) -> Self {
        Processor { state: seed }
    }

    pub fn apply(&mut self, action: Action) {
        match action {
            Action::Add(val) => self.state = self.state.wrapping_add(val),
            Action::Sub(val) => self.state = self.state.wrapping_sub(val),
            Action::Mul(val) => self.state = self.state.wrapping_mul(val),
            Action::None => {}
        }
    }
}

pub trait Computable {
    fn compute(&self) -> u32;
}

impl Computable for Processor {
    fn compute(&self) -> u32 {
        self.state ^ 0xCAFEBABE
    }
}

static mut RUST_SINK: u32 = 0;

#[no_mangle]
pub extern "C" fn rust_smoke(seed: u32) -> u32 {
    let mut proc = Processor::new(seed);
    
    let actions = [
        Action::Add(10),
        Action::Mul(3),
        Action::Sub(5),
        Action::None,
    ];
    
    for action in actions {
        proc.apply(action);
    }
    
    let result = proc.compute();
    
    unsafe {
        RUST_SINK = result;
    }
    
    result
}
