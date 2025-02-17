// Copyright (C) 2023 Quickwit, Inc.
//
// Quickwit is offered under the AGPL v3.0 and as commercial software.
// For commercial licensing, contact us at hello@quickwit.io.
//
// AGPL:
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <http://www.gnu.org/licenses/>.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use quickwit_proto::ingest::Shard;
use quickwit_proto::types::SourceId;
use quickwit_proto::IndexId;

/// A set of open shards for a given index and source.
#[derive(Debug, Default)]
pub(super) struct ShardTableEntry {
    shards: Vec<Shard>,
    round_robin_idx: AtomicUsize,
}

impl ShardTableEntry {
    /// Creates a new entry and ensures that the shards are open and unique.
    ///
    /// # Panics
    ///
    /// Panics if `shards` is empty after filtering out closed shards and deduplicating by shard ID.
    pub fn new(mut shards: Vec<Shard>) -> Self {
        shards.retain(|shard| shard.is_open());
        shards.sort_unstable_by_key(|shard| shard.shard_id);
        shards.dedup_by_key(|shard| shard.shard_id);

        assert!(!shards.is_empty(), "`shards` should not be empty");

        Self {
            shards,
            round_robin_idx: AtomicUsize::default(),
        }
    }

    /// Returns the next shard in round-robin order.
    pub fn next_shard_round_robin(&self) -> &Shard {
        let shard_idx = self.round_robin_idx.fetch_add(1, Ordering::Relaxed);
        &self.shards[shard_idx % self.shards.len()]
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.shards.len()
    }

    #[cfg(test)]
    pub fn shards(&self) -> &[Shard] {
        &self.shards
    }
}

/// A table of shard entries indexed by index UID and source ID.
#[derive(Debug, Default)]
pub(super) struct ShardTable {
    table: HashMap<(IndexId, SourceId), ShardTableEntry>,
}

impl ShardTable {
    pub fn contains_entry(
        &self,
        index_id: impl Into<IndexId>,
        source_id: impl Into<SourceId>,
    ) -> bool {
        let key = (index_id.into(), source_id.into());
        self.table.contains_key(&key)
    }

    pub fn find_entry(
        &self,
        index_id: impl Into<IndexId>,
        source_id: impl Into<SourceId>,
    ) -> Option<&ShardTableEntry> {
        let key = (index_id.into(), source_id.into());
        self.table.get(&key)
    }

    pub fn insert_shards(
        &mut self,
        index_id: impl Into<IndexId>,
        source_id: impl Into<SourceId>,
        shards: Vec<Shard>,
    ) {
        let key = (index_id.into(), source_id.into());
        self.table.insert(key, ShardTableEntry::new(shards));
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.table.len()
    }
}

#[cfg(test)]
mod tests {
    use quickwit_proto::ingest::ShardState;

    use super::*;

    #[test]
    fn test_shard_table() {
        let mut table = ShardTable::default();
        assert!(!table.contains_entry("test-index", "test-source"));

        table.insert_shards(
            "test-index",
            "test-source",
            vec![
                Shard {
                    index_uid: "test-index:0".to_string(),
                    shard_id: 0,
                    leader_id: "node-0".to_string(),
                    ..Default::default()
                },
                Shard {
                    index_uid: "test-index:0".to_string(),
                    shard_id: 1,
                    leader_id: "node-1".to_string(),
                    ..Default::default()
                },
                Shard {
                    index_uid: "test-index:0".to_string(),
                    shard_id: 0,
                    leader_id: "node-0".to_string(),
                    ..Default::default()
                },
                Shard {
                    index_uid: "test-index:0".to_string(),
                    shard_id: 2,
                    leader_id: "node-2".to_string(),
                    shard_state: ShardState::Closed as i32,
                    ..Default::default()
                },
            ],
        );
        assert!(table.contains_entry("test-index", "test-source"));

        let entry = table.find_entry("test-index", "test-source").unwrap();
        assert_eq!(entry.shards.len(), 2);
        assert_eq!(entry.shards[0].shard_id, 0);
        assert_eq!(entry.shards[1].shard_id, 1);

        assert_eq!(entry.next_shard_round_robin().shard_id, 0);
        assert_eq!(entry.next_shard_round_robin().shard_id, 1);
        assert_eq!(entry.next_shard_round_robin().shard_id, 0);
    }
}
