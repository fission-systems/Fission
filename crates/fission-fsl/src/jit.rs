//! Cranelift compilation for the decode-and-lift hot path.
//!
//! Generated host code reads instruction bytes and writes typed FIR records.
//! It emits semantic graphs; it does not execute guest instructions or lower
//! through P-code.

use cranelift_codegen::ir::{condcodes::IntCC, types, AbiParam, InstBuilder, MemFlagsData};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::CodegenError;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, FuncId, Linkage, Module, ModuleError};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::{CompiledInstruction, FirOp, FslError, FslcPackage, ValueDef, ValueId};

const OP_VM_STACK_POP: u32 = 0;
const OP_INT_ADD_WRAP: u32 = 1;
const OP_VM_STACK_PUSH: u32 = 2;

/// Stable C-layout record emitted by generated decoder/lifter functions.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NativeFirOp {
    pub kind: u32,
    pub output: u32,
    pub input0: u32,
    pub input1: u32,
    pub bits: u32,
    pub signed: u32,
}

/// One decoded instruction's semantic template in native-record form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeLift {
    pub instruction_index: usize,
    pub ops: Vec<NativeFirOp>,
}

type DecodeLiftFunction = unsafe extern "C" fn(u64, u64, u64, u64) -> i64;

/// Owns the JIT module so generated function memory remains live.
pub struct JitDecoder {
    _module: JITModule,
    decode_lift: DecodeLiftFunction,
    instruction_count: usize,
    op_counts: Vec<usize>,
    max_ops: usize,
}

impl JitDecoder {
    /// Compile a portable FSL package into a host-native byte decoder and FIR
    /// emitter. Current generated encodings are exact one-byte opcodes.
    pub fn compile(package: &FslcPackage) -> Result<Self, FslError> {
        if std::mem::size_of::<usize>() != 8 {
            return Err(FslError::at(
                1,
                1,
                "the initial Cranelift JIT ABI requires a 64-bit host",
            ));
        }
        if package.instructions.is_empty() {
            return Err(FslError::at(
                1,
                1,
                "cannot JIT-compile an empty FSL package",
            ));
        }

        let mut settings_builder = settings::builder();
        settings_builder
            .set("opt_level", "speed")
            .map_err(|error| FslError::at(1, 1, format!("set Cranelift opt level: {error}")))?;
        let isa_builder = cranelift_native::builder()
            .map_err(|error| FslError::at(1, 1, format!("host ISA unavailable: {error}")))?;
        let isa = isa_builder
            .finish(settings::Flags::new(settings_builder))
            .map_err(|error| FslError::at(1, 1, format!("configure Cranelift ISA: {error}")))?;
        let jit_builder = JITBuilder::with_isa(isa, default_libcall_names());
        let mut module = JITModule::new(jit_builder);
        let (function_id, max_ops) = define_decode_lift_function(&mut module, package)?;
        module
            .finalize_definitions()
            .map_err(|error| FslError::at(1, 1, format!("finalize FSL JIT code: {error}")))?;
        let address = module.get_finalized_function(function_id);
        // SAFETY: the function was compiled with the four-u64 ABI declared above;
        // `module` is retained in `Self`, keeping the executable allocation live.
        let decode_lift = unsafe { std::mem::transmute::<*const u8, DecodeLiftFunction>(address) };

        Ok(Self {
            _module: module,
            decode_lift,
            instruction_count: package.instructions.len(),
            op_counts: package
                .instructions
                .iter()
                .map(|instruction| instruction.ops.len())
                .collect(),
            max_ops,
        })
    }

    /// Decode one byte sequence and emit its FIR template as compact native
    /// records. `None` means no FSL instruction matched the leading byte.
    pub fn decode_and_lift(&self, bytes: &[u8]) -> Result<Option<NativeLift>, FslError> {
        let mut output = vec![NativeFirOp::default(); self.max_ops];
        let result = unsafe {
            // SAFETY: pointers refer to live slices for the duration of the call;
            // generated code checks input length and output capacity before access.
            (self.decode_lift)(
                bytes.as_ptr() as u64,
                bytes.len() as u64,
                output.as_mut_ptr() as u64,
                output.len() as u64,
            )
        };
        if result == 0 {
            return Ok(None);
        }
        if result == -1 {
            return Err(FslError::at(
                1,
                1,
                "truncated input: expected one opcode byte",
            ));
        }
        if result == -2 {
            return Err(FslError::at(1, 1, "FIR output capacity is too small"));
        }
        let instruction_index = usize::try_from(result - 1)
            .map_err(|_| FslError::at(1, 1, "JIT returned an invalid instruction index"))?;
        if instruction_index >= self.instruction_count {
            return Err(FslError::at(
                1,
                1,
                "JIT returned an out-of-range instruction index",
            ));
        }
        output.truncate(self.op_counts[instruction_index]);
        Ok(Some(NativeLift {
            instruction_index,
            ops: output,
        }))
    }
}

