use blinc_layout::{
    ime::ImeEdit,
    renderer::{ElementType, RenderTree},
    tree::StableNodeId,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct NativeImeEdit {
    pub id: String,
    pub session: String,
    pub sequence: u64,
    pub text: String,
    pub selection: [usize; 2],
    pub composing: Option<[usize; 2]>,
    #[serde(default)]
    pub submit: bool,
}

pub(crate) fn snapshot(tree: &RenderTree) -> serde_json::Value {
    let Some(snapshot) = tree.ime_snapshot() else {
        return serde_json::Value::Null;
    };
    let mut pending: Vec<_> = tree.layout_id(snapshot.id).into_iter().collect();
    let mut font_size = 14.0;
    while let Some(node) = pending.pop() {
        if let Some(render) = tree.get_render_node(node)
            && let ElementType::Text(text) = &render.element_type
        {
            font_size = text.font_size;
            break;
        }
        pending.extend(tree.layout().children(node).into_iter().rev());
    }
    let content = snapshot.content;
    serde_json::json!({
        "id": snapshot.id.to_raw().to_string(), "session": content.edit.session.to_string(),
        "sequence": content.edit.sequence, "text": content.edit.text,
        "selection": content.edit.selection, "composing": content.edit.composing,
        "secure": content.secure, "multiline": content.multiline, "autocorrect": content.autocorrect,
        "x": snapshot.bounds.x, "y": snapshot.bounds.y,
        "width": snapshot.bounds.width, "height": snapshot.bounds.height, "fontSize": font_size,
    })
}

pub(crate) fn apply(tree: &RenderTree, native: &NativeImeEdit) -> bool {
    let (Ok(id), Ok(session)) = (native.id.parse::<u64>(), native.session.parse::<u64>()) else {
        return false;
    };
    tree.apply_ime_edit(
        StableNodeId::from_raw(id),
        &ImeEdit {
            session,
            sequence: native.sequence,
            text: native.text.clone(),
            selection: native.selection,
            composing: native.composing,
        },
    )
}
