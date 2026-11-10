//! What the builds in place under each node created, held for as long as their output is.

use std::collections::HashMap;

use blinc_core::owner::Owner;

use crate::tree::LayoutNodeId;

/// The scopes of a node's subtree: the structural build of its children, and the latest refresh
/// that patched them in place (whose handlers and props replaced the previous refresh's).
#[derive(Default)]
struct NodeOwners {
    built: Option<Owner>,
    refreshed: Option<Owner>,
}

impl NodeOwners {
    fn dispose(self) {
        for owner in [self.built, self.refreshed].into_iter().flatten() {
            owner.dispose();
        }
    }
}

/// Per node, what rebuilding its subtree created. Disposed when a later build supersedes it,
/// when the node is removed, and with the tree.
#[derive(Default)]
pub(crate) struct SubtreeOwners(HashMap<LayoutNodeId, NodeOwners>);

impl SubtreeOwners {
    /// `node`'s children were rebuilt by a build in `owner`: what built the previous ones goes.
    pub(crate) fn built(&mut self, node: LayoutNodeId, owner: Option<Owner>) {
        if let Some(previous) = self.0.remove(&node) {
            previous.dispose();
        }
        if owner.is_some() {
            self.0.insert(
                node,
                NodeOwners {
                    built: owner,
                    refreshed: None,
                },
            );
        }
    }

    /// `node`'s children were patched in place by a refresh in `owner`, which supersedes the
    /// previous refresh.
    pub(crate) fn refreshed(&mut self, node: LayoutNodeId, owner: Option<Owner>) {
        let Some(owner) = owner else {
            return;
        };
        let entry = self.0.entry(node).or_default();
        if let Some(previous) = entry.refreshed.replace(owner) {
            previous.dispose();
        }
    }

    /// `node` is gone: so is what built under it.
    pub(crate) fn remove(&mut self, node: LayoutNodeId) {
        if let Some(owners) = self.0.remove(&node) {
            owners.dispose();
        }
    }
}

impl Drop for SubtreeOwners {
    fn drop(&mut self) {
        for (_, owners) in self.0.drain() {
            owners.dispose();
        }
    }
}
