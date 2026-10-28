/* Simulate an ASID-tagged FIFO TLB and refill it from a simple page table. */

#[derive(Clone, Debug)]
pub struct TlbEntry {
    pub valid: bool,
    pub asid: u16,
    pub vpn: u64,
    pub ppn: u64,
    pub flags: u64,
}

impl TlbEntry {
    pub fn empty() -> Self {
        Self {
            valid: false,
            asid: 0,
            vpn: 0,
            ppn: 0,
            flags: 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct TlbStats {
    pub hits: u64,
    pub misses: u64,
}

impl TlbStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

pub struct Tlb {
    entries: Vec<TlbEntry>,
    capacity: usize,
    fifo_ptr: usize,
    pub stats: TlbStats,
}

impl Tlb {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: vec![TlbEntry::empty(); capacity],
            capacity,
            fifo_ptr: 0,
            stats: TlbStats::default(),
        }
    }

    pub fn lookup(&mut self, vpn: u64, asid: u16) -> Option<u64> {
        if let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.valid && entry.vpn == vpn && entry.asid == asid)
        {
            self.stats.hits += 1;
            Some(entry.ppn)
        } else {
            self.stats.misses += 1;
            None
        }
    }

    pub fn insert(&mut self, vpn: u64, ppn: u64, asid: u16, flags: u64) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.valid && entry.vpn == vpn && entry.asid == asid)
        {
            entry.ppn = ppn;
            entry.flags = flags;
            return;
        }
        if self.capacity == 0 {
            return;
        }
        self.entries[self.fifo_ptr] = TlbEntry {
            valid: true,
            asid,
            vpn,
            ppn,
            flags,
        };
        self.fifo_ptr = (self.fifo_ptr + 1) % self.capacity;
    }

    pub fn flush_all(&mut self) {
        for entry in &mut self.entries {
            entry.valid = false;
        }
    }

    pub fn flush_by_vpn(&mut self, vpn: u64) {
        for entry in &mut self.entries {
            if entry.vpn == vpn {
                entry.valid = false;
            }
        }
    }

    pub fn flush_by_asid(&mut self, asid: u16) {
        for entry in &mut self.entries {
            if entry.asid == asid {
                entry.valid = false;
            }
        }
    }

    pub fn valid_count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.valid).count()
    }
}

pub struct PageMapping {
    pub vpn: u64,
    pub ppn: u64,
    pub flags: u64,
}

pub struct Mmu {
    pub tlb: Tlb,
    page_table: Vec<(u16, PageMapping)>,
    pub current_asid: u16,
}

impl Mmu {
    pub fn new(tlb_capacity: usize) -> Self {
        Self {
            tlb: Tlb::new(tlb_capacity),
            page_table: Vec::new(),
            current_asid: 0,
        }
    }

    pub fn add_mapping(&mut self, asid: u16, vpn: u64, ppn: u64, flags: u64) {
        self.page_table
            .push((asid, PageMapping { vpn, ppn, flags }));
    }

    pub fn switch_asid(&mut self, new_asid: u16) {
        self.current_asid = new_asid;
    }

    pub fn translate(&mut self, vpn: u64) -> Option<u64> {
        if let Some(ppn) = self.tlb.lookup(vpn, self.current_asid) {
            return Some(ppn);
        }
        let (_, mapping) = self
            .page_table
            .iter()
            .find(|(asid, mapping)| *asid == self.current_asid && mapping.vpn == vpn)?;
        self.tlb
            .insert(vpn, mapping.ppn, self.current_asid, mapping.flags);
        Some(mapping.ppn)
    }
}

#[cfg(test)]
mod edge_tests {
    use super::*;

    #[test]
    fn updating_an_entry_does_not_change_fifo_order() {
        let mut tlb = Tlb::new(2);
        tlb.insert(1, 10, 0, 3);
        tlb.insert(2, 20, 0, 3);
        tlb.insert(1, 11, 0, 7);
        tlb.insert(3, 30, 0, 3);
        assert_eq!(tlb.lookup(1, 0), None);
        assert_eq!(tlb.lookup(2, 0), Some(20));
        assert_eq!(tlb.lookup(3, 0), Some(30));
    }

