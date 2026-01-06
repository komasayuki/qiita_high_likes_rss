use crate::error::AppError;
use crate::qiita_api::QiitaItem;
use chrono::{DateTime, Duration, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

// フィードを永続化して再実行時に差分を保持する
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredItem {
    pub key: String,
    pub item_id: Option<String>,
    pub title: String,
    pub link: String,
    pub summary: Option<String>,
    pub published: Option<String>,
    pub updated: Option<String>,
    pub author_name: Option<String>,
    pub likes_count: u32,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StateFile {
    pub items: Vec<StoredItem>,
}

#[derive(Debug, Default)]
pub struct StateStore {
    pub items: HashMap<String, StoredItem>,
}

impl StateStore {
    pub fn load(path: &Path) -> Result<Self, AppError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)
            .map_err(|e| AppError::feed(format!("state 読み込み失敗: {}", e)))?;
        let file: StateFile = serde_json::from_str(&content)
            .map_err(|e| AppError::feed(format!("state パース失敗: {}", e)))?;
        let mut store = StateStore::default();
        for item in file.items {
            store.items.insert(item.key.clone(), item);
        }
        Ok(store)
    }

    pub fn merge_from_feed(&mut self, items: &[QiitaItem], now: DateTime<Utc>) -> usize {
        let mut merged = 0;
        for item in items {
            let Some(key) = item_key(item) else {
                eprintln!("item の識別子が不足しているためスキップ: title={}", item.title);
                continue;
            };
            let stored = StoredItem {
                key: key.clone(),
                item_id: item.item_id.clone(),
                title: item.title.clone(),
                link: item.link.clone(),
                summary: item.summary.clone(),
                published: item.published.clone(),
                updated: item.updated.clone(),
                author_name: item.author_name.clone(),
                likes_count: item.likes_count,
                last_seen: now.to_rfc3339(),
            };
            self.items.insert(key, stored);
            merged += 1;
        }
        merged
    }

    pub fn prune(&mut self, now: DateTime<Utc>, max_days: u32, max_items: usize) {
        let cutoff = now - Duration::days(max_days as i64);
        self.items.retain(|_, item| {
            if let Some(dt) = parse_datetime(&item.last_seen) {
                dt >= cutoff
            } else {
                false
            }
        });

        if self.items.len() > max_items {
            let mut list: Vec<_> = self.items.values().cloned().collect();
            list.sort_by_key(|item| parse_datetime(&item.last_seen).unwrap_or_else(|| now));
            let keep = list.split_off(list.len().saturating_sub(max_items));
            self.items = keep
                .into_iter()
                .map(|item| (item.key.clone(), item))
                .collect();
        }
    }

    pub fn to_sorted_vec(&self) -> Vec<StoredItem> {
        let mut list: Vec<_> = self.items.values().cloned().collect();
        list.sort_by_key(|item| item.key.clone());
        list
    }

    pub fn save(&self, path: &Path) -> Result<(), AppError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AppError::feed(format!("state ディレクトリ作成失敗: {}", e)))?;
        }
        let file = StateFile {
            items: self.to_sorted_vec(),
        };
        let json = serde_json::to_string_pretty(&file)
            .map_err(|e| AppError::feed(format!("state 書き込み失敗: {}", e)))?;
        fs::write(path, json)
            .map_err(|e| AppError::feed(format!("state 書き込み失敗: {}", e)))?;
        Ok(())
    }
}

pub fn item_key(item: &QiitaItem) -> Option<String> {
    if let Some(id) = &item.item_id {
        return Some(id.clone());
    }
    Some(item.link.clone())
}

fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

pub fn select_updated_time(item: &StoredItem) -> Option<DateTime<FixedOffset>> {
    let updated = item
        .updated
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok());
    let published = item
        .published
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok());

    match (updated, published) {
        (Some(a), Some(b)) => Some(if a > b { a } else { b }),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}
