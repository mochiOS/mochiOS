//! プロセス管理関連のシステムコール

use super::types::{EFAULT, EINVAL, ENOMEM, ENOSYS, SUCCESS};

/// ユーザー空間の上限アドレス (x86-64 canonical hole 下側)
const USER_SPACE_END: u64 = 0x0000_7FFF_FFFF_FFFF;
use crate::task::{current_thread_id, exit_current_task};

/// Exitシステムコール
///
/// プロセスを終了する
///
/// # 引数
/// - `exit_code`: 終了コード
///
/// # 戻り値
/// このシステムコールは戻らない（プロセスが終了する）
pub fn exit(exit_code: u64) -> ! {
    crate::sprintln!("Process exiting with code: {}", exit_code);

    // スケジューラから現在のタスクを削除して終了
    exit_current_task(exit_code)
}

/// GetPidシステムコール
///
/// 現在のプロセスIDを取得する
///
/// # 戻り値
/// プロセスID
pub fn getpid() -> u64 {
    if let Some(tid) = current_thread_id() {
        crate::task::with_thread(tid, |thread| thread.process_id().as_u64()).unwrap_or(0)
    } else {
        0
    }
}

/// GetTidシステムコール
///
/// 現在のスレッドIDを取得する
///
/// # 戻り値
/// スレッドID
pub fn gettid() -> u64 {
    if let Some(tid) = current_thread_id() {
        tid.as_u64()
    } else {
        0
    }
}

/// Brkシステムコール
///
/// メモリのヒープ領域サイズを変更する
pub fn brk(addr: u64) -> u64 {
    // 現在のプロセスIDを取得
    let current_tid = match current_thread_id() {
        Some(tid) => tid,
        None => return ENOSYS,
    };

    // プロセスIDを取得
    let pid = match crate::task::with_thread(current_tid, |t| t.process_id()) {
        Some(pid) => pid,
        None => return ENOSYS,
    };

    let result = crate::task::with_process_mut(pid, |process| {
        // addr == 0 なら現在の位置を返す
        if addr == 0 {
            if process.heap_start() == 0 {
                // ヒープ領域初期化（暫定）
                let default_heap_base = 0x4000_0000;
                process.set_heap_start(default_heap_base);
                process.set_heap_end(default_heap_base);
            }
            return Ok(process.heap_end());
        }

        let current_brk = process.heap_end();

        // ユーザー空間の上限アドレスを超えるbrkを拒否
        if addr > USER_SPACE_END {
            return Err(EINVAL);
        }

        // 縮小または変化なし
        if addr <= current_brk {
            process.set_heap_end(addr);
            return Ok(addr);
        }

        // プロセス固有のページテーブルアドレスを取得
        let pt_phys = match process.page_table() {
            Some(p) => p,
            None => return Err(ENOSYS),
        };

        // 拡大時にページをプロセスのページテーブルにマップ（書き込み可能、実行不可）
        // 現在の brk がページ境界でない場合に、既に存在するページを含めてマップするために
        // floor(current_brk) を使用する。
        let start_page = current_brk & !4095;
        let end_page = (addr + 4095) & !4095;

        if end_page > start_page {
            let size = end_page - start_page;
            if let Err(_) = crate::mem::paging::map_and_copy_segment_to(
                pt_phys,
                start_page,
                0,
                size,
                &[],
                true,
                false,
            ) {
                return Err(ENOSYS);
            }
        }

        process.set_heap_end(addr);
        Ok(addr)
    });

    match result {
        Some(Ok(addr)) => addr,
        Some(Err(err)) => err,
        None => ENOSYS,
    }
}

