/* Translate 32-bit addresses with a single-level table and write permissions. */

pub const PAGE_SIZE: usize = 4096;
pub const PAGE_OFFSET_BITS: u32 = 12;
pub const PTE_VALID: u8 = 1 << 0;
pub const PTE_READ: u8 = 1 << 1;
pub const PTE_WRITE: u8 = 1 << 2;

#[derive(Clone, Copy, Debug)]
pub struct PageTableEntry {
    pub ppn: u32,
    pub flags: u8,
}

#[derive(Debug, PartialEq)]
pub enum TranslateResult {
    Ok(u32),
    PageFault,
    PermissionDenied,
}

pub struct SingleLevelPageTable {
    entries: Vec<Option<PageTableEntry>>,
}

impl SingleLevelPageTable {
    pub fn new(max_pages: usize) -> Self {
        Self {
            entries: vec![None; max_pages],
        }
    }

    pub fn map(&mut self, vpn: usize, ppn: u32, flags: u8) {
        self.entries[vpn] = Some(PageTableEntry { ppn, flags });
    }

    pub fn unmap(&mut self, vpn: usize) {
        if let Some(entry) = self.entries.get_mut(vpn) {
            *entry = None;
        }
    }

    pub fn lookup(&self, vpn: usize) -> Option<&PageTableEntry> {
        self.entries.get(vpn)?.as_ref()
    }

    pub fn translate(&self, va: u32, is_write: bool) -> TranslateResult {
        let Some(entry) = self.lookup(va_to_vpn(va)) else {
            return TranslateResult::PageFault;
        };
        if entry.flags & PTE_VALID == 0 {
            return TranslateResult::PageFault;
        }
        if is_write && entry.flags & PTE_WRITE == 0 {
            return TranslateResult::PermissionDenied;
        }
        TranslateResult::Ok(make_pa(entry.ppn, va_to_offset(va)))
    }
}

pub fn va_to_vpn(va: u32) -> usize {
    (va >> PAGE_OFFSET_BITS) as usize
}

pub fn va_to_offset(va: u32) -> u32 {
    va & ((1 << PAGE_OFFSET_BITS) - 1)
}

pub fn make_pa(ppn: u32, offset: u32) -> u32 {
    (ppn << PAGE_OFFSET_BITS) | va_to_offset(offset)
}

#[cfg(test)]
mod edge_tests {
    use super::*;

    #[test]
    fn out_of_range_and_unmapped_addresses_fault() {
        let mut pt = SingleLevelPageTable::new(1);
        assert!(pt.lookup(1).is_none());
        assert_eq!(pt.translate(u32::MAX, false), TranslateResult::PageFault);
        pt.map(0, 0xfffff, PTE_VALID | PTE_READ | PTE_WRITE);
        assert_eq!(pt.translate(0xfff, true), TranslateResult::Ok(u32::MAX));
        pt.unmap(0);
        assert_eq!(pt.translate(0, false), TranslateResult::PageFault);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_va_decompose() {
        // 虚拟地址 0x12345678
        // VPN = 0x12345, offset = 0x678
        assert_eq!(va_to_vpn(0x12345678), 0x12345);
        assert_eq!(va_to_offset(0x12345678), 0x678);
    }

    #[test]
    fn test_va_decompose_zero() {
        assert_eq!(va_to_vpn(0), 0);
        assert_eq!(va_to_offset(0), 0);
    }

    #[test]
    fn test_va_decompose_page_boundary() {
        // 正好在页边界，offset 应为 0
        assert_eq!(va_to_vpn(0x3000), 3);
        assert_eq!(va_to_offset(0x3000), 0);
    }

    #[test]
    fn test_make_pa() {
        assert_eq!(make_pa(0x80, 0x100), 0x80 * 4096 + 0x100);
        assert_eq!(make_pa(0, 0), 0);
        assert_eq!(make_pa(1, 0), 4096);
    }

    #[test]
    fn test_map_and_lookup() {
        let mut pt = SingleLevelPageTable::new(1024);
        pt.map(5, 100, PTE_VALID | PTE_READ);

        let entry = pt.lookup(5).expect("应该找到映射");
        assert_eq!(entry.ppn, 100);
        assert_eq!(entry.flags, PTE_VALID | PTE_READ);
    }

    #[test]
    fn test_lookup_unmapped() {
        let pt = SingleLevelPageTable::new(1024);
        assert!(pt.lookup(0).is_none());
    }

    #[test]
    fn test_unmap() {
        let mut pt = SingleLevelPageTable::new(1024);
        pt.map(10, 200, PTE_VALID | PTE_READ);
        assert!(pt.lookup(10).is_some());

        pt.unmap(10);
        assert!(pt.lookup(10).is_none());
    }

    #[test]
    fn test_translate_basic() {
        let mut pt = SingleLevelPageTable::new(1024);
        // 虚拟页 1 -> 物理页 0x80
        pt.map(1, 0x80, PTE_VALID | PTE_READ);

        // VA = 页1 + offset 0x100 = 0x1100
        let result = pt.translate(0x1100, false);
        // PA = 0x80 * 4096 + 0x100 = 0x80100
        assert_eq!(result, TranslateResult::Ok(0x80100));
    }

    #[test]
    fn test_translate_page_fault() {
        let pt = SingleLevelPageTable::new(1024);
        assert_eq!(pt.translate(0x5000, false), TranslateResult::PageFault);
    }

    #[test]
    fn test_translate_write_permission() {
        let mut pt = SingleLevelPageTable::new(1024);
        // 只读页
        pt.map(2, 0x90, PTE_VALID | PTE_READ);

        // 读取应成功
        assert_eq!(
            pt.translate(0x2000, false),
            TranslateResult::Ok(0x90 * PAGE_SIZE as u32)
        );
        // 写入应拒绝
        assert_eq!(
            pt.translate(0x2000, true),
            TranslateResult::PermissionDenied
        );
    }

    #[test]
    fn test_translate_writable_page() {
        let mut pt = SingleLevelPageTable::new(1024);
        pt.map(3, 0xA0, PTE_VALID | PTE_READ | PTE_WRITE);

        // 写入可写页应成功
        assert_eq!(
            pt.translate(0x3456, true),
            TranslateResult::Ok(0xA0 * PAGE_SIZE as u32 + 0x456)
        );
    }

    #[test]
    fn test_translate_invalid_entry() {
        let mut pt = SingleLevelPageTable::new(1024);
        // 映射了但 VALID 未置位
        pt.map(4, 0x50, PTE_READ);
        assert_eq!(pt.translate(0x4000, false), TranslateResult::PageFault);
    }

    #[test]
    fn test_multiple_mappings() {
        let mut pt = SingleLevelPageTable::new(1024);
        pt.map(0, 0x10, PTE_VALID | PTE_READ);
        pt.map(1, 0x20, PTE_VALID | PTE_READ | PTE_WRITE);
        pt.map(2, 0x30, PTE_VALID | PTE_READ);

        assert_eq!(pt.translate(0x0FFF, false), TranslateResult::Ok(0x10FFF));
        assert_eq!(pt.translate(0x1000, true), TranslateResult::Ok(0x20000));
        assert_eq!(pt.translate(0x2800, false), TranslateResult::Ok(0x30800));
    }
}
