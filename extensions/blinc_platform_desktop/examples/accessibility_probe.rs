//! Query a hidden native window without moving the pointer or keyboard focus.

#[cfg(target_os = "macos")]
mod macos {
    use blinc_platform::{ControlFlow, Event, EventLoop, LifecycleEvent, Platform, WindowConfig};
    use blinc_platform_desktop::{DesktopPlatform, accesskit as ak};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::ffi::{CStr, c_char, c_void};

    #[link(name = "objc")]
    unsafe extern "C" {
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn objc_msgSend();
    }

    unsafe fn object(receiver: *mut c_void, selector: &CStr) -> *mut c_void {
        let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            unsafe { std::mem::transmute(objc_msgSend as *const ()) };
        unsafe { send(receiver, sel_registerName(selector.as_ptr())) }
    }

    unsafe fn count(array: *mut c_void) -> usize {
        let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
            unsafe { std::mem::transmute(objc_msgSend as *const ()) };
        unsafe { send(array, sel_registerName(c"count".as_ptr())) }
    }

    unsafe fn at(array: *mut c_void, index: usize) -> *mut c_void {
        let send: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void =
            unsafe { std::mem::transmute(objc_msgSend as *const ()) };
        unsafe { send(array, sel_registerName(c"objectAtIndex:".as_ptr()), index) }
    }

    unsafe fn string(receiver: *mut c_void, selector: &CStr) -> String {
        let value = unsafe { object(receiver, selector) };
        if value.is_null() {
            return String::new();
        }
        let bytes = unsafe { object(value, c"UTF8String") };
        if bytes.is_null() {
            return String::new();
        }
        unsafe { CStr::from_ptr(bytes.cast()).to_string_lossy().into_owned() }
    }

    unsafe fn walk(
        array: *mut c_void,
        depth: usize,
        result: &mut Vec<(*mut c_void, String, String)>,
    ) {
        if array.is_null() || depth > 8 {
            return;
        }
        let count = unsafe { count(array) };
        assert!(count < 32);
        for index in 0..count {
            let item = unsafe { at(array, index) };
            result.push((
                item,
                unsafe { string(item, c"accessibilityRole") },
                unsafe { string(item, c"accessibilityTitle") },
            ));
            unsafe {
                walk(object(item, c"accessibilityChildren"), depth + 1, result);
            }
        }
    }

    fn update(label: &str) -> ak::TreeUpdate {
        let mut root = ak::Node::new(ak::Role::Window);
        root.set_children(vec![ak::NodeId(1)]);
        let mut dialog = ak::Node::new(ak::Role::Dialog);
        dialog.set_label("Edit profile");
        dialog.set_children(vec![
            ak::NodeId(2),
            ak::NodeId(3),
            ak::NodeId(4),
            ak::NodeId(5),
        ]);
        let mut button = ak::Node::new(ak::Role::Button);
        button.set_label(label);
        button.add_action(ak::Action::Click);
        button.add_action(ak::Action::Focus);
        let mut field = ak::Node::new(ak::Role::TextInput);
        field.set_label("Name");
        field.set_value("Ada");
        field.add_action(ak::Action::SetValue);
        let mut custom = ak::Node::new(ak::Role::Section);
        custom.set_label("Run task");
        custom.set_role_description("task-action");
        custom.add_action(ak::Action::Click);
        let mut slider = ak::Node::new(ak::Role::Slider);
        slider.set_label("Volume");
        slider.set_numeric_value(42.0);
        ak::TreeUpdate {
            nodes: vec![
                (ak::NodeId(0), root),
                (ak::NodeId(1), dialog),
                (ak::NodeId(2), button),
                (ak::NodeId(3), field),
                (ak::NodeId(4), custom),
                (ak::NodeId(5), slider),
            ],
            tree: Some(ak::TreeInfo::new(ak::NodeId(0))),
            tree_id: ak::TreeId::ROOT,
            focus: ak::NodeId(2),
        }
    }

