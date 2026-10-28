/* Simulate SV39 page-table walks for 4 KiB pages and aligned 2 MiB leaves. */

use std::collections::HashMap;

pub const PAGE_SIZE: usize = 4096;
pub const PT_ENTRIES: usize = 512;
pub const PTE_V: u64 = 1 << 0;
pub const PTE_R: u64 = 1 << 1;
pub const PTE_W: u64 = 1 << 2;
pub const PTE_X: u64 = 1 << 3;
const PPN_SHIFT: u32 = 10;
const PPN_MASK: u64 = (1 << 44) - 1;

#[derive(Clone)]
pub struct PageTableNode {
    pub entries: [u64; PT_ENTRIES],
}

impl PageTableNode {
    pub fn new() -> Self {
        Self {
            entries: [0; PT_ENTRIES],
        }
    }
}

impl Default for PageTableNode {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Sv39PageTable {
    nodes: HashMap<u64, PageTableNode>,
    pub root_ppn: u64,
    next_ppn: u64,
}

#[derive(Debug, PartialEq)]
pub enum TranslateResult {
    Ok(u64),
    PageFault,
}

impl Sv39PageTable {
    pub fn new() -> Self {
        let mut pt = Self {
            nodes: HashMap::new(),
            root_ppn: 0x80000,
            next_ppn: 0x80001,
        };
        pt.nodes.insert(pt.root_ppn, PageTableNode::new());
        pt
    }

    fn alloc_node(&mut self) -> u64 {
        let ppn = self.next_ppn;
        self.next_ppn += 1;
        self.nodes.insert(ppn, PageTableNode::new());
        ppn
    }

    pub fn extract_vpn(va: u64, level: usize) -> usize {
        assert!(level < 3, "SV39 has three page-table levels");
        ((va >> (12 + level * 9)) & 0x1ff) as usize
    }

    fn child_node(&mut self, parent: u64, index: usize) -> u64 {
        let pte = self.nodes[&parent].entries[index];
        if pte & PTE_V != 0 {
            // A leaf maps memory; it must never be followed as a page-table pointer.
            assert_eq!(
                pte & (PTE_R | PTE_W | PTE_X),
                0,
                "mapping overlaps an existing superpage"
            );
            return (pte >> PPN_SHIFT) & PPN_MASK;
        }
        let child = self.alloc_node();
        self.nodes.get_mut(&parent).unwrap().entries[index] = (child << PPN_SHIFT) | PTE_V;
        child
    }

    pub fn map_page(&mut self, va: u64, pa: u64, flags: u64) {
        let mut current = self.root_ppn;
        for level in (1..=2).rev() {
            current = self.child_node(current, Self::extract_vpn(va, level));
        }
        self.nodes.get_mut(&current).unwrap().entries[Self::extract_vpn(va, 0)] =
            (((pa >> 12) & PPN_MASK) << PPN_SHIFT) | (flags & 0x3ff);
    }

    pub fn translate(&self, va: u64) -> TranslateResult {
        let mut current = self.root_ppn;
        for level in (0..=2).rev() {
            let Some(node) = self.nodes.get(&current) else {
                return TranslateResult::PageFault;
            };
            let pte = node.entries[Self::extract_vpn(va, level)];
            if pte & PTE_V == 0 || (pte & PTE_W != 0 && pte & PTE_R == 0) || pte >> 54 != 0 {
                return TranslateResult::PageFault;
            }
            let ppn = (pte >> PPN_SHIFT) & PPN_MASK;
            if pte & (PTE_R | PTE_W | PTE_X) != 0 {
                // A level-1/2 leaf replaces lower VPN fields with the VA's offset.
                let offset_mask = (1u64 << (12 + level * 9)) - 1;
                let base = ppn << 12;
                if base & offset_mask != 0 {
                    return TranslateResult::PageFault;
                }
                return TranslateResult::Ok(base | (va & offset_mask));
            }
            current = ppn;
        }
        TranslateResult::PageFault
    }

    pub fn map_superpage(&mut self, va: u64, pa: u64, flags: u64) {
        let mega_size = (PAGE_SIZE * PT_ENTRIES) as u64;
        assert_eq!(va % mega_size, 0, "va must be 2MB-aligned");
        assert_eq!(pa % mega_size, 0, "pa must be 2MB-aligned");
        let current = self.child_node(self.root_ppn, Self::extract_vpn(va, 2));
        self.nodes.get_mut(&current).unwrap().entries[Self::extract_vpn(va, 1)] =
            (((pa >> 12) & PPN_MASK) << PPN_SHIFT) | (flags & 0x3ff);
    }
}

impl Default for Sv39PageTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod edge_tests {
    use super::*;

    #[test]
    fn superpage_offsets_and_neighboring_small_pages() {
        let mut pt = Sv39PageTable::new();
        pt.map_superpage(0x200000, 0x80200000, PTE_V | PTE_R);
        pt.map_page(0x400000, 0x90000000, PTE_V | PTE_R);
        assert_eq!(pt.translate(0x3fffff), TranslateResult::Ok(0x803fffff));
        assert_eq!(pt.translate(0x400abc), TranslateResult::Ok(0x90000abc));
        assert_eq!(pt.translate(0x1fffff), TranslateResult::PageFault);
    }

