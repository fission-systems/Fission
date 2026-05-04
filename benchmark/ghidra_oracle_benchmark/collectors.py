"""Ghidra Java API collectors for oracle export (best-effort; fills collector_warnings on failure)."""

from __future__ import annotations

import time
from typing import Any


MAX_CALL_TARGETS = 64
MAX_STRING_SAMPLES = 32


def _str_java(obj: Any) -> str | None:
    if obj is None:
        return None
    try:
        return str(obj)
    except Exception:
        return None


def _count_xrefs_to(program: Any, addr: Any) -> int:
    rm = program.getReferenceManager()
    it = rm.getReferencesTo(addr)
    n = 0
    while it.hasNext():
        it.next()
        n += 1
    return n


def _collect_call_and_xref_facts(program: Any, func: Any) -> dict[str, Any]:
    listing = program.getListing()
    rm = program.getReferenceManager()
    entry = func.getEntryPoint()

    xref_in_count = _count_xrefs_to(program, entry)

    xref_out_count = 0
    call_targets: list[str] = []
    seen_calls: set[str] = set()
    external_call_count = 0

    body = func.getBody()
    ins_it = listing.getInstructions(body, True)
    while ins_it.hasNext():
        instr = ins_it.next()
        ref_it = rm.getReferencesFrom(instr.getMinAddress())
        while ref_it.hasNext():
            ref = ref_it.next()
            xref_out_count += 1
            try:
                if ref.getReferenceType().isCall():
                    to_addr = ref.getToAddress()
                    sym = program.getSymbolTable().getPrimarySymbol(to_addr)
                    if sym is not None and sym.isExternal():
                        external_call_count += 1
                    label = _str_java(sym) if sym is not None else _str_java(to_addr)
                    if label and label not in seen_calls and len(call_targets) < MAX_CALL_TARGETS:
                        seen_calls.add(label)
                        call_targets.append(label)
            except Exception:
                continue

    return {
        "xref_in_count": xref_in_count,
        "xref_out_count": xref_out_count,
        "call_targets": call_targets,
        "external_call_count": external_call_count,
    }


def _collect_string_refs(program: Any, func: Any) -> list[str]:
    """Sample defined strings referenced from instructions inside the function."""
    listing = program.getListing()
    rm = program.getReferenceManager()
    mem = program.getMemory()
    out: list[str] = []
    seen: set[str] = set()

    ins_it = listing.getInstructions(func.getBody(), True)
    while ins_it.hasNext():
        instr = ins_it.next()
        ref_it = rm.getReferencesFrom(instr.getMinAddress())
        while ref_it.hasNext():
            ref = ref_it.next()
            to_addr = ref.getToAddress()
            if to_addr is None:
                continue
            try:
                if not mem.contains(to_addr):
                    continue
                data = listing.getDefinedDataAt(to_addr)
                if data is None:
                    continue
                if not data.hasStringValue():
                    continue
                s = data.getDefaultValueRepresentation()
                text = _str_java(s)
                if text and text not in seen and len(out) < MAX_STRING_SAMPLES:
                    seen.add(text)
                    out.append(text)
            except Exception:
                continue
    return out


def _function_signature_facts(func: Any) -> dict[str, Any]:
    warnings: list[str] = []
    signature = None
    param_count = None
    try:
        signature = _str_java(func.getSignature(True))
    except Exception as exc:
        warnings.append(f"getSignature:{exc}")
    try:
        param_count = int(func.getParameterCount())
    except Exception as exc:
        warnings.append(f"getParameterCount:{exc}")

    return {
        "function_name": _str_java(func.getName()),
        "signature": signature,
        "param_count": param_count,
        "warnings": warnings,
    }


def _binary_import_export_snapshot(program: Any, limits: int = 256) -> dict[str, Any]:
    """Compact symbol-table snapshot (best-effort)."""
    symtab = program.getSymbolTable()
    imports: list[str] = []
    try:
        ext_it = symtab.getExternalSymbols()
        while ext_it.hasNext() and len(imports) < limits:
            sym = ext_it.next()
            name = _str_java(sym.getName())
            if name:
                imports.append(name)
    except Exception:
        pass

    return {
        "import_symbols_sample": imports[:limits],
        "export_symbols_note": "pe_exports_elf_dynsym_not_generalized_here",
    }


def collect_binary_snapshot(program: Any) -> dict[str, Any]:
    warnings: list[str] = []
    snap = {"program_name": None, "language": None, "compiler_spec": None}
    try:
        snap["program_name"] = _str_java(program.getName())
    except Exception as exc:
        warnings.append(f"program_info:{exc}")
    try:
        lang = program.getLanguage()
        snap["language"] = _str_java(lang.getLanguageID())
        snap["compiler_spec"] = _str_java(program.getCompilerSpec().getCompilerSpecID())
    except Exception as exc:
        warnings.append(f"language:{exc}")

    snap.update(_binary_import_export_snapshot(program))
    snap["collector_warnings"] = warnings
    return snap


def collect_function_oracle(
    program: Any,
    func: Any | None,
    decomp: Any,
    timeout_sec: int,
    monitor: Any,
) -> dict[str, Any]:
    """Collect decompiler + xref/call/string/signature facts for one Ghidra Function."""
    warnings: list[str] = []
    base: dict[str, Any] = {
        "decompile_success": False,
        "decompile_sec": 0.0,
        "decompile_failure_reason": None,
        "decompiled_c_preview": None,
        "signature": None,
        "function_name": None,
        "param_count": None,
        "local_symbol_count": None,
        "xref_in_count": None,
        "xref_out_count": None,
        "call_targets": [],
        "external_call_count": None,
        "string_refs": [],
        "collector_warnings": warnings,
    }

    if func is None:
        base["decompile_failure_reason"] = "missing_function"
        return base

    sig_facts = _function_signature_facts(func)
    base["function_name"] = sig_facts["function_name"]
    base["signature"] = sig_facts["signature"]
    base["param_count"] = sig_facts["param_count"]
    warnings.extend(sig_facts["warnings"])

    try:
        xref_facts = _collect_call_and_xref_facts(program, func)
        base.update(xref_facts)
    except Exception as exc:
        warnings.append(f"xrefs:{exc}")

    try:
        base["string_refs"] = _collect_string_refs(program, func)
    except Exception as exc:
        warnings.append(f"strings:{exc}")

    start = time.perf_counter()
    try:
        result = decomp.decompileFunction(func, timeout_sec, monitor)
        elapsed = time.perf_counter() - start
        base["decompile_sec"] = round(elapsed, 6)

        if result is None:
            base["decompile_failure_reason"] = "null_result"
        elif result.decompileCompleted() and result.getDecompiledFunction():
            base["decompile_success"] = True
            code = result.getDecompiledFunction().getC()
            preview = _str_java(code)
            if preview:
                base["decompiled_c_preview"] = preview[:4000]
            hf = result.getHighFunction()
            if hf is not None:
                try:
                    lmap = hf.getLocalSymbolMap()
                    base["local_symbol_count"] = int(lmap.getNumLocals())
                except Exception as exc:
                    warnings.append(f"high_locals:{exc}")
        else:
            try:
                base["decompile_failure_reason"] = _str_java(result.getErrorMessage()) or "decompile_incomplete"
            except Exception:
                base["decompile_failure_reason"] = "decompile_incomplete"
    except Exception as exc:
        base["decompile_sec"] = round(time.perf_counter() - start, 6)
        base["decompile_failure_reason"] = str(exc)
        warnings.append(f"decompile_exception:{exc}")

    base["collector_warnings"] = warnings
    return base
