use crate::ime::NativeImeEdit;
use blinc_layout::renderer::RenderTree;
use std::{collections::VecDeque, sync::Mutex};

static PENDING: Mutex<VecDeque<NativeImeEdit>> = Mutex::new(VecDeque::new());

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_blinc_BlincNativeBridge_nativeDispatchImeEdit(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    json: jni::objects::JString,
) -> jni::sys::jboolean {
    let Ok(json) = env.get_string(&json) else {
        return 0;
    };
    let Ok(edit) = serde_json::from_str::<NativeImeEdit>(&String::from(json)) else {
        return 0;
    };
    let mut queue = PENDING.lock().unwrap();
    if queue.len() >= 64 {
        return 0;
    }
    queue.push_back(edit);
    1
}

pub(super) fn drain(tree: &mut RenderTree) -> bool {
    let edits = std::mem::take(&mut *PENDING.lock().unwrap());
    let mut changed = false;
    for edit in edits {
        if crate::ime::apply(tree, &edit) {
            changed = true;
            if edit.submit {
                tree.broadcast_key_event(
                    blinc_core::events::event_types::KEY_DOWN,
                    13,
                    false,
                    false,
                    false,
                    false,
                );
            }
        }
    }
    changed
}

pub(super) fn synchronize(tree: Option<&RenderTree>, previous: &mut Option<String>) {
    let value = tree
        .map(crate::ime::snapshot)
        .unwrap_or_default()
        .to_string();
    if previous.as_ref() == Some(&value) {
        return;
    }
    let result: blinc_core::native_bridge::NativeResult<()> =
        blinc_core::native_bridge::native_call("keyboard", "sync", (value.as_str(),));
    if result.is_ok() {
        *previous = Some(value);
    }
}
