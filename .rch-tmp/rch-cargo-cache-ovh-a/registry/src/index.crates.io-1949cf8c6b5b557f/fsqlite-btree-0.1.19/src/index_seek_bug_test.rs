#[cfg(test)]
mod tests {
    use crate::cursor::{BtCursor, MemPageStore};
    use crate::traits::BtreeCursorOps;
    use fsqlite_types::PageNumber;
    use fsqlite_types::cx::Cx;

    #[test]
    fn test_index_seek_bug() {
        let cx = Cx::new();

        let root = PageNumber::new(2).unwrap();
        let store = MemPageStore::with_empty_index(root, 4096);
        let mut cursor = BtCursor::new(store, root, 4096, false);

        // Insert a middle value
        cursor.index_insert(&cx, b"M").unwrap();

        // Let's force a split? Actually, we can just insert A, Z, etc.
        for i in 0..100 {
            let key = format!("KEY_{:03}", i);
            cursor.index_insert(&cx, key.as_bytes()).unwrap();
        }

        // If the bug exists, inserting something that falls off a leaf but has a successor will panic.
        // Or we can just insert keys in order to force it.
        // Let's try inserting sequentially, then inserting one in the middle that falls at the end of a leaf

        // A better way is to do many inserts and see if it fails.
        let mut keys = Vec::new();
        for i in 0..250 {
            keys.push(format!("K{:03}", i));
        }
        for k in &keys {
            cursor.index_insert(&cx, k.as_bytes()).unwrap();
        }
    }
}
