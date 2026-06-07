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
    println!("[NET] net.service started");
    notify_ready_to_core();

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NetRequest {
        op: u64,
        arg0: u64,
        arg1: u64,
        arg2: u64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NetResponse {
        status: i64,
        value: u64,
    }

    impl NetRequest {
        const OP_CONNECT: u64 = 1;
        const OP_LISTEN: u64 = 2;
        const OP_RAW: u64 = 3;
    }

    let mut buf = [0u8; 64];
    loop {
        let (sender, len) = ipc::ipc_recv(&mut buf);
        if sender == 0xFFFFFFFF || len == 0xFFFFFFFD {
            task::yield_now();
            continue;
        }
        if sender == 0 || (len as usize) < core::mem::size_of::<NetRequest>() {
            time::sleep_ms(0);
            continue;
        }

        let req: NetRequest = unsafe { core::ptr::read(buf.as_ptr() as *const NetRequest) };
        let mut resp = NetResponse { status: -38, value: 0 }; // ENOSYS

        let required_cap = match req.op {
            NetRequest::OP_CONNECT => Some("net.connect"),
            NetRequest::OP_LISTEN => Some("net.listen"),
            NetRequest::OP_RAW => Some("net.raw"),
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
                core::mem::size_of::<NetResponse>(),
            )
        };
        let _ = ipc::ipc_send(sender, resp_slice);
    }
}