    pub fn run() {
        let event_loop = DesktopPlatform::new()
            .unwrap()
            .create_event_loop_with_config(WindowConfig {
                title: "Blinc native accessibility probe".into(),
                visible: false,
                active: false,
                width: 400,
                height: 300,
                ..Default::default()
            })
            .unwrap();
        event_loop.run(|event, window| {
            if !matches!(event, Event::Lifecycle(LifecycleEvent::Resumed)) { return ControlFlow::Continue; }
            let RawWindowHandle::AppKit(raw) = window.winit_window().window_handle().unwrap().as_raw() else { panic!("AppKit window"); };
            let view = raw.ns_view.as_ptr();
            // These selectors return objects, NSUInteger or BOOL as declared by AppKit.
            unsafe {
                object(view, c"accessibilityChildren");
                assert!(window.take_accessibility_request().unwrap().active);
                window.update_accessibility(|| update("Save"));
                let mut nodes = Vec::new();
                walk(object(view, c"accessibilityChildren"), 0, &mut nodes);
                let button = nodes.iter().find(|n| n.1 == "AXButton" && n.2 == "Save").expect("native Save button").0;
                let field = nodes.iter().find(|n| n.1 == "AXTextField" && n.2 == "Name").expect("native Name field").0;
                let press: unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool = std::mem::transmute(objc_msgSend as *const ());
                assert!(press(button, sel_registerName(c"accessibilityPerformPress".as_ptr())));
                let request = window.take_accessibility_request().expect("native press queued");
                assert!(request.actions.iter().any(|r| r.action == ak::Action::Click && r.target_node == ak::NodeId(2)));
                let custom = nodes.iter().find(|n| n.1 == "AXGroup" && n.2 == "Run task").expect("custom native group").0;
                assert_eq!(string(custom, c"accessibilityRoleDescription"), "task-action");
                assert!(press(custom, sel_registerName(c"accessibilityPerformPress".as_ptr())));
                assert!(window.take_accessibility_request().unwrap().actions.iter().any(|r| r.action == ak::Action::Click && r.target_node == ak::NodeId(4)));
                let slider = nodes.iter().find(|n| n.1 == "AXSlider").expect("native slider").0;
                let number = object(slider, c"accessibilityValue");
                let get_number: unsafe extern "C" fn(*mut c_void, *mut c_void) -> f64 = std::mem::transmute(objc_msgSend as *const ());
                assert_eq!(get_number(number, sel_registerName(c"doubleValue".as_ptr())), 42.0);
                let set_focus: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) = std::mem::transmute(objc_msgSend as *const ());
                set_focus(button, sel_registerName(c"setAccessibilityFocused:".as_ptr()), true);
                assert!(window.take_accessibility_request().unwrap().actions.iter().any(|r| r.action == ak::Action::Focus));
                let set: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) = std::mem::transmute(objc_msgSend as *const ());
                let make: unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
                let value = make(objc_getClass(c"NSString".as_ptr()), sel_registerName(c"stringWithUTF8String:".as_ptr()), c"River".as_ptr());
                set(field, sel_registerName(c"setAccessibilityValue:".as_ptr()), value);
                assert!(window.take_accessibility_request().unwrap().actions.iter().any(|r| r.action == ak::Action::SetValue && matches!(&r.data, Some(ak::ActionData::Value(v)) if v.as_ref() == "River")));
                window.update_accessibility(|| update("Saved"));
                let mut changed = Vec::new();
                walk(object(view, c"accessibilityChildren"), 0, &mut changed);
                assert_eq!(changed.iter().find(|n| n.2 == "Saved").unwrap().0, button);
                println!("PASS native AppKit: hidden window, dialog children, custom roles/actions, focus, value editing, numeric values and stable objects");
            }
            ControlFlow::Exit
        }).unwrap();
    }
}

#[cfg(target_os = "macos")]
fn main() {
    macos::run();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("The native AppKit probe runs on macOS.");
}
