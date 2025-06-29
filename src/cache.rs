use chrono::{DateTime, Duration, Utc};
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::StarredError;
use crate::models::Repository;

/// Cache metadata for tracking freshness and ETags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub username: String,
    pub fetched_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub etag: Option<String>,
    pub total_count: usize,
    pub cache_key: String,
    pub api_rate_limit_remaining: Option<u32>,
    pub api_rate_limit_reset: Option<DateTime<Utc>>,
}

/// Cached repository data with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedData {
    pub metadata: CacheMetadata,
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

/// Cache manager for GitHub stars data
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

    /// Get the cache file path for a given cache key
    fn get_cache_file_path(&self, cache_key: &str) -> PathBuf {
        self.config.cache_dir.join(format!("{}.json", cache_key))
    }

    /// Get the lock file path for a given cache key
    fn get_lock_file_path(&self, cache_key: &str) -> PathBuf {
        self.config.cache_dir.join(format!("{}.lock", cache_key))
    }

    /// Check if cached data exists and is valid
    pub fn is_cache_valid(&self, cache_key: &str) -> Result<bool, Box<StarredError>> {
        if self.config.force_refresh {
            return Ok(false);
        }

        let cache_file = self.get_cache_file_path(cache_key);
        if !cache_file.exists() {
            return Ok(false);
        }

        if self.config.skip_validation {
            return Ok(true);
        }

        // Read and validate cache metadata
        let cached_data = self.read_cache_file(cache_key)?;
        let now = Utc::now();

        // Check if cache has expired
        if cached_data.metadata.expires_at < now {
            return Ok(false);
        }

        Ok(true)
    }

    /// Read cached data from file with atomic operations
    pub fn read_cache_file(&self, cache_key: &str) -> Result<CachedData, Box<StarredError>> {
        let cache_file = self.get_cache_file_path(cache_key);
        let lock_file = self.get_lock_file_path(cache_key);

        // Acquire lock for reading
        let _lock = self.acquire_lock(&lock_file)?;

        let mut file = File::open(&cache_file).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to open cache file: {}", e),
            })
        })?;

        let mut contents = String::new();
        file.read_to_string(&mut contents).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to read cache file: {}", e),
            })
        })?;

        serde_json::from_str(&contents).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to parse cache file: {}", e),
            })
        })
    }

    /// Write cached data to file with atomic operations
    pub fn write_cache_file(
        &self,
        cache_key: &str,
        cached_data: &CachedData,
    ) -> Result<(), Box<StarredError>> {
        let cache_file = self.get_cache_file_path(cache_key);
        let lock_file = self.get_lock_file_path(cache_key);
        let temp_file = cache_file.with_extension("tmp");

        // Acquire lock for writing
        let _lock = self.acquire_lock(&lock_file)?;

        // Write to temporary file first
        let mut file = File::create(&temp_file).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to create temporary file: {}", e),
            })
        })?;

        let json_data = serde_json::to_string_pretty(cached_data).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to serialize cache data: {}", e),
            })
        })?;

        file.write_all(json_data.as_bytes()).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to write cache data: {}", e),
            })
        })?;

        file.sync_all().map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to sync cache file: {}", e),
            })
        })?;

        // Atomically replace the cache file
        fs::rename(&temp_file, &cache_file).map_err(|e| {
            Box::new(StarredError {
                message: format!("Failed to move cache file: {}", e),
            })
        })?;

        Ok(())
    }

    /// Get cached repositories if valid
    pub fn get_cached_repositories(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_limit: i32,
        include_private: bool,
    ) -> Result<Option<Vec<Repository>>, Box<StarredError>> {
        let cache_key = self.generate_cache_key(username, limit, topic_limit, include_private);

        if !self.is_cache_valid(&cache_key)? {
            return Ok(None);
        }

        let cached_data = self.read_cache_file(&cache_key)?;
        Ok(Some(cached_data.repositories))
    }

    /// Get cached ETag for conditional requests
    pub fn get_cached_etag(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_limit: i32,
        include_private: bool,
    ) -> Result<Option<String>, Box<StarredError>> {
        let cache_key = self.generate_cache_key(username, limit, topic_limit, include_private);
        let cache_file = self.get_cache_file_path(&cache_key);

        if !cache_file.exists() {
            return Ok(None);
        }

        let cached_data = self.read_cache_file(&cache_key)?;
        Ok(cached_data.metadata.etag)
    }

    /// Cache repositories with metadata
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
        let cache_key = self.generate_cache_key(username, limit, topic_limit, include_private);
        let now = Utc::now();
        let expires_at = now + Duration::hours(self.config.expiry_hours as i64);

        let metadata = CacheMetadata {
            username: username.to_string(),
            fetched_at: now,
            expires_at,
            etag,
            total_count: repositories.len(),
            cache_key: cache_key.clone(),
            api_rate_limit_remaining: rate_limit_remaining,
            api_rate_limit_reset: rate_limit_reset,
        };

        let cached_data = CachedData {
            metadata,
            repositories,
        };

        self.write_cache_file(&cache_key, &cached_data)
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
        let cache_file = self.get_cache_file_path(&cache_key);
        let lock_file = self.get_lock_file_path(&cache_key);

        if cache_file.exists() {
            fs::remove_file(&cache_file).map_err(|e| {
                Box::new(StarredError {
                    message: format!("Failed to remove cache file: {}", e),
                })
            })?;
        }

        if lock_file.exists() {
            fs::remove_file(&lock_file).map_err(|e| {
                Box::new(StarredError {
                    message: format!("Failed to remove lock file: {}", e),
                })
            })?;
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

    /// Get cache statistics
    pub fn get_cache_stats(&self) -> Result<CacheStats, Box<StarredError>> {
        let cache_dir = &self.config.cache_dir;
        let mut stats = CacheStats {
            total_files: 0,
            total_size: 0,
            valid_entries: 0,
            expired_entries: 0,
            oldest_entry: None,
            newest_entry: None,
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

        for entry in entries {
            let entry = entry.map_err(|e| {
                Box::new(StarredError {
                    message: format!("Failed to read cache entry: {}", e),
                })
            })?;

            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "json" {
                        stats.total_files += 1;

                        if let Ok(metadata) = fs::metadata(&path) {
                            stats.total_size += metadata.len();
                        }

                        // Try to read cache metadata
                        if let Ok(cache_key) = path.file_stem().and_then(|s| s.to_str()).ok_or("Invalid file name") {
                            if let Ok(cached_data) = self.read_cache_file(cache_key) {
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

/// Cache statistics for monitoring
#[derive(Debug)]
pub struct CacheStats {
    pub total_files: usize,
    pub total_size: u64,
    pub valid_entries: usize,
    pub expired_entries: usize,
    pub oldest_entry: Option<DateTime<Utc>>,
    pub newest_entry: Option<DateTime<Utc>>,
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