/// Forkシステムコール
///
/// プロセスを複製する
pub fn fork() -> u64 {
    use crate::syscall::syscall_entry::{
        SYSCALL_SAVED_USER_RFLAGS, SYSCALL_SAVED_USER_RIP, SYSCALL_TEMP_USER_RSP,
    };
    use core::sync::atomic::Ordering;

    let user_rsp = SYSCALL_TEMP_USER_RSP.load(Ordering::Relaxed);
    let user_rip = SYSCALL_SAVED_USER_RIP.load(Ordering::Relaxed);
    let user_rflags = SYSCALL_SAVED_USER_RFLAGS.load(Ordering::Relaxed);

    if user_rip == 0 {
        return ENOSYS;
    }

    // 現在のスレッド/プロセスを取得
    let parent_tid = match crate::task::current_thread_id() {
        Some(t) => t,
        None => return ENOSYS,
    };
    let parent_pid = match crate::task::with_thread(parent_tid, |t| t.process_id()) {
        Some(p) => p,
        None => return ENOSYS,
    };

    // 親のページテーブルとヒープ状態を取得
    let parent_pt = match crate::task::with_process(parent_pid, |p| p.page_table()) {
        Some(Some(pt)) => pt,
        _ => return ENOSYS,
    };
    let (parent_heap_start, parent_heap_end) =
        crate::task::with_process(parent_pid, |p| (p.heap_start(), p.heap_end())).unwrap_or((0, 0));

    // 親の FS ベース (TLS) を取得
    let fs_base = unsafe { crate::cpu::read_fs_base() };

    // 子プロセスを作成 (親のページテーブルを共有: シングルプロセス前提)
    let mut child_proc = crate::task::Process::new(
        "fork_child",
        crate::task::PrivilegeLevel::User,
        Some(parent_pid),
        0,
    );
    child_proc.set_page_table(parent_pt);
    child_proc.set_heap_start(parent_heap_start);
    child_proc.set_heap_end(parent_heap_end);
    let child_pid = child_proc.id();

    if crate::task::add_process(child_proc).is_none() {
        return ENOSYS;
    }

    // 子スレッド用カーネルスタックを確保
    let kstack = match crate::task::thread::allocate_kernel_stack(4096 * 4) {
        Some(a) => a,
        None => return ENOSYS,
    };

    // fork() で rax=0 を返す子スレッドを作成
    let child_thread = crate::task::Thread::new_fork_child(
        child_pid,
        user_rip,
        user_rsp,
        user_rflags,
        fs_base,
        kstack,
        4096 * 4,
    );

    if crate::task::add_thread(child_thread).is_none() {
        return ENOSYS;
    }

    // 子 PID を親に返す
    child_pid.as_u64()
}

/// Sleepシステムコール
///
/// 指定されたミリ秒数の間スリープする
///
/// # 引数
/// - `milliseconds`: スリープ時間（ミリ秒）
///
/// # 戻り値
/// 成功時はSUCCESS
pub fn sleep(milliseconds: u64) -> u64 {
    // TODO: 正確なタイマーベースのスリープを実装
    // 現在は単純にyieldするだけ（タイマー割り込みがあるので時間は経過する）
    // 最大でも数回yieldするだけにする
    let yield_count = (milliseconds / 10).max(1).min(100);

    for _ in 0..yield_count {
        crate::task::yield_now();
    }

    SUCCESS
}

/// Waitシステムコール (wait4)
///
/// # 引数
/// - `pid`: 待機するプロセスID (-1 = 任意の子プロセス)
/// - `status_ptr`: 終了ステータスを書き込むポインタ (0 = 無視)
/// - `options`: WNOHANG(0x1) = ノンブロッキング
pub fn wait(_pid: u64, status_ptr: u64, options: u64) -> u64 {
    const WNOHANG: u64 = 0x1;
    // status = 0 (terminated normally, exit code 0)
    if status_ptr != 0 {
        // ユーザー空間アドレスの有効性を検証する
        if !super::validate_user_ptr(status_ptr, 4) {
            return EFAULT;
        }
        unsafe {
            *(status_ptr as *mut i32) = 0;
        }
    }
    if options & WNOHANG != 0 {
        // ノンブロッキング: まだ終了していない (pid=0 = 子プロセス実行中)
        return 0;
    }
    // ブロッキング wait: 子プロセスが存在しない場合 ECHILD を返す
    // 本来はここでスリープするが、シンプル実装のため ECHILD を返す
    (-10i64) as u64 // ECHILD
}

/// Mmapシステムコール
///
/// 匿名メモリマッピングを作成する (MAP_ANONYMOUS | MAP_PRIVATE のみサポート)
///
/// # 引数
/// - `addr`: ヒント仮想アドレス (0で任意)
/// - `length`: マップするサイズ
/// - `prot`: 保護フラグ (PROT_READ|PROT_WRITE = 3)
/// - `flags`: マップフラグ (MAP_ANONYMOUS=0x20, MAP_PRIVATE=0x2)
/// - `_fd`: ファイルディスクリプタ (-1 = 匿名)
///
/// # 戻り値
/// マップされた仮想アドレス、またはエラーコード
pub fn mmap(addr: u64, length: u64, _prot: u64, flags: u64, _fd: u64) -> u64 {
    use super::types::{EINVAL, ENOMEM};

    if length == 0 {
        return EINVAL;
    }

    // MAP_ANONYMOUS (0x20) のみサポート
    const MAP_ANONYMOUS: u64 = 0x20;
    if flags & MAP_ANONYMOUS == 0 {
        return ENOSYS;
    }

    let current_tid = match crate::task::current_thread_id() {
        Some(tid) => tid,
        None => return ENOMEM,
    };
    let pid = match crate::task::with_thread(current_tid, |t| t.process_id()) {
        Some(pid) => pid,
        None => return ENOMEM,
    };

    // ページ境界に切り上げ
    let size = (length + 4095) & !4095;

    let result = crate::task::with_process_mut(pid, |process| {
        // mmap用のヒープ領域を現在のbrk以降に割り当てる
        // (簡易実装: brkと同じ領域を使う)
        if process.heap_start() == 0 {
            let default_heap_base = 0x5000_0000u64;
            process.set_heap_start(default_heap_base);
            process.set_heap_end(default_heap_base);
        }

        // ユーザー空間の上限アドレスを超えるaddrを拒否
        if addr != 0 && addr > USER_SPACE_END {
            return Err(EINVAL);
        }

        let map_start = if addr != 0 {
            (addr + 4095) & !4095
        } else {
            // heap_endを mmap_base として使う（簡易実装）
            // 実際は別のアドレス空間管理が必要
            let base = process.heap_end();
            (base + 4095) & !4095
        };

        let pt_phys = match process.page_table() {
            Some(p) => p,
            None => return Err(ENOMEM),
        };

        if let Err(_) = crate::mem::paging::map_and_copy_segment_to(
            pt_phys,
            map_start,
            0,
            size,
            &[],
            true,
            false,
        ) {
            return Err(ENOMEM);
        }

        // heap_end を更新してアドレス空間が重ならないようにする
        if addr == 0 {
            process.set_heap_end(map_start + size);
        }

        Ok(map_start)
    });

    match result {
        Some(Ok(va)) => va,
        Some(Err(e)) => e,
        None => ENOMEM,
    }
}

