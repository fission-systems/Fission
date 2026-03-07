//! Windows API Constant Groups
//!
//! Categorized enum values for context-aware constant substitution.
//! Each group contains constants that belong to a specific API parameter context.

use std::collections::HashMap;
use std::sync::LazyLock;

/// Global lazily-initialized Windows constants database for efficient reuse.
/// This avoids recreating the database with 16 enum groups on each use.
pub static WIN_CONSTANTS_DB: LazyLock<WinConstantsDb> = LazyLock::new(WinConstantsDb::new);

/// A group of related enum constants
#[derive(Debug, Clone)]
pub struct EnumGroup {
    pub name: String,
    pub values: Vec<(String, u64)>,
}

impl EnumGroup {
    pub fn new(name: &str, values: &[(&str, u64)]) -> Self {
        Self {
            name: name.to_string(),
            values: values.iter().map(|(n, v)| (n.to_string(), *v)).collect(),
        }
    }

    /// Get constant name for a value in this group
    pub fn get_name(&self, value: u64) -> Option<&str> {
        self.values
            .iter()
            .find(|(_, v)| *v == value)
            .map(|(n, _)| n.as_str())
    }

    /// Try to resolve a combined flags value (e.g., MEM_COMMIT | MEM_RESERVE)
    pub fn resolve_flags(&self, value: u64) -> Option<String> {
        if value == 0 {
            return Some("0".to_string());
        }

        let mut remaining = value;
        let mut parts: Vec<&str> = Vec::new();

        // Sort by value descending to match larger flags first
        let mut sorted_values: Vec<_> = self.values.iter().collect();
        sorted_values.sort_by(|a, b| b.1.cmp(&a.1));

        for (name, v) in &sorted_values {
            if *v != 0 && (remaining & *v) == *v {
                parts.push(name);
                remaining &= !*v;
            }
        }

        if remaining == 0 && !parts.is_empty() {
            Some(parts.join(" | "))
        } else {
            None
        }
    }
}

/// Windows API constant groups database
pub struct WinConstantsDb {
    groups: HashMap<String, EnumGroup>,
}

impl WinConstantsDb {
    pub fn new() -> Self {
        let mut db = Self {
            groups: HashMap::new(),
        };
        db.init_all_groups();
        db
    }

    pub fn get_group(&self, name: &str) -> Option<&EnumGroup> {
        self.groups.get(name)
    }

    pub fn resolve_in_group(&self, group_name: &str, value: u64) -> Option<String> {
        self.groups.get(group_name)?.resolve_flags(value)
    }

    fn add_group(&mut self, group: EnumGroup) {
        self.groups.insert(group.name.clone(), group);
    }