/// Compile a portable FSL package to a host-native relocatable object file.
/// The object exports `fsl_decode_lift` with the same ABI as the JIT path.
pub fn emit_aot_object(package: &FslcPackage) -> Result<Vec<u8>, FslError> {
    let mut settings_builder = settings::builder();
    settings_builder
        .set("opt_level", "speed")
        .map_err(|error| FslError::at(1, 1, format!("set Cranelift opt level: {error}")))?;
    let isa_builder = cranelift_native::builder()
        .map_err(|error| FslError::at(1, 1, format!("host ISA unavailable: {error}")))?;
    let isa = isa_builder
        .finish(settings::Flags::new(settings_builder))
        .map_err(|error| FslError::at(1, 1, format!("configure Cranelift ISA: {error}")))?;
    let object_builder =
        ObjectBuilder::new(isa, "fsl_decode_lift".to_string(), default_libcall_names())
            .map_err(|error| FslError::at(1, 1, format!("create FSL AOT object: {error}")))?;
    let mut module = ObjectModule::new(object_builder);
    define_decode_lift_function(&mut module, package)?;
    module
        .finish()
        .emit()
        .map_err(|error| FslError::at(1, 1, format!("emit FSL AOT object: {error}")))
}

fn define_decode_lift_function<M: Module>(
    module: &mut M,
    package: &FslcPackage,
) -> Result<(FuncId, usize), FslError> {
    package.validate()?;
    if package.instructions.is_empty() {
        return Err(FslError::at(1, 1, "cannot compile an empty FSL package"));
    }

    let mut signature = module.make_signature();
    signature.params.extend([
        AbiParam::new(types::I64), // input pointer
        AbiParam::new(types::I64), // input length
        AbiParam::new(types::I64), // output pointer
        AbiParam::new(types::I64), // output capacity in FIR records
    ]);
    signature.returns.push(AbiParam::new(types::I64));
    let function_id = module
        .declare_function("fsl_decode_lift", Linkage::Export, &signature)
        .map_err(|error| FslError::at(1, 1, format!("declare FSL decoder/lifter: {error}")))?;

    let mut context = module.make_context();
    context.func.signature = signature;
    let mut builder_context = FunctionBuilderContext::new();
    let mut max_ops = 0usize;
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let params = builder.block_params(entry).to_vec();
        let input_ptr = params[0];
        let input_len = params[1];
        let output_ptr = params[2];
        let output_capacity = params[3];

        let dispatch = builder.create_block();
        let truncated = builder.create_block();
        let has_opcode = builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, input_len, 1);
        builder
            .ins()
            .brif(has_opcode, dispatch, &[], truncated, &[]);
        builder.seal_block(entry);

        builder.switch_to_block(truncated);
        let truncated_status = builder.ins().iconst(types::I64, -1);
        builder.ins().return_(&[truncated_status]);
        builder.seal_block(truncated);

        builder.switch_to_block(dispatch);
        let opcode_byte = builder
            .ins()
            .load(types::I8, MemFlagsData::trusted(), input_ptr, 0);
        let opcode = builder.ins().uextend(types::I64, opcode_byte);

        let capacity_error = builder.create_block();
        let mut current = dispatch;

        for (index, instruction) in package.instructions.iter().enumerate() {
            let matched = builder.create_block();
            let next = builder.create_block();

            let is_match =
                builder
                    .ins()
                    .icmp_imm(IntCC::Equal, opcode, i64::from(instruction.opcode));
            builder.ins().brif(is_match, matched, &[], next, &[]);
            builder.seal_block(current);

            builder.switch_to_block(matched);
            let enough_space = builder.ins().icmp_imm(
                IntCC::UnsignedGreaterThanOrEqual,
                output_capacity,
                instruction.ops.len() as i64,
            );
            let emit = builder.create_block();
            builder
                .ins()
                .brif(enough_space, emit, &[], capacity_error, &[]);
            builder.seal_block(matched);

            builder.switch_to_block(emit);
            for (op_index, op) in instruction.ops.iter().enumerate() {
                let record = native_record(instruction, op)?;
                let byte_offset = i32::try_from(op_index * std::mem::size_of::<NativeFirOp>())
                    .map_err(|_| FslError::at(1, 1, "FIR output offset exceeds i32"))?;
                emit_u32(&mut builder, output_ptr, byte_offset, record.kind);
                emit_u32(&mut builder, output_ptr, byte_offset + 4, record.output);
                emit_u32(&mut builder, output_ptr, byte_offset + 8, record.input0);
                emit_u32(&mut builder, output_ptr, byte_offset + 12, record.input1);
                emit_u32(&mut builder, output_ptr, byte_offset + 16, record.bits);
                emit_u32(&mut builder, output_ptr, byte_offset + 20, record.signed);
            }
            let instruction_result = builder.ins().iconst(types::I64, (index + 1) as i64);
            builder.ins().return_(&[instruction_result]);
            builder.seal_block(emit);
            max_ops = max_ops.max(instruction.ops.len());
            current = next;
            builder.switch_to_block(current);
        }

        let no_match_status = builder.ins().iconst(types::I64, 0);
        builder.ins().return_(&[no_match_status]);
        builder.seal_block(current);
        builder.switch_to_block(capacity_error);
        let capacity_status = builder.ins().iconst(types::I64, -2);
        builder.ins().return_(&[capacity_status]);
        builder.seal_block(capacity_error);
        builder.seal_all_blocks();
        builder.finalize();
    }

    module
        .define_function(function_id, &mut context)
        .map_err(|error| {
            FslError::at(
                1,
                1,
                format!(
                    "compile FSL decoder/lifter: {}",
                    describe_compile_error(&error)
                ),
            )
        })?;
    module.clear_context(&mut context);
    Ok((function_id, max_ops))
}

