use mochi_syscall::ipc;
use mochi_syscall::task;
use mochi_syscall::time;

/// READY通知OPコード
const OP_NOTIFY_READY: u64 = 0xFF;

fn notify_ready_to_core() {
    let core_pid = match task::find_process_by_name("core.service") {
        Some(pid) => pid,
        None => return,
    };
    let op_bytes = OP_NOTIFY_READY.to_le_bytes();
    let _ = ipc::ipc_send(core_pid, &op_bytes);
}

fn main() {
    println!("[WINDOW] window.service started");
    notify_ready_to_core();

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct WindowRequest {
        op: u64,
        arg0: u64,
        arg1: u64,
        arg2: u64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct WindowResponse {
        status: i64,
        value: u64,
    }

    impl WindowRequest {
        const OP_CREATE: u64 = 1;
        const OP_OVERLAY: u64 = 2;
        const OP_CAPTURE: u64 = 3;
        const OP_DISPLAY_CAPTURE: u64 = 4;
    }

    let mut buf = [0u8; 64];
    loop {
        let (sender, len) = ipc::ipc_recv(&mut buf);
        if sender == 0xFFFFFFFF || len == 0xFFFFFFFD {
            task::yield_now();
            continue;
        }
        if sender == 0 || (len as usize) < core::mem::size_of::<WindowRequest>() {
            time::sleep_ms(0);
            continue;
        }

        let req: WindowRequest =
            unsafe { core::ptr::read(buf.as_ptr() as *const WindowRequest) };
        let mut resp = WindowResponse {
            status: -38, // ENOSYS
            value: 0,
        };

        let required_caps: &[&str] = match req.op {
            WindowRequest::OP_CREATE => &["window.create"],
            WindowRequest::OP_OVERLAY => &["window.overlay"],
            WindowRequest::OP_CAPTURE => &["window.capture"],
            WindowRequest::OP_DISPLAY_CAPTURE => &["display.capture"],
            _ => &[],
        };

        for cap in required_caps {
            let ok = mochi_syscall::capability::check_thread_capability(sender, cap)
                .ok()
                .unwrap_or(false);
            if !ok {
                resp.status = -13; // EACCES
                break;
            }
        }

        let resp_slice = unsafe {
            core::slice::from_raw_parts(
                &resp as *const _ as *const u8,
                core::mem::size_of::<WindowResponse>(),
            )
        };
        let _ = ipc::ipc_send(sender, resp_slice);
    }
}