/// Munmapシステムコール (スタブ)
pub fn munmap(_addr: u64, _length: u64) -> u64 {
    // TODO: ページテーブルからマッピングを削除する
    SUCCESS
}

/// Futexシステムコール (最小実装)
///
/// FUTEX_WAIT と FUTEX_WAKE のみサポート
pub fn futex(uaddr: u64, op: u32, val: u64, _timeout: u64) -> u64 {
    use super::types::EAGAIN;
    const FUTEX_WAIT: u32 = 0;
    const FUTEX_WAKE: u32 = 1;
    const FUTEX_PRIVATE_FLAG: u32 = 128;

    let op_base = op & !FUTEX_PRIVATE_FLAG;

    match op_base {
        FUTEX_WAIT => {
            if uaddr == 0 {
                return EFAULT;
            }
            // ユーザー空間アドレスの有効性を検証する
            if !super::validate_user_ptr(uaddr, 4) {
                return EFAULT;
            }
            let current_val = unsafe { core::ptr::read_volatile(uaddr as *const u32) };
            if current_val != val as u32 {
                return EAGAIN;
            }
            // 簡易実装: yield して再試行させる
            crate::task::yield_now();
            SUCCESS
        }
        FUTEX_WAKE => {
            // Wake は何もしなくても yield ベースで動く
            SUCCESS
        }
        _ => ENOSYS,
    }
}

/// arch_prctlシステムコール
///
/// TLS 用の FS ベースレジスタを設定する
pub fn arch_prctl(code: u64, addr: u64) -> u64 {
    const ARCH_SET_FS: u64 = 0x1002;
    const ARCH_GET_FS: u64 = 0x1003;

    match code {
        ARCH_SET_FS => {
            // FS ベースレジスタを設定 (WRFSBASE または IA32_FS_BASE MSR)
            unsafe {
                crate::cpu::write_fs_base(addr);
            }
            // 現在のスレッドに FS base を記録 (コンテキストスイッチ時に復元するため)
            if let Some(tid) = crate::task::current_thread_id() {
                crate::task::with_thread_mut(tid, |t| t.set_fs_base(addr));
            }
            SUCCESS
        }
        ARCH_GET_FS => {
            let val = unsafe { crate::cpu::read_fs_base() };
            // addrが指すメモリに書き込む
            if addr == 0 {
                return EFAULT;
            }
            // ユーザー空間アドレスの有効性を検証する
            if !super::validate_user_ptr(addr, 8) {
                return EFAULT;
            }
            unsafe { core::ptr::write_unaligned(addr as *mut u64, val) };
            SUCCESS
        }
        _ => EINVAL,
    }
}

/// FindProcessByNameシステムコール
///
/// プロセス名からPIDを検索する
///
/// # 引数
/// - `name_ptr`: プロセス名のポインタ
/// - `len`: プロセス名の長さ
///
/// # 戻り値
/// 見つかった場合はPID、見つからない場合は0
pub fn find_process_by_name(name_ptr: u64, len: u64) -> u64 {
    use crate::task;
    use core::str;

    if name_ptr == 0 || len == 0 || len > 64 {
        return 0;
    }

    // ユーザー空間アドレスの有効性を検証する
    if !super::validate_user_ptr(name_ptr, len) {
        return 0;
    }

    let name_slice = unsafe { core::slice::from_raw_parts(name_ptr as *const u8, len as usize) };
    let name = match str::from_utf8(name_slice) {
        Ok(s) => s,
        Err(_) => return 0,
    };

    // プロセスリストを検索
    // TODO: 直接タスク管理モジュールにアクセスするのはリスキーなのでロックをかける
    // taskモジュールに検索関数を追加するのが望ましい
    task::find_process_id_by_name(name)
        .map(|pid| pid.as_u64())
        .unwrap_or(0)
}
