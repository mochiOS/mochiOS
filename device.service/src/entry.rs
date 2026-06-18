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
    println!("[DEVICE] device.service started");
    notify_ready_to_core();

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct DeviceRequest {
        op: u64,
        arg0: u64,
        arg1: u64,
        arg2: u64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct DeviceResponse {
        status: i64,
        value: u64,
    }

    impl DeviceRequest {
        const OP_GPU: u64 = 1;
        const OP_AUDIO: u64 = 2;
        const OP_INPUT: u64 = 3;
        const OP_STORAGE: u64 = 4;
        const OP_NET: u64 = 5;
        const OP_USB: u64 = 6;
        const OP_SERIAL: u64 = 7;
        const OP_BLUETOOTH: u64 = 8;
    }

    let mut buf = [0u8; 64];
    loop {
        let (sender, len) = ipc::ipc_recv(&mut buf);
        if sender == 0xFFFFFFFF || len == 0xFFFFFFFD {
            task::yield_now();
            continue;
        }
        if sender == 0 || (len as usize) < core::mem::size_of::<DeviceRequest>() {
            task::yield_now();
            continue;
        }

        let req: DeviceRequest =
            unsafe { core::ptr::read(buf.as_ptr() as *const DeviceRequest) };
        let mut resp = DeviceResponse {
            status: -38, // ENOSYS
            value: 0,
        };

        let required_cap = match req.op {
            DeviceRequest::OP_GPU => Some("device.gpu"),
            DeviceRequest::OP_AUDIO => Some("device.audio"),
            DeviceRequest::OP_INPUT => Some("device.input"),
            DeviceRequest::OP_STORAGE => Some("device.storage"),
            DeviceRequest::OP_NET => Some("device.net"),
            DeviceRequest::OP_USB => Some("usb.access"),
            DeviceRequest::OP_SERIAL => Some("serial.access"),
            DeviceRequest::OP_BLUETOOTH => Some("bluetooth.access"),
            _ => None,
        };

        if let Some(cap) = required_cap {
            let ok = mochi_syscall::capability::check_thread_capability(sender, cap)
                .ok()
                .unwrap_or(false);
            if !ok {
                resp.status = -13; // EACCES
            }
        }

        let resp_slice = unsafe {
            core::slice::from_raw_parts(
                &resp as *const _ as *const u8,
                core::mem::size_of::<DeviceResponse>(),
            )
        };
        let _ = ipc::ipc_send(sender, resp_slice);
    }
}
