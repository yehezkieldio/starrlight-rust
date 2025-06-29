use chrono::{DateTime, Duration, Utc};
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::StarredError;
use crate::models::Repository;

/// Page-specific cache metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageMetadata {
    pub page_number: usize,
    pub fetched_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub etag: Option<String>,
    pub cursor: Option<String>,
    pub next_cursor: Option<String>,
    pub has_next_page: bool,
    pub repository_count: usize,
    pub api_rate_limit_remaining: Option<u32>,
    pub api_rate_limit_reset: Option<DateTime<Utc>>,
}

/// Session-wide cache metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub username: String,
    pub cache_key: String,
    pub query_parameters: QueryParameters,
    pub created_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
    pub total_pages: usize,
    pub total_repositories: usize,
    pub pages: HashMap<usize, PageMetadata>,
}

/// Query parameters for cache key generation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryParameters {
    pub limit: Option<usize>,
    pub topic_stargazer_count_limit: i32,
    pub include_private: bool,
}

/// Cached page data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPageData {
    pub metadata: PageMetadata,
    pub repositories: Vec<Repository>,
}

/// Legacy cache data for backward compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyCacheMetadata {
    pub username: String,
    pub fetched_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub etag: Option<String>,
    pub total_count: usize,
    pub cache_key: String,
    pub api_rate_limit_remaining: Option<u32>,
    pub api_rate_limit_reset: Option<DateTime<Utc>>,
}

/// Legacy cached repository data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyCachedData {
    pub metadata: LegacyCacheMetadata,
    pub repositories: Vec<Repository>,
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub cache_dir: PathBuf,
    pub expiry_hours: u64,
    pub force_refresh: bool,
    pub skip_validation: bool,
}

/// Cache manager for GitHub stars data with per-page caching
pub struct CacheManager {
    config: CacheConfig,
}

impl CacheManager {
    /// Create a new cache manager with the given configuration
    pub fn new(config: CacheConfig) -> Result<Self, Box<StarredError>> {
        // Ensure cache directory exists
        if !config.cache_dir.exists() {
            fs::create_dir_all(&config.cache_dir).map_err(|e| {
                Box::new(StarredError {
                    message: format!("Failed to create cache directory: {}", e),
                })
            })?;
        }

        Ok(CacheManager { config })
    }

