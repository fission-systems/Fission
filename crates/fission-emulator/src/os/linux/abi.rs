use byteorder::{LittleEndian, WriteBytesExt};

/// Representation of Linux x86_64 `stat` structure (target_stat).
/// This matches the structure expected by guest binaries.
#[derive(Debug, Default)]
pub struct TargetStat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_nlink: u64,
    pub st_mode: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub __pad0: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i64,
    pub st_blocks: i64,
    pub st_atime: u64,
    pub st_atime_nsec: u64,
    pub st_mtime: u64,
    pub st_mtime_nsec: u64,
    pub st_ctime: u64,
    pub st_ctime_nsec: u64,
}

impl TargetStat {
    /// Serializes the struct into a little-endian byte array to be written into guest memory.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(144); // Approx size
        buf.write_u64::<LittleEndian>(self.st_dev).unwrap();
        buf.write_u64::<LittleEndian>(self.st_ino).unwrap();
        buf.write_u64::<LittleEndian>(self.st_nlink).unwrap();
        buf.write_u32::<LittleEndian>(self.st_mode).unwrap();
        buf.write_u32::<LittleEndian>(self.st_uid).unwrap();
        buf.write_u32::<LittleEndian>(self.st_gid).unwrap();
        buf.write_u32::<LittleEndian>(self.__pad0).unwrap();
        buf.write_u64::<LittleEndian>(self.st_rdev).unwrap();
        buf.write_i64::<LittleEndian>(self.st_size).unwrap();
        buf.write_i64::<LittleEndian>(self.st_blksize).unwrap();
        buf.write_i64::<LittleEndian>(self.st_blocks).unwrap();
        buf.write_u64::<LittleEndian>(self.st_atime).unwrap();
        buf.write_u64::<LittleEndian>(self.st_atime_nsec).unwrap();
        buf.write_u64::<LittleEndian>(self.st_mtime).unwrap();
        buf.write_u64::<LittleEndian>(self.st_mtime_nsec).unwrap();
        buf.write_u64::<LittleEndian>(self.st_ctime).unwrap();
        buf.write_u64::<LittleEndian>(self.st_ctime_nsec).unwrap();
        
        // Pad to exactly 144 bytes if needed, but the struct size in x64 is 144 bytes.
        buf.resize(144, 0);
        buf
    }
}