    fn init_all_groups(&mut self) {
        // PAGE_PROTECT - VirtualAlloc/VirtualProtect flProtect parameter
        self.add_group(EnumGroup::new(
            "PAGE_PROTECT",
            &[
                ("PAGE_NOACCESS", 0x01),
                ("PAGE_READONLY", 0x02),
                ("PAGE_READWRITE", 0x04),
                ("PAGE_WRITECOPY", 0x08),
                ("PAGE_EXECUTE", 0x10),
                ("PAGE_EXECUTE_READ", 0x20),
                ("PAGE_EXECUTE_READWRITE", 0x40),
                ("PAGE_EXECUTE_WRITECOPY", 0x80),
                ("PAGE_GUARD", 0x100),
                ("PAGE_NOCACHE", 0x200),
                ("PAGE_WRITECOMBINE", 0x400),
            ],
        ));

        // MEM_ALLOC - VirtualAlloc flAllocationType parameter
        self.add_group(EnumGroup::new(
            "MEM_ALLOC",
            &[
                ("MEM_COMMIT", 0x1000),
                ("MEM_RESERVE", 0x2000),
                ("MEM_DECOMMIT", 0x4000),
                ("MEM_RELEASE", 0x8000),
                ("MEM_FREE", 0x10000),
                ("MEM_RESET", 0x80000),
                ("MEM_TOP_DOWN", 0x100000),
                ("MEM_WRITE_WATCH", 0x200000),
                ("MEM_PHYSICAL", 0x400000),
                ("MEM_LARGE_PAGES", 0x20000000),
            ],
        ));

        // GENERIC_ACCESS - CreateFile dwDesiredAccess parameter
        self.add_group(EnumGroup::new(
            "GENERIC_ACCESS",
            &[
                ("GENERIC_READ", 0x80000000),
                ("GENERIC_WRITE", 0x40000000),
                ("GENERIC_EXECUTE", 0x20000000),
                ("GENERIC_ALL", 0x10000000),
            ],
        ));

        // FILE_SHARE - CreateFile dwShareMode parameter
        self.add_group(EnumGroup::new(
            "FILE_SHARE",
            &[
                ("FILE_SHARE_READ", 0x01),
                ("FILE_SHARE_WRITE", 0x02),
                ("FILE_SHARE_DELETE", 0x04),
            ],
        ));

        // FILE_CREATE - CreateFile dwCreationDisposition parameter
        self.add_group(EnumGroup::new(
            "FILE_CREATE",
            &[
                ("CREATE_NEW", 1),
                ("CREATE_ALWAYS", 2),
                ("OPEN_EXISTING", 3),
                ("OPEN_ALWAYS", 4),
                ("TRUNCATE_EXISTING", 5),
            ],
        ));

        // FILE_ATTRIBUTE - CreateFile dwFlagsAndAttributes parameter
        self.add_group(EnumGroup::new(
            "FILE_ATTRIBUTE",
            &[
                ("FILE_ATTRIBUTE_READONLY", 0x01),
                ("FILE_ATTRIBUTE_HIDDEN", 0x02),
                ("FILE_ATTRIBUTE_SYSTEM", 0x04),
                ("FILE_ATTRIBUTE_DIRECTORY", 0x10),
                ("FILE_ATTRIBUTE_ARCHIVE", 0x20),
                ("FILE_ATTRIBUTE_NORMAL", 0x80),
                ("FILE_ATTRIBUTE_TEMPORARY", 0x100),
                ("FILE_FLAG_WRITE_THROUGH", 0x80000000),
                ("FILE_FLAG_OVERLAPPED", 0x40000000),
                ("FILE_FLAG_NO_BUFFERING", 0x20000000),
                ("FILE_FLAG_RANDOM_ACCESS", 0x10000000),
                ("FILE_FLAG_SEQUENTIAL_SCAN", 0x08000000),
                ("FILE_FLAG_DELETE_ON_CLOSE", 0x04000000),
            ],
        ));

        // PROCESS_ACCESS - OpenProcess dwDesiredAccess parameter
        self.add_group(EnumGroup::new(
            "PROCESS_ACCESS",
            &[
                ("PROCESS_TERMINATE", 0x0001),
                ("PROCESS_CREATE_THREAD", 0x0002),
                ("PROCESS_SET_SESSIONID", 0x0004),
                ("PROCESS_VM_OPERATION", 0x0008),
                ("PROCESS_VM_READ", 0x0010),
                ("PROCESS_VM_WRITE", 0x0020),
                ("PROCESS_DUP_HANDLE", 0x0040),
                ("PROCESS_CREATE_PROCESS", 0x0080),
                ("PROCESS_SET_QUOTA", 0x0100),
                ("PROCESS_SET_INFORMATION", 0x0200),
                ("PROCESS_QUERY_INFORMATION", 0x0400),
                ("PROCESS_SUSPEND_RESUME", 0x0800),
                ("PROCESS_QUERY_LIMITED_INFORMATION", 0x1000),
                ("PROCESS_ALL_ACCESS", 0x1FFFFF),
            ],
        ));

        // THREAD_ACCESS - OpenThread dwDesiredAccess parameter
        self.add_group(EnumGroup::new(
            "THREAD_ACCESS",
            &[
                ("THREAD_TERMINATE", 0x0001),
                ("THREAD_SUSPEND_RESUME", 0x0002),
                ("THREAD_GET_CONTEXT", 0x0008),
                ("THREAD_SET_CONTEXT", 0x0010),
                ("THREAD_SET_INFORMATION", 0x0020),
                ("THREAD_QUERY_INFORMATION", 0x0040),
                ("THREAD_SET_THREAD_TOKEN", 0x0080),
                ("THREAD_IMPERSONATE", 0x0100),
                ("THREAD_DIRECT_IMPERSONATION", 0x0200),
                ("THREAD_ALL_ACCESS", 0x1FFFFF),
            ],
        ));

        // MB_TYPE - MessageBox uType parameter
        self.add_group(EnumGroup::new(
            "MB_TYPE",
            &[
                ("MB_OK", 0x00000000),
                ("MB_OKCANCEL", 0x00000001),
                ("MB_ABORTRETRYIGNORE", 0x00000002),
                ("MB_YESNOCANCEL", 0x00000003),
                ("MB_YESNO", 0x00000004),
                ("MB_RETRYCANCEL", 0x00000005),
                ("MB_ICONERROR", 0x00000010),
                ("MB_ICONWARNING", 0x00000030),
                ("MB_ICONINFORMATION", 0x00000040),
                ("MB_ICONQUESTION", 0x00000020),
                ("MB_DEFBUTTON1", 0x00000000),
                ("MB_DEFBUTTON2", 0x00000100),
                ("MB_DEFBUTTON3", 0x00000200),
            ],
        ));

        // WH_HOOK - SetWindowsHookEx idHook parameter
        self.add_group(EnumGroup::new(
            "WH_HOOK",
            &[
                ("WH_MSGFILTER", -1_i32 as u64),
                ("WH_JOURNALRECORD", 0),
                ("WH_JOURNALPLAYBACK", 1),
                ("WH_KEYBOARD", 2),
                ("WH_GETMESSAGE", 3),
                ("WH_CALLWNDPROC", 4),
                ("WH_CBT", 5),
                ("WH_SYSMSGFILTER", 6),
                ("WH_MOUSE", 7),
                ("WH_DEBUG", 9),
                ("WH_SHELL", 10),
                ("WH_FOREGROUNDIDLE", 11),
                ("WH_CALLWNDPROCRET", 12),
                ("WH_KEYBOARD_LL", 13),
                ("WH_MOUSE_LL", 14),
            ],
        ));

        // REG_KEY - Registry root keys
        self.add_group(EnumGroup::new(
            "REG_KEY",
            &[
                ("HKEY_CLASSES_ROOT", 0x80000000),
                ("HKEY_CURRENT_USER", 0x80000001),
                ("HKEY_LOCAL_MACHINE", 0x80000002),
                ("HKEY_USERS", 0x80000003),
                ("HKEY_PERFORMANCE_DATA", 0x80000004),
                ("HKEY_CURRENT_CONFIG", 0x80000005),
            ],
        ));

        // REG_ACCESS - Registry access rights
        self.add_group(EnumGroup::new(
            "REG_ACCESS",
            &[
                ("KEY_QUERY_VALUE", 0x0001),
                ("KEY_SET_VALUE", 0x0002),
                ("KEY_CREATE_SUB_KEY", 0x0004),
                ("KEY_ENUMERATE_SUB_KEYS", 0x0008),
                ("KEY_NOTIFY", 0x0010),
                ("KEY_CREATE_LINK", 0x0020),
                ("KEY_WOW64_64KEY", 0x0100),
                ("KEY_WOW64_32KEY", 0x0200),
                ("KEY_READ", 0x20019),
                ("KEY_WRITE", 0x20006),
                ("KEY_ALL_ACCESS", 0xF003F),
            ],
        ));

        // REG_TYPE - Registry value types
        self.add_group(EnumGroup::new(
            "REG_TYPE",
            &[
                ("REG_NONE", 0),
                ("REG_SZ", 1),
                ("REG_EXPAND_SZ", 2),
                ("REG_BINARY", 3),
                ("REG_DWORD", 4),
                ("REG_DWORD_BIG_ENDIAN", 5),
                ("REG_LINK", 6),
                ("REG_MULTI_SZ", 7),
                ("REG_QWORD", 11),
            ],
        ));

        // WAIT - WaitForSingleObject return values
        self.add_group(EnumGroup::new(
            "WAIT",
            &[
                ("WAIT_OBJECT_0", 0x00000000),
                ("WAIT_ABANDONED", 0x00000080),
                ("WAIT_TIMEOUT", 0x00000102),
                ("WAIT_FAILED", 0xFFFFFFFF),
                ("INFINITE", 0xFFFFFFFF),
            ],
        ));

        // TH32CS - CreateToolhelp32Snapshot dwFlags parameter
        self.add_group(EnumGroup::new(
            "TH32CS",
            &[
                ("TH32CS_SNAPHEAPLIST", 0x00000001),
                ("TH32CS_SNAPPROCESS", 0x00000002),
                ("TH32CS_SNAPTHREAD", 0x00000004),
                ("TH32CS_SNAPMODULE", 0x00000008),
                ("TH32CS_SNAPMODULE32", 0x00000010),
                ("TH32CS_SNAPALL", 0x0000001F),
                ("TH32CS_INHERIT", 0x80000000),
            ],
        ));

        // CREATION_FLAGS - CreateProcess dwCreationFlags parameter
        self.add_group(EnumGroup::new(
            "CREATION_FLAGS",
            &[
                ("DEBUG_PROCESS", 0x00000001),
                ("DEBUG_ONLY_THIS_PROCESS", 0x00000002),
                ("CREATE_SUSPENDED", 0x00000004),
                ("DETACHED_PROCESS", 0x00000008),
                ("CREATE_NEW_CONSOLE", 0x00000010),
                ("CREATE_NEW_PROCESS_GROUP", 0x00000200),
                ("CREATE_NO_WINDOW", 0x08000000),
            ],
        ));

        // SW_SHOW - ShowWindow nCmdShow parameter
        self.add_group(EnumGroup::new(
            "SW_SHOW",
            &[
                ("SW_HIDE", 0),
                ("SW_SHOWNORMAL", 1),
                ("SW_SHOWMINIMIZED", 2),
                ("SW_SHOWMAXIMIZED", 3),
                ("SW_SHOWNOACTIVATE", 4),
                ("SW_SHOW", 5),
                ("SW_MINIMIZE", 6),
                ("SW_SHOWMINNOACTIVE", 7),
                ("SW_SHOWNA", 8),
                ("SW_RESTORE", 9),
                ("SW_SHOWDEFAULT", 10),
            ],
        ));
    }
}

impl Default for WinConstantsDb {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_single_flag() {
        let db = WinConstantsDb::new();
        assert_eq!(
            db.resolve_in_group("PAGE_PROTECT", 0x40),
            Some("PAGE_EXECUTE_READWRITE".to_string())
        );
    }

    #[test]
    fn test_resolve_combined_flags() {
        let db = WinConstantsDb::new();
        assert_eq!(
            db.resolve_in_group("MEM_ALLOC", 0x3000),
            Some("MEM_RESERVE | MEM_COMMIT".to_string())
        );
    }

    #[test]
    fn test_generic_access() {
        let db = WinConstantsDb::new();
        assert_eq!(
            db.resolve_in_group("GENERIC_ACCESS", 0x80000000),
            Some("GENERIC_READ".to_string())
        );
        assert_eq!(
            db.resolve_in_group("GENERIC_ACCESS", 0xC0000000),
            Some("GENERIC_READ | GENERIC_WRITE".to_string())
        );
    }
}