    /// Generate a cache key based on username and configuration
    pub fn generate_cache_key(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_limit: i32,
        include_private: bool,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(username.as_bytes());
        hasher.update(limit.unwrap_or(0).to_string().as_bytes());
        hasher.update(topic_limit.to_string().as_bytes());
        hasher.update(include_private.to_string().as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Get the metadata file path for a cache session
    fn get_metadata_file_path(&self, cache_key: &str) -> PathBuf {
        self.config.cache_dir.join(format!("{}-metadata.json", cache_key))
    }

    /// Get the page file path for a specific page
    fn get_page_file_path(&self, cache_key: &str, page_number: usize) -> PathBuf {
        self.config.cache_dir.join(format!("{}-page-{:03}.json", cache_key, page_number))
    }

    /// Get the lock file path for a given cache key
    fn get_lock_file_path(&self, cache_key: &str) -> PathBuf {
        self.config.cache_dir.join(format!("{}.lock", cache_key))
    }

    /// Get session metadata for a cache key
    pub fn get_session_metadata(&self, cache_key: &str) -> Result<Option<SessionMetadata>, Box<StarredError>> {
        let metadata_file = self.get_metadata_file_path(cache_key);
        if !metadata_file.exists() {
            return Ok(None);
        }

        let lock_file = self.get_lock_file_path(cache_key);
        let _lock = self.acquire_lock(&lock_file)?;

        let mut file = File::open(&metadata_file).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to open metadata file: {}", e),
            })
        })?;

        let mut contents = String::new();
        file.read_to_string(&mut contents).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to read metadata file: {}", e),
            })
        })?;

        let metadata: SessionMetadata = serde_json::from_str(&contents).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to parse metadata file: {}", e),
            })
        })?;

        Ok(Some(metadata))
    }

    /// Update session metadata
    pub fn update_session_metadata(&self, metadata: &SessionMetadata) -> Result<(), Box<StarredError>> {
        let metadata_file = self.get_metadata_file_path(&metadata.cache_key);
        let lock_file = self.get_lock_file_path(&metadata.cache_key);
        let temp_file = metadata_file.with_extension("tmp");

        let _lock = self.acquire_lock(&lock_file)?;

        let mut file = File::create(&temp_file).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to create temporary metadata file: {}", e),
            })
        })?;

        let json_data = serde_json::to_string_pretty(metadata).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to serialize metadata: {}", e),
            })
        })?;

        file.write_all(json_data.as_bytes()).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to write metadata: {}", e),
            })
        })?;

        file.sync_all().map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to sync metadata file: {}", e),
            })
        })?;

        fs::rename(&temp_file, &metadata_file).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to move metadata file: {}", e),
            })
        })?;

        Ok(())
    }

    /// Check if a specific page is valid
    pub fn is_page_valid(&self, cache_key: &str, page_number: usize) -> Result<bool, Box<StarredError>> {
        if self.config.force_refresh {
            return Ok(false);
        }

        let page_file = self.get_page_file_path(cache_key, page_number);
        if !page_file.exists() {
            return Ok(false);
        }

        if self.config.skip_validation {
            return Ok(true);
        }

        // Get session metadata to check page validity
        if let Some(session_metadata) = self.get_session_metadata(cache_key)? {
            if let Some(page_metadata) = session_metadata.pages.get(&page_number) {
                let now = Utc::now();
                return Ok(page_metadata.expires_at > now);
            }
        }

        Ok(false)
    }

    /// Read cached page data
    pub fn read_page_data(&self, cache_key: &str, page_number: usize) -> Result<Option<CachedPageData>, Box<StarredError>> {
        if !self.is_page_valid(cache_key, page_number)? {
            return Ok(None);
        }

        let page_file = self.get_page_file_path(cache_key, page_number);
        let lock_file = self.get_lock_file_path(cache_key);

        let _lock = self.acquire_lock(&lock_file)?;

        let mut file = File::open(&page_file).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to open page file: {}", e),
            })
        })?;

        let mut contents = String::new();
        file.read_to_string(&mut contents).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to read page file: {}", e),
            })
        })?;

        let page_data: CachedPageData = serde_json::from_str(&contents).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to parse page file: {}", e),
            })
        })?;

        Ok(Some(page_data))
    }

    /// Write cached page data
    pub fn write_page_data(&self, cache_key: &str, page_data: &CachedPageData) -> Result<(), Box<StarredError>> {
        let page_file = self.get_page_file_path(cache_key, page_data.metadata.page_number);
        let lock_file = self.get_lock_file_path(cache_key);
        let temp_file = page_file.with_extension("tmp");

        let _lock = self.acquire_lock(&lock_file)?;

        let mut file = File::create(&temp_file).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to create temporary page file: {}", e),
            })
        })?;

        let json_data = serde_json::to_string_pretty(page_data).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to serialize page data: {}", e),
            })
        })?;

        file.write_all(json_data.as_bytes()).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to write page data: {}", e),
            })
        })?;

        file.sync_all().map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to sync page file: {}", e),
            })
        })?;

        fs::rename(&temp_file, &page_file).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to move page file: {}", e),
            })
        })?;

        Ok(())
    }

    /// Get all cached repositories for a query, reconstructing from pages
    pub fn get_cached_repositories(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_limit: i32,
        include_private: bool,
    ) -> Result<Option<Vec<Repository>>, Box<StarredError>> {
        let cache_key = self.generate_cache_key(username, limit, topic_limit, include_private);

        // First check if we have session metadata
        let session_metadata = match self.get_session_metadata(&cache_key)? {
            Some(metadata) => metadata,
            None => {
                // Try legacy cache format for backward compatibility
                return self.get_legacy_cached_repositories(username, limit, topic_limit, include_private);
            }
        };

        let mut all_repositories = Vec::new();
        let mut repositories_collected = 0;

        // Collect repositories from all valid pages
        for page_number in 1..=session_metadata.total_pages {
            if let Some(page_data) = self.read_page_data(&cache_key, page_number)? {
                for repo in page_data.repositories {
                    if let Some(limit) = limit {
                        if repositories_collected >= limit {
                            return Ok(Some(all_repositories));
                        }
                    }
                    all_repositories.push(repo);
                    repositories_collected += 1;
                }
            } else {
                // If any page is invalid, we need to refresh
                return Ok(None);
            }
        }

        Ok(Some(all_repositories))
    }

    /// Get cached repositories using legacy format (backward compatibility)
    fn get_legacy_cached_repositories(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_limit: i32,
        include_private: bool,
    ) -> Result<Option<Vec<Repository>>, Box<StarredError>> {
        let cache_key = self.generate_cache_key(username, limit, topic_limit, include_private);
        let cache_file = self.config.cache_dir.join(format!("{}.json", cache_key));

        if !cache_file.exists() {
            return Ok(None);
        }

        let lock_file = self.get_lock_file_path(&cache_key);
        let _lock = self.acquire_lock(&lock_file)?;

        let mut file = File::open(&cache_file).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to open legacy cache file: {}", e),
            })
        })?;

        let mut contents = String::new();
        file.read_to_string(&mut contents).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to read legacy cache file: {}", e),
            })
        })?;

        let cached_data: LegacyCachedData = serde_json::from_str(&contents).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to parse legacy cache file: {}", e),
            })
        })?;

        // Check if legacy cache is still valid
        let now = Utc::now();
        if !self.config.force_refresh && !self.config.skip_validation && cached_data.metadata.expires_at < now {
            return Ok(None);
        }

        Ok(Some(cached_data.repositories))
    }

    /// Get the next page that needs to be fetched (for incremental fetching)
    pub fn get_next_page_to_fetch(&self, cache_key: &str) -> Result<Option<(usize, Option<String>)>, Box<StarredError>> {
        if let Some(session_metadata) = self.get_session_metadata(cache_key)? {
            for page_number in 1..=session_metadata.total_pages {
                if !self.is_page_valid(cache_key, page_number)? {
                    if let Some(page_metadata) = session_metadata.pages.get(&page_number) {
                        return Ok(Some((page_number, page_metadata.cursor.clone())));
                    }
                }
            }

            // Check if we need to fetch more pages
            if let Some(last_page_metadata) = session_metadata.pages.get(&session_metadata.total_pages) {
                if last_page_metadata.has_next_page {
                    return Ok(Some((session_metadata.total_pages + 1, last_page_metadata.next_cursor.clone())));
                }
            }
        }

        Ok(None)
    }

    /// Cache a page of repositories
    pub fn cache_page(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_limit: i32,
        include_private: bool,
        page_number: usize,
        repositories: Vec<Repository>,
        cursor: Option<String>,
        next_cursor: Option<String>,
        has_next_page: bool,
        etag: Option<String>,
        rate_limit_remaining: Option<u32>,
        rate_limit_reset: Option<DateTime<Utc>>,
    ) -> Result<(), Box<StarredError>> {
        let cache_key = self.generate_cache_key(username, limit, topic_limit, include_private);
        let now = Utc::now();
        let expires_at = now + Duration::hours(self.config.expiry_hours as i64);

        // Create page metadata
        let page_metadata = PageMetadata {
            page_number,
            fetched_at: now,
            expires_at,
            etag,
            cursor,
            next_cursor,
            has_next_page,
            repository_count: repositories.len(),
            api_rate_limit_remaining: rate_limit_remaining,
            api_rate_limit_reset: rate_limit_reset,
        };

        // Create page data
        let page_data = CachedPageData {
            metadata: page_metadata.clone(),
            repositories,
        };

        // Write the page data
        self.write_page_data(&cache_key, &page_data)?;

        // Update session metadata
        let mut session_metadata = self.get_session_metadata(&cache_key)?
            .unwrap_or_else(|| SessionMetadata {
                username: username.to_string(),
                cache_key: cache_key.clone(),
                query_parameters: QueryParameters {
                    limit,
                    topic_stargazer_count_limit: topic_limit,
                    include_private,
                },
                created_at: now,
                last_updated: now,
                total_pages: 0,
                total_repositories: 0,
                pages: HashMap::new(),
            });

        session_metadata.last_updated = now;
        session_metadata.total_pages = session_metadata.total_pages.max(page_number);
        session_metadata.pages.insert(page_number, page_metadata);

        // Recalculate total repositories
        session_metadata.total_repositories = session_metadata.pages.values()
            .map(|p| p.repository_count)
            .sum();

        self.update_session_metadata(&session_metadata)?;

        Ok(())
    }

    /// Get cached ETag for a specific page (for conditional requests)
    pub fn get_cached_page_etag(&self, cache_key: &str, page_number: usize) -> Result<Option<String>, Box<StarredError>> {
        if let Some(session_metadata) = self.get_session_metadata(cache_key)? {
            if let Some(page_metadata) = session_metadata.pages.get(&page_number) {
                return Ok(page_metadata.etag.clone());
            }
        }
        Ok(None)
    }

    /// Get cached ETag for the first page (for legacy compatibility)
    pub fn get_cached_etag(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_limit: i32,
        include_private: bool,
    ) -> Result<Option<String>, Box<StarredError>> {
        let cache_key = self.generate_cache_key(username, limit, topic_limit, include_private);

        // Try new format first
        if let Some(etag) = self.get_cached_page_etag(&cache_key, 1)? {
            return Ok(Some(etag));
        }

        // Fall back to legacy format
        let cache_file = self.config.cache_dir.join(format!("{}.json", cache_key));
        if !cache_file.exists() {
            return Ok(None);
        }

        let lock_file = self.get_lock_file_path(&cache_key);
        let _lock = self.acquire_lock(&lock_file)?;

        let mut file = File::open(&cache_file).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to open legacy cache file: {}", e),
            })
        })?;

        let mut contents = String::new();
        file.read_to_string(&mut contents).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to read legacy cache file: {}", e),
            })
        })?;

        let cached_data: LegacyCachedData = serde_json::from_str(&contents).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to parse legacy cache file: {}", e),
            })
        })?;

        Ok(cached_data.metadata.etag)
    }

    /// Legacy method for backward compatibility
    pub fn cache_repositories(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_limit: i32,
        include_private: bool,
        repositories: Vec<Repository>,
        etag: Option<String>,
        rate_limit_remaining: Option<u32>,
        rate_limit_reset: Option<DateTime<Utc>>,
    ) -> Result<(), Box<StarredError>> {
        // Convert to page-based caching
        let page_size = 100; // Standard GraphQL page size
        let mut page_number = 1;
        let mut start_index = 0;

        while start_index < repositories.len() {
            let end_index = (start_index + page_size).min(repositories.len());
            let page_repositories = repositories[start_index..end_index].to_vec();
            let has_next_page = end_index < repositories.len();

            self.cache_page(
                username,
                limit,
                topic_limit,
                include_private,
                page_number,
                page_repositories,
                None, // cursor not available in legacy format
                None, // next_cursor not available
                has_next_page,
                if page_number == 1 { etag.clone() } else { None },
                rate_limit_remaining,
                rate_limit_reset,
            )?;

            start_index = end_index;
            page_number += 1;
        }

        Ok(())
    }

    /// Clear cache for a specific user/configuration
    pub fn clear_cache(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_limit: i32,
        include_private: bool,
    ) -> Result<(), Box<StarredError>> {
        let cache_key = self.generate_cache_key(username, limit, topic_limit, include_private);

        // Remove metadata file
        let metadata_file = self.get_metadata_file_path(&cache_key);
        if metadata_file.exists() {
            fs::remove_file(&metadata_file).map_err(|e| {
                Box::new(StarredError {
                    message: format!("Failed to remove metadata file: {}", e),
                })
            })?;
        }

        // Remove all page files
        if let Some(session_metadata) = self.get_session_metadata(&cache_key)? {
            for page_number in 1..=session_metadata.total_pages {
                let page_file = self.get_page_file_path(&cache_key, page_number);
                if page_file.exists() {
                    fs::remove_file(&page_file).map_err(|e| {
                        Box::new(StarredError {
                            message: format!("Failed to remove page file: {}", e),
                        })
                    })?;
                }
            }
        }

        // Remove legacy cache file if it exists
        let legacy_cache_file = self.config.cache_dir.join(format!("{}.json", cache_key));
        if legacy_cache_file.exists() {
            fs::remove_file(&legacy_cache_file).map_err(|e| {
                Box::new(StarredError {
                    message: format!("Failed to remove legacy cache file: {}", e),
                })
            })?;
        }

        // Remove lock file
        let lock_file = self.get_lock_file_path(&cache_key);
        if lock_file.exists() {
            fs::remove_file(&lock_file).map_err(|e| {
                Box::new(StarredError {
                    message: format!("Failed to remove lock file: {}", e),
                })
            })?;
        }

        Ok(())
    }

    /// Clear expired pages for a specific cache key
    pub fn clear_expired_pages(&self, cache_key: &str) -> Result<(), Box<StarredError>> {
        if let Some(mut session_metadata) = self.get_session_metadata(cache_key)? {
            let now = Utc::now();
            let mut pages_to_remove = Vec::new();

            for (page_number, page_metadata) in &session_metadata.pages {
                if page_metadata.expires_at < now {
                    pages_to_remove.push(*page_number);
                }
            }

            for page_number in pages_to_remove {
                let page_file = self.get_page_file_path(cache_key, page_number);
                if page_file.exists() {
                    fs::remove_file(&page_file).map_err(|e| {
                        Box::new(StarredError {
                            message: format!("Failed to remove expired page file: {}", e),
                        })
                    })?;
                }
                session_metadata.pages.remove(&page_number);
            }

            // Update total pages and repositories count
            session_metadata.total_pages = session_metadata.pages.keys().max().copied().unwrap_or(0);
            session_metadata.total_repositories = session_metadata.pages.values()
                .map(|p| p.repository_count)
                .sum();

            if !session_metadata.pages.is_empty() {
                self.update_session_metadata(&session_metadata)?;
            } else {
                // If no pages left, remove metadata file
                let metadata_file = self.get_metadata_file_path(cache_key);
                if metadata_file.exists() {
                    fs::remove_file(&metadata_file).map_err(|e| {
                        Box::new(StarredError {
                            message: format!("Failed to remove empty metadata file: {}", e),
                        })
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Clear all cache files
    pub fn clear_all_cache(&self) -> Result<(), Box<StarredError>> {
        let cache_dir = &self.config.cache_dir;

        if !cache_dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(cache_dir).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to read cache directory: {}", e),
            })
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                Box::new(StarredError {
                    message: format!("Failed to read cache entry: {}", e),
                })
            })?;

            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "json" || ext == "lock" || ext == "tmp" {
                        fs::remove_file(&path).map_err(|e| {
                            Box::new(StarredError {
                                message: format!("Failed to remove cache file: {}", e),
                            })
                        })?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Get cache statistics with per-page information
    pub fn get_cache_stats(&self) -> Result<CacheStats, Box<StarredError>> {
        let cache_dir = &self.config.cache_dir;
        let mut stats = CacheStats {
            total_files: 0,
            total_size: 0,
            valid_entries: 0,
            expired_entries: 0,
            oldest_entry: None,
            newest_entry: None,
            total_sessions: 0,
            total_pages: 0,
            total_repositories: 0,
        };

        if !cache_dir.exists() {
            return Ok(stats);
        }

        let entries = fs::read_dir(cache_dir).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to read cache directory: {}", e),
            })
        })?;

        let now = Utc::now();
        let mut session_keys = std::collections::HashSet::new();

        for entry in entries {
            let entry = entry.map_err(|e| {
                Box::new(StarredError {
                    message: format!("Failed to read cache entry: {}", e),
                })
            })?;

            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if let Some(ext) = path.extension() {
                        if ext == "json" || ext == "lock" || ext == "tmp" {
                            stats.total_files += 1;

                            if let Ok(metadata) = fs::metadata(&path) {
                                stats.total_size += metadata.len();
                            }
                        }

                        if ext == "json" {
                            if file_name.ends_with("-metadata.json") {
                                // Session metadata file
                                let cache_key = file_name.replace("-metadata.json", "");
                                session_keys.insert(cache_key.clone());

                                if let Ok(Some(session_metadata)) = self.get_session_metadata(&cache_key) {
                                    stats.total_sessions += 1;
                                    stats.total_pages += session_metadata.total_pages;
                                    stats.total_repositories += session_metadata.total_repositories;

                                    let created_at = session_metadata.created_at;
                                    if stats.oldest_entry.is_none() || Some(created_at) < stats.oldest_entry {
                                        stats.oldest_entry = Some(created_at);
                                    }
                                    if stats.newest_entry.is_none() || Some(created_at) > stats.newest_entry {
                                        stats.newest_entry = Some(created_at);
                                    }

                                    // Count valid vs expired pages
                                    for page_metadata in session_metadata.pages.values() {
                                        if page_metadata.expires_at > now {
                                            stats.valid_entries += 1;
                                        } else {
                                            stats.expired_entries += 1;
                                        }
                                    }
                                }
                            } else if file_name.contains("-page-") {
                                // Individual page file - counted via session metadata
                                continue;
                            } else {
                                // Legacy format
                                let cache_key = file_name.replace(".json", "");
                                if !session_keys.contains(&cache_key) {
                                    // This is a legacy cache file
                                    if let Ok(legacy_data) = self.get_legacy_cached_repositories(&cache_key, None, 0, false) {
                                        if legacy_data.is_some() {
                                            stats.total_sessions += 1;

                                            // Try to read legacy metadata for timing info
                                            let legacy_cache_file = self.config.cache_dir.join(format!("{}.json", cache_key));
                                            if let Ok(mut file) = File::open(&legacy_cache_file) {
                                                let mut contents = String::new();
                                                if file.read_to_string(&mut contents).is_ok() {
                                                    if let Ok(cached_data) = serde_json::from_str::<LegacyCachedData>(&contents) {
                                                        let fetched_at = cached_data.metadata.fetched_at;

                                                        if stats.oldest_entry.is_none() || Some(fetched_at) < stats.oldest_entry {
                                                            stats.oldest_entry = Some(fetched_at);
                                                        }
                                                        if stats.newest_entry.is_none() || Some(fetched_at) > stats.newest_entry {
                                                            stats.newest_entry = Some(fetched_at);
                                                        }

                                                        if cached_data.metadata.expires_at > now {
                                                            stats.valid_entries += 1;
                                                        } else {
                                                            stats.expired_entries += 1;
                                                        }

                                                        stats.total_repositories += cached_data.metadata.total_count;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(stats)
    }

    /// Acquire a file lock for atomic operations
    fn acquire_lock(&self, lock_file_path: &Path) -> Result<File, Box<StarredError>> {
        let lock_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(lock_file_path)
            .map_err(|e| {
                Box::new(StarredError {
                    message: format!("Failed to create lock file: {}", e),
                })
            })?;

        lock_file.lock_exclusive().map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to acquire file lock: {}", e),
            })
        })?;

        Ok(lock_file)
    }
}

/// Cache statistics for monitoring with per-page information
#[derive(Debug)]
pub struct CacheStats {
    pub total_files: usize,
    pub total_size: u64,
    pub valid_entries: usize,
    pub expired_entries: usize,
    pub oldest_entry: Option<DateTime<Utc>>,
    pub newest_entry: Option<DateTime<Utc>>,
    pub total_sessions: usize,
    pub total_pages: usize,
    pub total_repositories: usize,
}

impl CacheStats {
    pub fn format_size(&self) -> String {
        let size = self.total_size as f64;
        if size < 1024.0 {
            format!("{} B", size)
        } else if size < 1024.0 * 1024.0 {
            format!("{:.1} KB", size / 1024.0)
        } else if size < 1024.0 * 1024.0 * 1024.0 {
            format!("{:.1} MB", size / (1024.0 * 1024.0))
        } else {
            format!("{:.1} GB", size / (1024.0 * 1024.0 * 1024.0))
        }
    }
}

/// Get default cache directory path
pub fn get_default_cache_dir() -> Result<PathBuf, Box<StarredError>> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| {
            Box::new(StarredError {
                message: "Unable to determine cache directory".to_string(),
            })
        })?
        .join("starrlight");

    Ok(cache_dir)
}