    #[test]
    fn a_zero_capacity_tlb_bypasses_caching() {
        let mut mmu = Mmu::new(0);
        mmu.add_mapping(0, 1, 10, 7);
        assert_eq!(mmu.translate(1), Some(10));
        assert_eq!(mmu.translate(1), Some(10));
        assert_eq!(mmu.tlb.stats.hits, 0);
        assert_eq!(mmu.tlb.stats.misses, 2);
        assert_eq!(mmu.tlb.valid_count(), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ──────── TLB 基础测试 ────────

    #[test]
    fn test_tlb_empty_lookup() {
        let mut tlb = Tlb::new(4);
        assert_eq!(tlb.lookup(0x100, 0), None);
        assert_eq!(tlb.stats.misses, 1);
        assert_eq!(tlb.stats.hits, 0);
    }

    #[test]
    fn test_tlb_insert_and_lookup() {
        let mut tlb = Tlb::new(4);
        tlb.insert(0x100, 0x200, 1, 0x7);
        assert_eq!(tlb.lookup(0x100, 1), Some(0x200));
        assert_eq!(tlb.stats.hits, 1);
    }

    #[test]
    fn test_tlb_asid_isolation() {
        let mut tlb = Tlb::new(4);
        tlb.insert(0x100, 0x200, 1, 0x7);
        tlb.insert(0x100, 0x300, 2, 0x7);

        // 同一 VPN，不同 ASID 应返回不同 PPN
        assert_eq!(tlb.lookup(0x100, 1), Some(0x200));
        assert_eq!(tlb.lookup(0x100, 2), Some(0x300));
    }

    #[test]
    fn test_tlb_miss_wrong_asid() {
        let mut tlb = Tlb::new(4);
        tlb.insert(0x100, 0x200, 1, 0x7);

        // ASID 不匹配应该 miss
        assert_eq!(tlb.lookup(0x100, 99), None);
        assert_eq!(tlb.stats.misses, 1);
    }

    #[test]
    fn test_tlb_fifo_eviction() {
        let mut tlb = Tlb::new(2); // 只有 2 个槽位
        tlb.insert(0x10, 0x20, 0, 0x7);
        tlb.insert(0x30, 0x40, 0, 0x7);
        // TLB 满了，再插入应该淘汰最先插入的
        tlb.insert(0x50, 0x60, 0, 0x7);

        // 0x10 应该被淘汰
        assert_eq!(tlb.lookup(0x10, 0), None);
        // 0x30 和 0x50 应该还在
        assert_eq!(tlb.lookup(0x30, 0), Some(0x40));
        assert_eq!(tlb.lookup(0x50, 0), Some(0x60));
    }

    #[test]
    fn test_tlb_update_existing() {
        let mut tlb = Tlb::new(4);
        tlb.insert(0x100, 0x200, 1, 0x3);
        tlb.insert(0x100, 0x999, 1, 0x7); // 更新同一条目

        assert_eq!(tlb.lookup(0x100, 1), Some(0x999));
        assert_eq!(tlb.valid_count(), 1); // 不应该多出一条
    }

    #[test]
    fn test_tlb_valid_count() {
        let mut tlb = Tlb::new(4);
        assert_eq!(tlb.valid_count(), 0);

        tlb.insert(0x1, 0x10, 0, 0x7);
        assert_eq!(tlb.valid_count(), 1);

        tlb.insert(0x2, 0x20, 0, 0x7);
        assert_eq!(tlb.valid_count(), 2);
    }

    // ──────── TLB 刷新测试 ────────

    #[test]
    fn test_flush_all() {
        let mut tlb = Tlb::new(4);
        tlb.insert(0x1, 0x10, 0, 0x7);
        tlb.insert(0x2, 0x20, 1, 0x7);
        tlb.insert(0x3, 0x30, 2, 0x7);
        assert_eq!(tlb.valid_count(), 3);

        tlb.flush_all();
        assert_eq!(tlb.valid_count(), 0);
        assert_eq!(tlb.lookup(0x1, 0), None);
    }

    #[test]
    fn test_flush_by_vpn() {
        let mut tlb = Tlb::new(4);
        tlb.insert(0x100, 0x200, 1, 0x7);
        tlb.insert(0x100, 0x300, 2, 0x7); // 同 VPN 不同 ASID
        tlb.insert(0x999, 0x400, 1, 0x7);

        tlb.flush_by_vpn(0x100);

        // VPN=0x100 的两条都应被刷掉
        assert_eq!(tlb.lookup(0x100, 1), None);
        assert_eq!(tlb.lookup(0x100, 2), None);
        // VPN=0x999 不受影响
        assert_eq!(tlb.lookup(0x999, 1), Some(0x400));
    }

    #[test]
    fn test_flush_by_asid() {
        let mut tlb = Tlb::new(4);
        tlb.insert(0x1, 0x10, 1, 0x7);
        tlb.insert(0x2, 0x20, 1, 0x7);
        tlb.insert(0x3, 0x30, 2, 0x7);

        tlb.flush_by_asid(1);

        // ASID=1 的条目被刷掉
        assert_eq!(tlb.lookup(0x1, 1), None);
        assert_eq!(tlb.lookup(0x2, 1), None);
        // ASID=2 不受影响
        assert_eq!(tlb.lookup(0x3, 2), Some(0x30));
    }

    #[test]
    fn test_flush_by_vpn_then_reinsert() {
        let mut tlb = Tlb::new(4);
        tlb.insert(0x100, 0x200, 1, 0x7);
        tlb.flush_by_vpn(0x100);
        assert_eq!(tlb.lookup(0x100, 1), None);

        // 重新插入后应该能找到
        tlb.insert(0x100, 0x500, 1, 0x7);
        assert_eq!(tlb.lookup(0x100, 1), Some(0x500));
    }

    // ──────── MMU 集成测试 ────────

    #[test]
    fn test_mmu_basic_translate() {
        let mut mmu = Mmu::new(4);
        mmu.current_asid = 1;
        mmu.add_mapping(1, 0x100, 0x200, 0x7);

        // 第一次：TLB miss，走页表
        let ppn = mmu.translate(0x100);
        assert_eq!(ppn, Some(0x200));
        assert_eq!(mmu.tlb.stats.misses, 1);
        assert_eq!(mmu.tlb.stats.hits, 0);

        // 第二次：TLB hit
        let ppn = mmu.translate(0x100);
        assert_eq!(ppn, Some(0x200));
        assert_eq!(mmu.tlb.stats.hits, 1);
    }

    #[test]
    fn test_mmu_page_fault() {
        let mut mmu = Mmu::new(4);
        mmu.current_asid = 1;
        // 没有添加任何映射
        assert_eq!(mmu.translate(0x999), None);
    }

    #[test]
    fn test_mmu_asid_switch() {
        let mut mmu = Mmu::new(4);
        mmu.add_mapping(1, 0x100, 0x200, 0x7);
        mmu.add_mapping(2, 0x100, 0x300, 0x7);

        mmu.switch_asid(1);
        assert_eq!(mmu.translate(0x100), Some(0x200));

        mmu.switch_asid(2);
        assert_eq!(mmu.translate(0x100), Some(0x300));
    }

    #[test]
    fn test_mmu_flush_on_asid_switch() {
        let mut mmu = Mmu::new(4);
        mmu.add_mapping(1, 0x100, 0x200, 0x7);
        mmu.add_mapping(2, 0x100, 0x300, 0x7);

        mmu.switch_asid(1);
        assert_eq!(mmu.translate(0x100), Some(0x200));

        // 切换 ASID 后刷新 TLB 中旧 ASID 的条目
        mmu.switch_asid(2);
        mmu.tlb.flush_by_asid(1);

        // 应该 TLB miss 然后走页表
        let old_misses = mmu.tlb.stats.misses;
        assert_eq!(mmu.translate(0x100), Some(0x300));
        assert_eq!(mmu.tlb.stats.misses, old_misses + 1);
    }

    #[test]
    fn test_mmu_hit_rate() {
        let mut mmu = Mmu::new(4);
        mmu.current_asid = 0;
        mmu.add_mapping(0, 0x1, 0x10, 0x7);

        // 第一次 miss
        mmu.translate(0x1);
        // 后续 9 次 hit
        for _ in 0..9 {
            mmu.translate(0x1);
        }

        assert_eq!(mmu.tlb.stats.hits, 9);
        assert_eq!(mmu.tlb.stats.misses, 1);
        let rate = mmu.tlb.stats.hit_rate();
        assert!(
            (rate - 0.9).abs() < 1e-9,
            "hit rate should be 0.9, got {rate}"
        );
    }

    #[test]
    fn test_mmu_thrashing() {
        // TLB 只有 2 个槽，但交替访问 3 个不同的页
        let mut mmu = Mmu::new(2);
        mmu.current_asid = 0;
        mmu.add_mapping(0, 0x1, 0x10, 0x7);
        mmu.add_mapping(0, 0x2, 0x20, 0x7);
        mmu.add_mapping(0, 0x3, 0x30, 0x7);

        // 访问 1, 2, 3, 1, 2, 3 — 由于容量只有 2，会持续 miss（thrashing）
        for vpn in [1, 2, 3, 1, 2, 3] {
            mmu.translate(vpn);
        }

        // 前两次一定 miss（冷启动），第三次也 miss（淘汰 vpn=1），
        // 第四次 vpn=1 被淘汰了所以 miss ... 全部 miss
        assert_eq!(mmu.tlb.stats.misses, 6);
        assert_eq!(mmu.tlb.stats.hits, 0);
    }
}
