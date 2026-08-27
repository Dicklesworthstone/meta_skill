#[cfg(test)]
mod tests {
    use crate::write_coordinator::{WriteCoordinator, CoordinatorMode, CompatCommitRequest, CompatCommitResponse, CommitWriteSet};
    use fsqlite_types::{TxnToken, TxnId, TxnEpoch, CommitSeq, PageNumber, PageData, Snapshot};
    use std::collections::{HashMap, HashSet};

    fn test_token(id: u64) -> TxnToken {
        TxnToken::new(TxnId::new(id).unwrap(), TxnEpoch::new(0))
    }

    fn test_snapshot(high: u64) -> Snapshot {
        Snapshot {
            high: CommitSeq::new(high),
            schema_epoch: fsqlite_types::SchemaEpoch::new(1),
        }
    }

    fn inline_write_set(pages: &[u32]) -> CommitWriteSet {
        let mut map = HashMap::new();
        for &pgno in pages {
            let mut data = vec![0u8; 4096];
            data[0] = 1; // dummy data
            map.insert(PageNumber::new(pgno).unwrap(), PageData::from_vec(data));
        }
        CommitWriteSet::Inline(map)
    }

    #[test]
    fn test_repro_fcw_loss_on_coordinator_restart() {
        // 1. Start Coordinator 1
        let coord1 = WriteCoordinator::new(CoordinatorMode::Compatibility);
        coord1.acquire_lease(100, 0);

        // 2. T1 and T2 start concurrently at snapshot 0.
        let t1 = test_token(1);
        let t2 = test_token(2);
        let snapshot = test_snapshot(0);

        // 3. T1 commits (writes page 10).
        let req1 = CompatCommitRequest {
            txn: t1,
            mode: crate::core_types::TransactionMode::Concurrent,
            write_set: inline_write_set(&[10]),
            intent_log: Vec::new(),
            page_locks: HashSet::from([PageNumber::new(10).unwrap()]),
            snapshot,
            has_in_rw: false,
            has_out_rw: false,
            wal_fec_r: 0,
        };
        let resp1 = coord1.compat_commit(&req1);
        let CompatCommitResponse::Ok { commit_seq: seq1, .. } = resp1 else {
            panic!("T1 failed to commit");
        };
        assert_eq!(seq1.get(), 1);

        // 4. "Restart" the coordinator (simulate crash/takeover).
        // The new coordinator has no memory of T1's commit.
        let coord2 = WriteCoordinator::new(CoordinatorMode::Compatibility);
        coord2.acquire_lease(200, 10);

        // 5. T2 tries to commit (writes page 10).
        // Since T2 started at snapshot 0, and T1 wrote page 10 at seq 1,
        // T2 SHOULD fail with Conflict.
        let req2 = CompatCommitRequest {
            txn: t2,
            mode: crate::core_types::TransactionMode::Concurrent,
            write_set: inline_write_set(&[10]),
            intent_log: Vec::new(),
            page_locks: HashSet::from([PageNumber::new(10).unwrap()]),
            snapshot, // still snapshot 0!
            has_in_rw: false,
            has_out_rw: false,
            wal_fec_r: 0,
        };

        let resp2 = coord2.compat_commit(&req2);
        
        // This assertion will FAIL if the bug exists.
        // T2 will succeed because coord2 doesn't know about T1.
        if let CompatCommitResponse::Ok { .. } = resp2 {
            panic!("FCW violation! T2 committed despite conflict with T1 (lost on restart)");
        }
    }
}
