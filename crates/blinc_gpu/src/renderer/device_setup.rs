//! Adapter and device selection: the features and limits to request, and the on-disk pipeline cache.

use super::GpuRenderer;
use super::config::env_u64;

/// Feature set to request from `adapter.request_device`. Picks
/// `TEXTURE_COMPRESSION_BC` when the adapter advertises it; the
/// renderer probes the active feature set afterward via
/// `device.features()` to decide whether to upload BC-encoded
/// material textures or fall back to Rgba8.
///
/// Safe to call on wgpu backends that don't support BC (WebGL2,
/// iOS/Metal-without-BC): the adapter's features query simply
/// returns `false` and no feature is requested. Callers that
/// need to detect this downstream should query
/// `GpuRenderer::has_texture_compression_bc()`.
pub(super) fn requested_device_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    let mut features = wgpu::Features::empty();
    let available = adapter.features();
    if available.contains(wgpu::Features::TEXTURE_COMPRESSION_BC) {
        features |= wgpu::Features::TEXTURE_COMPRESSION_BC;
    }
    // Persisted pipeline cache: on mobile, rebuilding all render pipelines
    // from scratch every launch is the dominant cold-start cost (measured
    // ~925ms for the core pipelines on a Pixel 10 Pro). A serialized
    // `PipelineCache` collapses that on every launch after the first.
    if available.contains(wgpu::Features::PIPELINE_CACHE) {
        features |= wgpu::Features::PIPELINE_CACHE;
    }
    features
}

// On-disk pipeline-cache header. Bump the version when the layout changes.
pub(super) const PIPELINE_CACHE_MAGIC: &[u8; 4] = b"BLPC";

pub(super) const PIPELINE_CACHE_VERSION: u32 = 1;

// magic(4) + version(4) + vendor(4) + device(4) + driver_hash(8)
// + data_len(8) + data_hash(8)
pub(super) const PIPELINE_CACHE_HEADER_LEN: usize = 40;

pub(super) fn stable_hash_str(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

pub(super) fn stable_hash_bytes(b: &[u8]) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    h.write(b);
    h.finish()
}

pub(super) fn pipeline_cache_driver_key(info: &wgpu::AdapterInfo) -> u64 {
    stable_hash_str(&format!("{}|{}", info.driver, info.driver_info))
}

/// Load previously-serialized `PipelineCache` data, validated against the
/// current adapter. Android drivers do minimal integrity checking on cache
/// blobs, so we gate the data behind a header (magic, layout version,
/// vendor/device IDs, driver string hash, and a hash of the payload) and
/// hand nothing to the driver unless every field matches. `fallback: true`
/// on the descriptor is the second safety net.
pub(super) fn load_pipeline_cache_data(
    path: &std::path::Path,
    info: &wgpu::AdapterInfo,
) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < PIPELINE_CACHE_HEADER_LEN || &bytes[0..4] != PIPELINE_CACHE_MAGIC {
        return None;
    }
    let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let u64_at = |o: usize| u64::from_le_bytes(bytes[o..o + 8].try_into().unwrap());
    if u32_at(4) != PIPELINE_CACHE_VERSION
        || u32_at(8) != info.vendor
        || u32_at(12) != info.device
        || u64_at(16) != pipeline_cache_driver_key(info)
    {
        return None;
    }
    let data_len = u64_at(24) as usize;
    let data_hash = u64_at(32);
    let data = bytes.get(PIPELINE_CACHE_HEADER_LEN..PIPELINE_CACHE_HEADER_LEN + data_len)?;
    if stable_hash_bytes(data) != data_hash {
        return None;
    }
    Some(data.to_vec())
}

/// Serialize `PipelineCache` data with the validation header, keyed to the
/// current adapter. Best-effort: write to a temp file and rename so a
/// half-written cache is never read on the next launch.
pub(super) fn save_pipeline_cache_data(
    path: &std::path::Path,
    info: &wgpu::AdapterInfo,
    data: &[u8],
) {
    let mut out = Vec::with_capacity(PIPELINE_CACHE_HEADER_LEN + data.len());
    out.extend_from_slice(PIPELINE_CACHE_MAGIC);
    out.extend_from_slice(&PIPELINE_CACHE_VERSION.to_le_bytes());
    out.extend_from_slice(&info.vendor.to_le_bytes());
    out.extend_from_slice(&info.device.to_le_bytes());
    out.extend_from_slice(&pipeline_cache_driver_key(info).to_le_bytes());
    out.extend_from_slice(&(data.len() as u64).to_le_bytes());
    out.extend_from_slice(&stable_hash_bytes(data).to_le_bytes());
    out.extend_from_slice(data);
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, &out).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

pub(super) fn device_required_limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
    // Default wgpu limits include `max_buffer_size = 256 MiB`.
    // This is conservative and may be smaller than what the hardware supports.
    //
    // If you want to raise this limit (e.g. for large path buffers), set:
    //   BLINC_WGPU_MAX_BUFFER_MB=512
    // The value is clamped to the adapter-supported maximum.
    let supported = adapter.limits();

    // On wasm32, use the adapter's own supported limits directly
    // instead of requesting wgpu defaults. Different browsers support
    // different subsets of WebGPU — Safari/Firefox may report 0 for
    // compute workgroups or storage buffer binding size. Requesting
    // any limit above what the adapter supports causes device creation
    // to fail. Using the adapter's limits verbatim is always safe.
    #[cfg(target_arch = "wasm32")]
    let mut limits = supported.clone();
    #[cfg(not(target_arch = "wasm32"))]
    let mut limits = wgpu::Limits::default();

    if let Some(mib) = env_u64("BLINC_WGPU_MAX_BUFFER_MB") {
        let requested = mib.saturating_mul(1024 * 1024);
        let clamped = requested.min(supported.max_buffer_size);
        limits.max_buffer_size = clamped;

        tracing::info!(
            "wgpu limits override: max_buffer_size={} MiB (requested {} MiB, supported {} MiB)",
            limits.max_buffer_size / (1024 * 1024),
            mib,
            supported.max_buffer_size / (1024 * 1024)
        );
    } else {
        tracing::debug!(
            "wgpu limits: max_buffer_size={} MiB (supported {} MiB)",
            limits.max_buffer_size / (1024 * 1024),
            supported.max_buffer_size / (1024 * 1024)
        );
    }

    limits
}
