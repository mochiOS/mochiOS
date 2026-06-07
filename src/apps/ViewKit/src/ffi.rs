use crate::app::ViewKitApp;
use crate::pipeline::core::BackendImpl;
use std::ffi::{CStr, c_char, c_void};

#[unsafe(no_mangle)]
pub extern "C" fn viewkit_app_create() -> *mut c_void {
    match BackendImpl::new() {
        Ok(backend) => {
            let app = ViewKitApp::new(Box::new(backend));
            Box::into_raw(Box::new(app)) as *mut c_void
        }
        Err(e) => {
            eprintln!("Failed to initialize Wayland backend: {}", e);
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn viewkit_app_destroy(app_ptr: *mut c_void) {
    if !app_ptr.is_null() {
        unsafe {
            let _ = Box::from_raw(app_ptr as *mut ViewKitApp);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn viewkit_window_create(
    app_ptr: *mut c_void,
    width: u32,
    height: u32,
    title_ptr: *const c_char,
    no_decoration: bool,
) {
    let app = unsafe { &mut *(app_ptr as *mut ViewKitApp) };
    let title = if title_ptr.is_null() {
        "Kome Window"
    } else {
        unsafe { CStr::from_ptr(title_ptr).to_str().unwrap_or("Kome Window") }
    };

    // トレイト経由でバックエンドにウィンドウ生成を命令
    app.backend
        .create_window(width, height, title, no_decoration);
}

#[unsafe(no_mangle)]
pub extern "C" fn viewkit_register_component(
    app_ptr: *mut c_void,
    name_ptr: *const c_char,
    html_ptr: *const c_char,
) -> bool {
    let app = unsafe { &mut *(app_ptr as *mut ViewKitApp) };
    if name_ptr.is_null() || html_ptr.is_null() {
        return false;
    }

    let name = unsafe { CStr::from_ptr(name_ptr).to_str().unwrap() };
    let html = unsafe { CStr::from_ptr(html_ptr).to_str().unwrap() };

    app.backend.register_component(name, html).is_ok()
}

#[unsafe(no_mangle)]
pub extern "C" fn viewkit_update_ui_tree(app_ptr: *mut c_void, tree_json_ptr: *const c_char) {
    let app = unsafe { &mut *(app_ptr as *mut ViewKitApp) };
    if tree_json_ptr.is_null() {
        return;
    }

    let json_str = unsafe { CStr::from_ptr(tree_json_ptr).to_str().unwrap() };

    // Rust側のレンダラに仮想DOM/UIツリーの更新を要求
    app.backend.update_ui_tree(json_str);
}

#[unsafe(no_mangle)]
pub extern "C" fn viewkit_set_key_tap_callback(
    app_ptr: *mut c_void,
    callback: extern "C" fn(key_code: u32),
) {
    let app = unsafe { &mut *(app_ptr as *mut ViewKitApp) };
    app.set_key_tap_callback(callback);
}

#[unsafe(no_mangle)]
pub extern "C" fn viewkit_app_run(app_ptr: *mut c_void) {
    let app = unsafe { &mut *(app_ptr as *mut ViewKitApp) };
    app.run_loop();
}
