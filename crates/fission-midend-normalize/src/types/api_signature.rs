//! Windows API signature lookup and type-name conversion.
//!
//! This module owns the external signature-resource boundary used by the
//! call-site propagation passes. The parent module re-exports the public
//! lookup helpers to preserve the existing normalize API.

use crate::prelude::*;
use fission_signatures::{ApiSignature, SIGNATURE_RESOURCES, symbol_for_win_api_database_lookup};

/// Convert a Windows API type name string to a `NirType`, or `None` for
/// unconstrained types (void, variadic, …).
pub fn win_type_name_to_nir(name: &str) -> Option<NirType> {
    // Strip leading/trailing whitespace and trailing `*` for pointer types.
    let name = name.trim();

    // Pointer types first.
    if name.ends_with('*') {
        let inner_name = name.trim_end_matches('*').trim();
        let inner = match inner_name {
            "VOID" | "void" | "" => NirType::Unknown,
            "CHAR" | "char" => NirType::Int {
                bits: 8,
                signed: true,
            },
            "WCHAR" | "wchar_t" | "TCHAR" => NirType::Int {
                bits: 16,
                signed: false,
            },
            "BYTE" | "UCHAR" | "unsigned char" => NirType::Int {
                bits: 8,
                signed: false,
            },
            _ => NirType::Unknown,
        };
        return Some(NirType::Ptr(Box::new(inner)));
    }

    let nir = match name {
        // Void — no constraint.
        "void" | "VOID" => return None,
        // 32-bit unsigned integers.
        "DWORD" | "UINT" | "ULONG" | "UINT32" | "ULONG32" | "DWORD32" => NirType::Int {
            bits: 32,
            signed: false,
        },
        // 32-bit signed integers.
        "INT" | "LONG" | "INT32" | "LONG32" => NirType::Int {
            bits: 32,
            signed: true,
        },
        // BOOL is signed int32 in Windows ABI.
        "BOOL" => NirType::Int {
            bits: 32,
            signed: true,
        },
        // 16-bit.
        "WORD" | "USHORT" | "UINT16" => NirType::Int {
            bits: 16,
            signed: false,
        },
        "SHORT" | "INT16" => NirType::Int {
            bits: 16,
            signed: true,
        },
        // 8-bit.
        "BYTE" | "UCHAR" | "UINT8" | "BOOLEAN" => NirType::Int {
            bits: 8,
            signed: false,
        },
        "CHAR" | "INT8" => NirType::Int {
            bits: 8,
            signed: true,
        },
        // 64-bit unsigned.
        "QWORD" | "UINT64" | "ULONG64" | "DWORD64" | "ULONGLONG" | "ULONG_PTR" | "SIZE_T"
        | "UINT_PTR" => NirType::Int {
            bits: 64,
            signed: false,
        },
        // 64-bit signed.
        "LONGLONG" | "INT64" | "LONG64" | "LONG_PTR" | "SSIZE_T" | "INT_PTR" => NirType::Int {
            bits: 64,
            signed: true,
        },
        // Generic pointer to void.
        "LPVOID" | "PVOID" | "HANDLE" => NirType::Ptr(Box::new(NirType::Unknown)),
        // Typed string pointers.
        "LPSTR" | "LPCSTR" | "PSTR" | "PCSTR" => NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        })),
        "LPWSTR" | "LPCWSTR" | "PWSTR" | "PCWSTR" => NirType::Ptr(Box::new(NirType::Int {
            bits: 16,
            signed: false,
        })),
        // Opaque Windows handle types — typed as Ptr to empty Aggregate.
        "HWND"
        | "HMODULE"
        | "HINSTANCE"
        | "HKEY"
        | "HFILE"
        | "HBITMAP"
        | "HBRUSH"
        | "HFONT"
        | "HPEN"
        | "HICON"
        | "HCURSOR"
        | "HMENU"
        | "HRGN"
        | "HDC"
        | "HGLOBAL"
        | "HLOCAL"
        | "HRSRC"
        | "HWINSTA"
        | "HDESK"
        | "HPALETTE"
        | "HENHMETAFILE"
        | "HMETAFILE"
        | "HCOLORSPACE"
        | "HCONV"
        | "HCONVLIST"
        | "HDDEDATA"
        | "HDDERESERVATION"
        | "HSZ"
        | "HHOOK"
        | "HMONITOR"
        | "HWINEVENTHOOK"
        | "HPOWERNOTIFY"
        | "SC_HANDLE"
        | "SERVICE_STATUS_HANDLE" => NirType::Ptr(Box::new(NirType::Aggregate {
            size: 0,
            fields: vec![],
        })),
        // NTSTATUS / HRESULT: signed 32-bit.
        "NTSTATUS" | "HRESULT" => NirType::Int {
            bits: 32,
            signed: true,
        },
        // MSVC va_list (opaque; model as generic pointer).
        "va_list" => NirType::Ptr(Box::new(NirType::Unknown)),
        // Unknown / not yet mapped → no constraint.
        _ => return None,
    };
    Some(nir)
}

pub fn is_known_api_signature(name: &str) -> bool {
    api_signature_via_import_aliases(name).is_some()
}

pub fn api_signature(name: &str) -> Option<&'static ApiSignature> {
    SIGNATURE_RESOURCES.api_signature(name)
}

#[inline]
pub(super) fn api_signature_via_import_aliases(name: &str) -> Option<&'static ApiSignature> {
    api_signature(name)
        .or_else(|| symbol_for_win_api_database_lookup(name).and_then(|flat| api_signature(flat)))
}

/// Return the NirType implied by the API signature's return type string.
/// Returns `None` when the return type is void or not mappable.
pub(super) fn resolve_return_ty(ret_type_str: &str) -> Option<NirType> {
    win_type_name_to_nir(ret_type_str)
}

#[cfg(test)]
mod tests {
    use super::win_type_name_to_nir;
    use crate::prelude::NirType;

    #[test]
    fn unsigned_char_pointer_type_is_an_unsigned_byte_pointer() {
        assert_eq!(
            win_type_name_to_nir("unsigned char*"),
            Some(NirType::Ptr(Box::new(NirType::Int {
                bits: 8,
                signed: false,
            })))
        );
    }
}
