//! Mesh cache for loaded scenes
//!
//! Provides an LRU cache to avoid reloading the same meshes.

use super::LoadedScene;
use std::collections::HashMap;

/// Statistics about the mesh cache
#[derive(Clone, Debug, Default)]
pub struct CacheStats {
    /// Number of entries in the cache
    pub entries: usize,
    /// Number of cache hits
    pub hits: u64,
    /// Number of cache misses
    pub misses: u64,
    /// Total vertices in cache
    pub total_vertices: usize,
    /// Total triangles in cache
    pub total_triangles: usize,
}

impl CacheStats {
    /// Get the hit rate (0.0 to 1.0)
    pub fn hit_rate(&self) -> f32 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f32 / total as f32
        }
    }
}

/// Entry in the mesh cache
struct CacheEntry {
    scene: LoadedScene,
    access_order: u64,
}

/// LRU cache for loaded mesh scenes
pub struct MeshCache {
    entries: HashMap<String, CacheEntry>,
    max_entries: usize,
    access_counter: u64,
    hits: u64,
    misses: u64,
}

impl MeshCache {
    /// Create a new cache with the specified maximum entries
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_entries,
            access_counter: 0,
            hits: 0,
            misses: 0,
        }
    }

    /// Get a scene from the cache
    pub fn get(&mut self, key: &str) -> Option<&LoadedScene> {
        self.access_counter += 1;

        if let Some(entry) = self.entries.get_mut(key) {
            entry.access_order = self.access_counter;
            self.hits += 1;
            Some(&entry.scene)
        } else {
            self.misses += 1;
            None
        }
    }

    /// Insert a scene into the cache
    pub fn insert(&mut self, key: String, scene: LoadedScene) {
        self.access_counter += 1;

        // Evict if necessary
        while self.entries.len() >= self.max_entries {
            self.evict_lru();
        }

        self.entries.insert(
            key,
            CacheEntry {
                scene,
                access_order: self.access_counter,
            },
        );
    }

    /// Remove and return a scene from the cache
    pub fn remove(&mut self, key: &str) -> Option<LoadedScene> {
        self.entries.remove(key).map(|e| e.scene)
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.entries.clear();
        self.access_counter = 0;
        self.hits = 0;
        self.misses = 0;
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let total_vertices: usize = self.entries.values().map(|e| e.scene.total_vertices()).sum();
        let total_triangles: usize = self.entries.values().map(|e| e.scene.total_triangles()).sum();

        CacheStats {
            entries: self.entries.len(),
            hits: self.hits,
            misses: self.misses,
            total_vertices,
            total_triangles,
        }
    }

    /// Check if a key is in the cache
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get the maximum number of entries
    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// Set the maximum number of entries
    pub fn set_max_entries(&mut self, max: usize) {
        self.max_entries = max;
        while self.entries.len() > self.max_entries {
            self.evict_lru();
        }
    }

    /// Evict the least recently used entry
    fn evict_lru(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        let lru_key = self
            .entries
            .iter()
            .min_by_key(|(_, e)| e.access_order)
            .map(|(k, _)| k.clone());

        if let Some(key) = lru_key {
            self.entries.remove(&key);
        }
    }

    /// Get all cached keys
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }

    /// Preload a scene into the cache
    ///
    /// This is useful for preloading assets during loading screens.
    pub fn preload(&mut self, key: String, scene: LoadedScene) {
        if !self.entries.contains_key(&key) {
            self.insert(key, scene);
        }
    }
}

impl Default for MeshCache {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_scene(name: &str) -> LoadedScene {
        LoadedScene {
            name: name.to_string(),
            source_path: PathBuf::from(name),
            meshes: Vec::new(),
            materials: Vec::new(),
            textures: Vec::new(),
            animations: Vec::new(),
            nodes: Vec::new(),
            root_nodes: Vec::new(),
        }
    }

    #[test]
    fn test_cache_insert_get() {
        let mut cache = MeshCache::new(10);

        cache.insert("test1".to_string(), create_test_scene("test1"));
        cache.insert("test2".to_string(), create_test_scene("test2"));

        assert!(cache.get("test1").is_some());
        assert!(cache.get("test2").is_some());
        assert!(cache.get("test3").is_none());
    }

    #[test]
    fn test_cache_lru_eviction() {
        let mut cache = MeshCache::new(2);

        cache.insert("test1".to_string(), create_test_scene("test1"));
        cache.insert("test2".to_string(), create_test_scene("test2"));

        // Access test1 to make it more recent
        cache.get("test1");

        // Insert test3, should evict test2 (least recently used)
        cache.insert("test3".to_string(), create_test_scene("test3"));

        assert!(cache.get("test1").is_some());
        assert!(cache.get("test2").is_none()); // Evicted
        assert!(cache.get("test3").is_some());
    }

    #[test]
    fn test_cache_stats() {
        let mut cache = MeshCache::new(10);

        cache.insert("test1".to_string(), create_test_scene("test1"));

        cache.get("test1"); // Hit
        cache.get("test1"); // Hit
        cache.get("test2"); // Miss

        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entries, 1);
    }
}