fn describe_compile_error(error: &ModuleError) -> String {
    match error {
        ModuleError::Compilation(CodegenError::Verifier(errors)) => {
            let first = errors
                .0
                .first()
                .map(ToString::to_string)
                .unwrap_or_else(|| "unknown verifier error".to_string());
            format!(
                "Cranelift IR verification failed ({} error(s)); first: {first}",
                errors.0.len()
            )
        }
        _ => error.to_string(),
    }
}

fn native_record(instruction: &CompiledInstruction, op: &FirOp) -> Result<NativeFirOp, FslError> {
    let get_value = |id: ValueId| -> Result<&ValueDef, FslError> {
        instruction
            .values
            .get(id.0 as usize)
            .filter(|value| value.id == id)
            .ok_or_else(|| FslError::at(1, 1, format!("unknown FIR value id {}", id.0)))
    };
    let encode_type = |value: &ValueDef| NativeFirOp {
        bits: u32::from(value.ty.bits),
        signed: u32::from(value.ty.sign == crate::IntegerSign::Signed),
        ..NativeFirOp::default()
    };
    Ok(match op {
        FirOp::VmStackPop { output } => {
            let mut record = encode_type(get_value(*output)?);
            record.kind = OP_VM_STACK_POP;
            record.output = u32::from(output.0);
            record
        }
        FirOp::IntAddWrap {
            output,
            left,
            right,
        } => {
            let mut record = encode_type(get_value(*output)?);
            record.kind = OP_INT_ADD_WRAP;
            record.output = u32::from(output.0);
            record.input0 = u32::from(left.0);
            record.input1 = u32::from(right.0);
            record
        }
        FirOp::VmStackPush { value } => {
            let mut record = encode_type(get_value(*value)?);
            record.kind = OP_VM_STACK_PUSH;
            record.input0 = u32::from(value.0);
            record
        }
    })
}

fn emit_u32(
    builder: &mut FunctionBuilder,
    output_ptr: cranelift_codegen::ir::Value,
    byte_offset: i32,
    value: u32,
) {
    let pointer = builder.ins().iadd_imm(output_ptr, i64::from(byte_offset));
    let value = builder.ins().iconst(types::I32, i64::from(value));
    let value = builder.ins().uextend(types::I64, value);
    builder
        .ins()
        .istore32(MemFlagsData::trusted(), value, pointer, 0);
}