    #[test]
    fn malformed_leaf_and_missing_table_fault() {
        let mut pt = Sv39PageTable::new();
        pt.map_superpage(0, 0x80000000, PTE_V | PTE_R);
        let child = pt.nodes[&pt.root_ppn].entries[0] >> PPN_SHIFT;
        pt.nodes.get_mut(&child).unwrap().entries[0] |= 1 << PPN_SHIFT;
        assert_eq!(pt.translate(0), TranslateResult::PageFault);
        pt.nodes.get_mut(&child).unwrap().entries[0] = (0x12345 << PPN_SHIFT) | PTE_V;
        assert_eq!(pt.translate(0), TranslateResult::PageFault);
        pt.map_page(0x400000, 0x90000000, PTE_V | PTE_W);
        assert_eq!(pt.translate(0x400000), TranslateResult::PageFault);
        pt.map_page(0x500000, 0x90000000, PTE_V);
        assert_eq!(pt.translate(0x500000), TranslateResult::PageFault);
    }

    #[test]
    #[should_panic(expected = "mapping overlaps an existing superpage")]
    fn a_leaf_is_not_followed_as_a_table_pointer() {
        let mut pt = Sv39PageTable::new();
        pt.map_superpage(0, 0x80000000, PTE_V | PTE_R);
        pt.map_page(0x1000, 0x90000000, PTE_V | PTE_R);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_vpn() {
        // VA = 0x0000_003F_FFFF_F000 (最大的 39 位地址的页边界)
        // VPN[2] = 0xFF (bits 38:30)
        // VPN[1] = 0x1FF (bits 29:21)
        // VPN[0] = 0x1FF (bits 20:12)
        let va: u64 = 0x7FFFFFF000;
        assert_eq!(Sv39PageTable::extract_vpn(va, 2), 0x1FF);
        assert_eq!(Sv39PageTable::extract_vpn(va, 1), 0x1FF);
        assert_eq!(Sv39PageTable::extract_vpn(va, 0), 0x1FF);
    }

    #[test]
    fn test_extract_vpn_simple() {
        // VA = 0x00000000 + page 1 = 0x1000
        // VPN[2] = 0, VPN[1] = 0, VPN[0] = 1
        let va: u64 = 0x1000;
        assert_eq!(Sv39PageTable::extract_vpn(va, 2), 0);
        assert_eq!(Sv39PageTable::extract_vpn(va, 1), 0);
        assert_eq!(Sv39PageTable::extract_vpn(va, 0), 1);
    }

    #[test]
    fn test_extract_vpn_level2() {
        // VPN[2] = 1 means bit 30 set -> VA >= 0x40000000
        let va: u64 = 0x40000000;
        assert_eq!(Sv39PageTable::extract_vpn(va, 2), 1);
        assert_eq!(Sv39PageTable::extract_vpn(va, 1), 0);
        assert_eq!(Sv39PageTable::extract_vpn(va, 0), 0);
    }

    #[test]
    fn test_map_and_translate_single() {
        let mut pt = Sv39PageTable::new();
        // 映射：VA 0x1000 -> PA 0x80001000
        pt.map_page(0x1000, 0x80001000, PTE_V | PTE_R);

        let result = pt.translate(0x1000);
        assert_eq!(result, TranslateResult::Ok(0x80001000));
    }

    #[test]
    fn test_translate_with_offset() {
        let mut pt = Sv39PageTable::new();
        pt.map_page(0x2000, 0x90000000, PTE_V | PTE_R | PTE_W);

        // 访问 VA 0x2ABC -> PA 应为 0x90000ABC
        let result = pt.translate(0x2ABC);
        assert_eq!(result, TranslateResult::Ok(0x90000ABC));
    }

    #[test]
    fn test_translate_page_fault() {
        let pt = Sv39PageTable::new();
        assert_eq!(pt.translate(0x1000), TranslateResult::PageFault);
    }

    #[test]
    fn test_multiple_mappings() {
        let mut pt = Sv39PageTable::new();
        pt.map_page(0x0000_1000, 0x8000_1000, PTE_V | PTE_R);
        pt.map_page(0x0000_2000, 0x8000_5000, PTE_V | PTE_R | PTE_W);
        pt.map_page(0x0040_0000, 0x9000_0000, PTE_V | PTE_R);

        assert_eq!(pt.translate(0x1234), TranslateResult::Ok(0x80001234));
        assert_eq!(pt.translate(0x2000), TranslateResult::Ok(0x80005000));
        assert_eq!(pt.translate(0x400100), TranslateResult::Ok(0x90000100));
    }

    #[test]
    fn test_map_overwrite() {
        let mut pt = Sv39PageTable::new();
        pt.map_page(0x1000, 0x80001000, PTE_V | PTE_R);
        assert_eq!(pt.translate(0x1000), TranslateResult::Ok(0x80001000));

        pt.map_page(0x1000, 0x90002000, PTE_V | PTE_R);
        assert_eq!(pt.translate(0x1000), TranslateResult::Ok(0x90002000));
    }

    #[test]
    fn test_superpage_mapping() {
        let mut pt = Sv39PageTable::new();
        // 2MB 大页映射：VA 0x200000 -> PA 0x80200000
        pt.map_superpage(0x200000, 0x80200000, PTE_V | PTE_R | PTE_W);

        // 大页内不同偏移都应命中
        assert_eq!(pt.translate(0x200000), TranslateResult::Ok(0x80200000));
        assert_eq!(pt.translate(0x200ABC), TranslateResult::Ok(0x80200ABC));
        assert_eq!(pt.translate(0x2FF000), TranslateResult::Ok(0x802FF000));
    }

    #[test]
    fn test_superpage_and_normal_coexist() {
        let mut pt = Sv39PageTable::new();
        // 大页映射在第一个 2MB 区域
        pt.map_superpage(0x0, 0x80000000, PTE_V | PTE_R);
        // 普通页在不同的 VPN[2] 区域
        pt.map_page(0x40000000, 0x90001000, PTE_V | PTE_R);

        assert_eq!(pt.translate(0x100), TranslateResult::Ok(0x80000100));
        assert_eq!(pt.translate(0x40000000), TranslateResult::Ok(0x90001000));
    }
}
