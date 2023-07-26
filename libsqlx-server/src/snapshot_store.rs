use std::mem::size_of;
use std::path::PathBuf;

use bytemuck::{Pod, Zeroable};
use heed::BytesDecode;
use heed_types::{ByteSlice, CowType, SerdeBincode};
use libsqlx::FrameNo;
use serde::{Deserialize, Serialize};
use tokio::task::block_in_place;
use uuid::Uuid;

use crate::meta::DatabaseId;

#[derive(Clone, Copy, Zeroable, Pod, Debug)]
#[repr(transparent)]
struct BEU64([u8; size_of::<u64>()]);

impl From<u64> for BEU64 {
    fn from(value: u64) -> Self {
        Self(value.to_be_bytes())
    }
}

impl From<BEU64> for u64 {
    fn from(value: BEU64) -> Self {
        u64::from_be_bytes(value.0)
    }
}

#[derive(Clone, Copy, Zeroable, Pod, Debug)]
#[repr(C)]
struct SnapshotKey {
    database_id: DatabaseId,
    start_frame_no: BEU64,
    end_frame_no: BEU64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub snapshot_id: Uuid,
}

pub struct SnapshotStore {
    env: heed::Env,
    database: heed::Database<CowType<SnapshotKey>, SerdeBincode<SnapshotMeta>>,
    db_path: PathBuf,
}

impl SnapshotStore {
    const SNAPSHOT_STORE_NAME: &str = "snapshot-store-db";

    pub fn new(db_path: PathBuf, env: heed::Env) -> color_eyre::Result<Self> {
        let mut txn = env.write_txn().unwrap();
        let database = env.create_database(&mut txn, Some(Self::SNAPSHOT_STORE_NAME))?;
        txn.commit()?;

        Ok(Self {
            database,
            db_path,
            env,
        })
    }

    pub fn register(
        &self,
        txn: &mut heed::RwTxn,
        database_id: DatabaseId,
        start_frame_no: FrameNo,
        end_frame_no: FrameNo,
        snapshot_id: Uuid,
    ) {
        let key = SnapshotKey {
            database_id,
            start_frame_no: start_frame_no.into(),
            end_frame_no: end_frame_no.into(),
        };

        let data = SnapshotMeta { snapshot_id };

        block_in_place(|| self.database.put(txn, &key, &data).unwrap());
    }

    /// Locate a snapshot for `database_id` that contains `frame_no`
    pub fn locate(&self, database_id: DatabaseId, frame_no: FrameNo) -> Option<SnapshotMeta> {
        let txn = self.env.read_txn().unwrap();
        // Snapshot keys being lexicographically ordered, looking for the first key less than of
        // equal to (db_id, frame_no, FrameNo::MAX) will always return the entry we're looking for
        // if it exists.
        let key = SnapshotKey {
            database_id,
            start_frame_no: frame_no.into(),
            end_frame_no: u64::MAX.into(),
        };

        match self
            .database
            .get_lower_than_or_equal_to(&txn, &key)
            .transpose()?
        {
            Ok((key, v)) => {
                if key.database_id != database_id {
                    return None;
                } else if frame_no >= key.start_frame_no.into()
                    && frame_no <= key.end_frame_no.into()
                {
                    return Some(v);
                } else {
                    None
                }
            }
            Err(_) => todo!(),
        }
    }
}
    }
}
