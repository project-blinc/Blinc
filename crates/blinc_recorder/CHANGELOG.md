# Changelog

## [Unreleased]

### Fixed
- Cached headless paints invalidate after reactive property writes, matching the windowed runner. Changed transforms and other visual properties now appear in cached snapshots and frame-cost probes.
