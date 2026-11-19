//! Optional structure diagnostics, kept outside the measured frame interval.

use blinc_layout::LayoutNodeId;
use blinc_recorder::Headless;
use std::sync::atomic::Ordering;

pub(super) struct Snapshot {
    root_builds: u64,
    scope: u64,
    generation: u64,
    layout_epoch: u64,
    pages: Vec<LayoutNodeId>,
    nodes: usize,
    index_entries: usize,
    stale_pages: usize,
}

impl Snapshot {
    pub(super) fn read(ui: &Headless) -> Self {
        let tree = ui.tree();
        let index = tree.element_registry().class_to_nodes_index();
        let pages: Vec<_> = index
            .get("cn-calendar-page")
            .into_iter()
            .flatten()
            .copied()
            .collect();
        let stale_pages = pages
            .iter()
            .filter(|node| tree.get_render_node(**node).is_none())
            .count();
        Self {
            root_builds: super::ROOT_BUILDS.load(Ordering::Relaxed),
            scope: tree.tree_scope_id(),
            generation: tree.build_generation(),
            layout_epoch: tree.layout_tree.epoch(),
            pages: pages
                .into_iter()
                .filter(|node| tree.get_render_node(*node).is_some())
                .collect(),
            nodes: tree.layout_tree.len(),
            index_entries: index.values().map(Vec::len).sum(),
            stale_pages,
        }
    }
}

#[derive(Default)]
pub(super) struct TreeCounts {
    frames: usize,
    root_builds: u64,
    scope_replacements: usize,
    generations: u64,
    structural_frames: usize,
    layout_frames: usize,
    pages_added: usize,
    pages_removed: usize,
    peak_index_entries: usize,
    last_index_entries: usize,
    peak_stale_pages: usize,
}

impl TreeCounts {
    pub(super) fn record(&mut self, before: Snapshot, ui: &Headless) {
        let after = Snapshot::read(ui);
        self.frames += 1;
        let roots = after.root_builds - before.root_builds;
        let scope_changed = before.scope != after.scope;
        let generations = if scope_changed {
            after.generation
        } else {
            after.generation.wrapping_sub(before.generation)
        };
        let added = after
            .pages
            .iter()
            .filter(|p| !before.pages.contains(p))
            .count();
        let removed = before
            .pages
            .iter()
            .filter(|p| !after.pages.contains(p))
            .count();
        let layout_changed = scope_changed || before.layout_epoch != after.layout_epoch;
        self.root_builds += roots;
        self.scope_replacements += usize::from(scope_changed);
        self.generations += generations;
        self.layout_frames += usize::from(layout_changed);
        self.pages_added += added;
        self.pages_removed += removed;
        self.peak_index_entries = self.peak_index_entries.max(after.index_entries);
        self.last_index_entries = after.index_entries;
        self.peak_stale_pages = self.peak_stale_pages.max(after.stale_pages);
        if roots > 0 || scope_changed || generations > 0 {
            self.structural_frames += 1;
            println!(
                "    structure frame {}: root_builds={roots} scope_changed={scope_changed} generation_delta={generations} pages=+{added}/-{removed} layout_changed={layout_changed} nodes={}->{}",
                self.frames, before.nodes, after.nodes,
            );
        }
    }

    pub(super) fn report(&self) {
        println!(
            "    tree totals: frames={} root_builds={} scope_replacements={} generation_increments={} structural_frames={} layout_mutation_frames={} pages_added={} pages_removed={}",
            self.frames,
            self.root_builds,
            self.scope_replacements,
            self.generations,
            self.structural_frames,
            self.layout_frames,
            self.pages_added,
            self.pages_removed,
        );
        println!(
            "    selector totals: peak_entries={} final_entries={} peak_stale_pages={}",
            self.peak_index_entries, self.last_index_entries, self.peak_stale_pages,
        );
    }
}
