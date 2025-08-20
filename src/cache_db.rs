use heed::{
    Database, Env, Error,
    types::{Bytes, Str},
};
use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;
use std::sync::Arc;

use crate::types::Cache;

pub const DB_PROFILES: &str = "profiles";
pub const DB_FOLLOWED: &str = "followed_pubkeys";
pub const DB_RELAYS: &str = "nip65_relays";
pub const DB_TIMELINE: &str = "timeline_posts";
pub const DB_IMAGES: &str = "images";

#[derive(Clone)]
pub struct LmdbCache {
    env: Arc<Env>,
}

impl LmdbCache {
    pub fn new(path: &Path) -> Result<Self, Error> {
        std::fs::create_dir_all(path)?;
        let mut options = heed::EnvOpenOptions::new();
        options.map_size(1024 * 1024 * 1024); // 1 GB
        options.max_dbs(8);
        let env = unsafe { options.open(path)? };

        let mut txn = env.write_txn()?;
        let _: Database<Str, Bytes> = env.create_database(&mut txn, Some(DB_PROFILES))?;
        let _: Database<Str, Bytes> = env.create_database(&mut txn, Some(DB_FOLLOWED))?;
        let _: Database<Str, Bytes> = env.create_database(&mut txn, Some(DB_RELAYS))?;
        let _: Database<Str, Bytes> = env.create_database(&mut txn, Some(DB_TIMELINE))?;
        let _: Database<Str, Bytes> = env.create_database(&mut txn, Some(DB_IMAGES))?;
        txn.commit()?;

        Ok(Self { env: Arc::new(env) })
    }

    pub fn read_cache<T: DeserializeOwned>(
        &self,
        db_name: &str,
        key: &str,
    ) -> Result<Cache<T>, Box<dyn std::error::Error + Send + Sync>> {
        let rtxn = self.env.read_txn()?;
        let db: Database<Str, Bytes> = self
            .env
            .open_database(&rtxn, Some(db_name))?
            .ok_or("database not found")?;
        let data = db.get(&rtxn, key)?.ok_or("key not found")?;

        let cache: Cache<T> = serde_json::from_slice(data)?;

        if cache.is_expired() {
            Err("Cache expired".into())
        } else {
            Ok(cache)
        }
    }

    pub fn write_cache<T: Serialize>(
        &self,
        db_name: &str,
        key: &str,
        data: &T,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut wtxn = self.env.write_txn()?;
        let db: Database<Str, Bytes> = self
            .env
            .open_database(&wtxn, Some(db_name))?
            .ok_or("database not found")?;
        let cache = Cache::new(data);
        let serialized_data = serde_json::to_vec(&cache)?;

        db.put(&mut wtxn, key, &serialized_data)?;
        wtxn.commit()?;

        Ok(())
    }

    pub fn read_image_cache(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        let rtxn = self.env.read_txn()?;
        let db: Database<Str, Bytes> = self
            .env
            .open_database(&rtxn, Some(DB_IMAGES))?
            .ok_or(Error::DatabaseClosing)?;
        match db.get(&rtxn, key)? {
            Some(data) => Ok(Some(data.to_vec())),
            None => Ok(None),
        }
    }

    pub fn write_image_cache(&self, key: &str, data: &[u8]) -> Result<(), Error> {
        let mut wtxn = self.env.write_txn()?;
        let db: Database<Str, Bytes> = self
            .env
            .open_database(&wtxn, Some(DB_IMAGES))?
            .ok_or(Error::DatabaseClosing)?;
        db.put(&mut wtxn, key, data)?;
        wtxn.commit()?;
        Ok(())
    }
}
