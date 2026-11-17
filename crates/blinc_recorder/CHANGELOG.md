# Changelog

## [Unreleased]

### Changed
- Continuous Calendar checks verify that retired month pages and day cells leave the selector index after their exit animations.
- Pixel and retention regressions cover rapid Calendar swipes and hover ownership, settled-month slides, overlapping CSS groups and later occluders, background-only hover colours and unsupported source handoffs on CPU and GPU rows.
- Frame-path probes distinguish a cached geometry repaint from a full tree walk.

### Fixed
- Headless reapplies CSS layout after property-driven region changes, matching the native runner. Ancestor `:has()`, sibling and structural layout selectors follow row insertion and removal.
- Headless consumes native redraw requests and distinguishes explicit repaints from property-only wakeups, so its retained-paint probes exercise the same input invalidation contract as a desktop window.
- Cached headless paints follow the windowed runner’s property invalidation: eligible translations retain their paint, while other property writes invalidate it. Changed transforms and other visual properties appear in cached snapshots and frame-cost probes.
