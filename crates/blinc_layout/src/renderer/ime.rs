use super::RenderTree;
use crate::{
    ime::{ImeEdit, ImeSnapshot},
    tree::StableNodeId,
};

impl RenderTree {
    /// The focused editable document belonging to this tree and active scope.
    pub fn ime_snapshot(&self) -> Option<ImeSnapshot> {
        let mut focused = None;
        for (node, binding) in &self.ime_bindings {
            let Some(content) = (binding.read)() else {
                continue;
            };
            if !content.focused || content.disabled {
                continue;
            }
            if let Some((_, scope)) = self.active_focus_scopes().last()
                && node != scope
                && !self.layout_tree.ancestors(*node).contains(scope)
            {
                continue;
            }
            focused = Some(ImeSnapshot {
                id: self.stable_id(*node)?,
                content,
                bounds: self.get_absolute_bounds(*node)?,
            });
        }
        focused
    }

    /// Reject updates from a removed field or a previous focus session.
    pub fn apply_ime_edit(&self, id: StableNodeId, edit: &ImeEdit) -> bool {
        let Some(snapshot) = self.ime_snapshot() else {
            return false;
        };
        if snapshot.id != id || snapshot.content.edit.session != edit.session {
            return false;
        }
        let Some(node) = self.layout_id(id) else {
            return false;
        };
        self.ime_bindings
            .get(&node)
            .is_some_and(|binding| (binding.edit)(edit))
    }
}
