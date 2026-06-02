use crate::interrupt::spinlock::SpinLock;

use super::context::switch_to_thread_with_slots;
use super::ids::{SchedulingClass, ThreadId, ThreadState};
use super::thread::{
    current_thread_id, current_thread_slot, remove_thread, set_current_thread, set_thread_state,
    set_thread_state_at_slot, with_thread, with_thread_at_slot, with_thread_at_slot_mut,
    with_thread_mut, THREAD_QUEUE,
};

/// スケジューラ
///
/// スレッドのスケジューリングを管理
pub struct Scheduler {
    /// スケジューラが有効かどうか
    enabled: bool,
    /// タイムスライス（タイマー割り込み回数）
    time_slice: u64,
    /// 現在のタイムスライスカウンタ
    current_slice: u64,
}

impl Scheduler {
    pub const DEFAULT_TIME_SLICE: u64 = crate::interrupt::timer::ms_to_ticks_ceil(10);

    /// 新しいスケジューラを作成
    pub const fn new() -> Self {
        Self {
            enabled: false,
            time_slice: Self::DEFAULT_TIME_SLICE,
            current_slice: 0,
        }
    }

    /// スケジューラを有効化
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// スケジューラを無効化
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// スケジューラが有効かどうか
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// タイムスライスを設定
    pub fn set_time_slice(&mut self, slice: u64) {
        self.time_slice = slice;
    }

    /// タイマー割り込み時に呼ばれる
    ///
    /// タイムスライスをカウントし、期限が来たらスケジューリングを実行
    pub fn tick(&mut self) -> bool {
        if !self.enabled {
            return false;
        }

        self.current_slice += 1;
        if self.current_slice >= self.time_slice {
            self.current_slice = 0;
            true // スケジューリングが必要
        } else {
            false
        }
    }

    /// タイムスライスをリセット
    pub fn reset_slice(&mut self) {
        self.current_slice = 0;
    }
}

const INTERACTIVE_MIN_TICKS: u64 = crate::interrupt::timer::ms_to_ticks_ceil(4);
const INTERACTIVE_MID_TICKS: u64 = crate::interrupt::timer::ms_to_ticks_ceil(6);
const INTERACTIVE_MAX_TICKS: u64 = crate::interrupt::timer::ms_to_ticks_ceil(8);
const NORMAL_TICKS: u64 = crate::interrupt::timer::ms_to_ticks_ceil(10);
const CPU_BOUND_TICKS: u64 = crate::interrupt::timer::ms_to_ticks_ceil(20);
const BACKGROUND_TICKS: u64 = crate::interrupt::timer::ms_to_ticks_ceil(30);
const LOW_PRIORITY_THRESHOLD: u8 = 192;
const CPU_BIAS_PRIORITY_THRESHOLD: u8 = 128;
const INTERACTIVE_PRIORITY_THRESHOLD: u8 = 31;

fn classify_thread(
    is_foreground: bool,
    process_priority: u8,
    interactive_score: u8,
    cpu_burst_score: u8,
) -> SchedulingClass {
    if is_foreground {
        SchedulingClass::Interactive
    } else if process_priority >= LOW_PRIORITY_THRESHOLD {
        SchedulingClass::Background
    } else if cpu_burst_score >= 4 || process_priority >= CPU_BIAS_PRIORITY_THRESHOLD {
        SchedulingClass::CpuBound
    } else if interactive_score >= 2 || process_priority <= INTERACTIVE_PRIORITY_THRESHOLD {
        SchedulingClass::Interactive
    } else {
        SchedulingClass::Normal
    }
}

fn class_time_slice_ticks(
    class: SchedulingClass,
    is_foreground: bool,
    interactive_score: u8,
    process_priority: u8,
) -> u64 {
    match class {
        SchedulingClass::Interactive => {
            if is_foreground || interactive_score >= 6 || process_priority <= 7 {
                INTERACTIVE_MIN_TICKS
            } else if interactive_score >= 4 || process_priority <= 15 {
                INTERACTIVE_MID_TICKS
            } else {
                INTERACTIVE_MAX_TICKS
            }
        }
        SchedulingClass::Normal => NORMAL_TICKS,
        SchedulingClass::CpuBound => CPU_BOUND_TICKS,
        SchedulingClass::Background => {
            BACKGROUND_TICKS + ((process_priority.saturating_sub(LOW_PRIORITY_THRESHOLD)) / 16) as u64
        }
    }
}

fn time_slice_ticks_for_thread(
    is_foreground: bool,
    process_priority: u8,
    interactive_score: u8,
    cpu_burst_score: u8,
) -> u64 {
    let class = classify_thread(
        is_foreground,
        process_priority,
        interactive_score,
        cpu_burst_score,
    );
    class_time_slice_ticks(class, is_foreground, interactive_score, process_priority)
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// グローバルスケジューラ
static SCHEDULER: SpinLock<Scheduler> = SpinLock::new(Scheduler::new());

/// スケジューラを初期化
pub fn init_scheduler() {
    let mut scheduler = SCHEDULER.lock();
    scheduler.enable();
}

/// スケジューラを有効化
pub fn enable_scheduler() {
    SCHEDULER.lock().enable();
}

/// タイムスライスを設定
pub fn set_time_slice(slice: u64) {
    SCHEDULER.lock().set_time_slice(slice);
}

/// スケジューラを無効化
pub fn disable_scheduler() {
    SCHEDULER.lock().disable();
}

/// スケジューラが有効かどうか
pub fn is_scheduler_enabled() -> bool {
    SCHEDULER.lock().is_enabled()
}

/// タイマー割り込み時に呼ばれる（タイマー割り込みハンドラから呼び出す）
///
/// # Returns
/// スケジューリングが必要な場合はtrue
pub fn scheduler_tick() -> bool {
    if let Some(slot) = current_thread_slot() {
        if with_thread_at_slot(slot, |t| t.in_syscall()).unwrap_or(false) {
            return false;
        }
    } else if let Some(tid) = current_thread_id() {
        if with_thread(tid, |t| t.in_syscall()).unwrap_or(false) {
            return false;
        }
    }
    let should_schedule = SCHEDULER.lock().tick();
    if should_schedule {
        if let Some(slot) = current_thread_slot() {
            let _ = with_thread_at_slot_mut(slot, |thread| thread.note_slice_exhausted());
        } else if let Some(tid) = current_thread_id() {
            let _ = with_thread_mut(tid, |thread| thread.note_slice_exhausted());
        }
    }
    should_schedule
}

/// 次に実行すべきスレッドを選択
///
/// ラウンドロビンスケジューリング：Ready状態のスレッドを順に選択
///
/// # Returns
/// 次に実行すべきスレッドID。実行可能なスレッドがない場合はNone
pub fn schedule() -> Option<ThreadId> {
    schedule_with_slot().map(|(next_id, _)| next_id)
}

/// 次に実行すべきスレッドIDとスロットを取得
fn schedule_with_slot() -> Option<(ThreadId, usize)> {
    let mut queue = THREAD_QUEUE.lock();

    let current_slot = current_thread_slot();
    let current = current_thread_id();

    // 現在のスレッドの状態を Running から Ready に戻す
    if let Some(slot) = current_slot {
        if queue
            .get_slot(slot)
            .is_some_and(|thread| thread.state() == ThreadState::Running)
        {
            queue.set_state_at_slot(slot, ThreadState::Ready);
        }
    } else if let Some(current_id) = current {
        if let Some(slot) = queue.slot_index(current_id) {
            if queue
                .get_slot(slot)
                .is_some_and(|thread| thread.state() == ThreadState::Running)
            {
                queue.set_state_at_slot(slot, ThreadState::Ready);
            }
        }
    }

    // 次の Ready スレッドを探す
    let next_slot = queue.next_ready_slot_after(current_slot)?;
    let (next_id, next_pid, interactive_score, cpu_burst_score) =
        queue.get_slot(next_slot).map(|thread| {
            (
                thread.id(),
                thread.process_id(),
                thread.interactive_score(),
                thread.cpu_burst_score(),
            )
        })?;

    // 見つけた次のスレッドを Running 状態に設定
    queue.set_state_at_slot(next_slot, ThreadState::Running);

    let (process_priority, is_foreground) = crate::task::with_process(next_pid, |process| {
        (process.priority(), process.is_foreground())
    })
    .unwrap_or((0, false));
    let next_time_slice =
        time_slice_ticks_for_thread(is_foreground, process_priority, interactive_score, cpu_burst_score);

    drop(queue);
    let mut scheduler = SCHEDULER.lock();
    scheduler.set_time_slice(next_time_slice.max(1));
    scheduler.reset_slice();

    Some((next_id, next_slot))
}

/// 現在のスレッドを明示的にCPUを手放す（yield）
///
/// スケジューラを呼び出して次のスレッドに切り替える
pub fn yield_now() {
    if !is_scheduler_enabled() {
        return;
    }

    // スケジューリングと切り替えは割り込み禁止区間で実行し、
    // 状態更新と実際の切替の間に割り込みが入る競合窓を防ぐ。
    x86_64::instructions::interrupts::without_interrupts(|| {
        if let Some((next_id, next_slot)) = schedule_with_slot() {
            let current = current_thread_id();
            let current_slot = current_thread_slot();

            // 次のスレッドが現在のスレッドと異なる場合のみ切り替え
            if Some(next_id) != current {
                unsafe {
                    switch_to_thread_with_slots(current, current_slot, next_id, next_slot);
                }
            }
        }
    });
}

/// スレッドをブロック状態にする
///
/// 現在のスレッドをBlocked状態にして、次のスレッドにスケジューリング
pub fn block_current_thread() {
    if let Some(current_id) = current_thread_id() {
        let _ = with_thread_mut(current_id, |thread| thread.note_voluntary_yield());
        if let Some(slot) = current_thread_slot() {
            set_thread_state_at_slot(slot, ThreadState::Blocked);
        } else {
            set_thread_state(current_id, ThreadState::Blocked);
        }

        // 次のスレッドにスケジューリング
        yield_now();
    }
}

/// スレッドをスリープ状態にする
///
/// 指定されたスレッドをSleeping状態にする
pub fn sleep_thread(id: ThreadId) {
    if Some(id) == current_thread_id() {
        let _ = with_thread_mut(id, |thread| thread.note_voluntary_yield());
    }
    set_thread_state(id, ThreadState::Sleeping);
}

/// スレッドを起床させる
///
/// Sleeping/Blocked状態のスレッドをReady状態にする。
/// Ready状態の場合は pending_wakeup フラグを立てて競合を防ぐ。
pub fn wake_thread(id: ThreadId) {
    if let Some(current_state) = crate::task::with_thread(id, |t| t.state()) {
        if current_state == ThreadState::Sleeping || current_state == ThreadState::Blocked {
            let _ = with_thread_mut(id, |thread| thread.note_interactive_wakeup());
            set_thread_state(id, ThreadState::Ready);
        } else if current_state == ThreadState::Ready {
            with_thread_mut(id, |t| t.set_pending_wakeup());
        }
    }
}

/// 現在のスレッドをスリープ状態にする。
///
/// pending_wakeup フラグが立っていれば眠らずに即座に返す（競合回避）。
/// # Returns
/// `true` なら実際に Sleeping 状態に遷移した。`false` なら眠らなかった。
pub fn sleep_thread_unless_woken(id: ThreadId) -> bool {
    let mut should_sleep = true;
    crate::task::with_thread_mut(id, |thread| {
        if thread.take_pending_wakeup() {
            should_sleep = false;
        }
    });
    
    if should_sleep {
        if Some(id) == current_thread_id() {
            let _ = with_thread_mut(id, |thread| thread.note_voluntary_yield());
        }
        set_thread_state(id, ThreadState::Sleeping);
        true
    } else {
        false
    }
}

/// 子プロセス終了時に親プロセスの先頭スレッドの IPC waiter を起床させる。
/// IPC recv_blocking でスリープしている親スレッドを叩き起こし、child exit を検知させる。
fn wake_parent_ipc_waiter(exited_pid: crate::task::ProcessId) {
    use crate::task::with_process;
    let parent_pid = match with_process(exited_pid, |p| p.parent_id()) {
        Some(Some(pid)) => pid,
        _ => return,
    };

    // 親プロセスの最初のスレッドを探し、IPC mailbox に積まれた waiter を起床させる
    let mut parent_tid: Option<ThreadId> = None;
    crate::task::for_each_thread(|thread| {
        if parent_tid.is_none() && thread.process_id() == parent_pid {
            parent_tid = Some(thread.id());
        }
    });

    if let Some(tid) = parent_tid {
        // ゼロ長メッセージを mailbox に積んで recv_blocking が確実に戻れるようにする。
        // wake_thread だけでは「スリープ中に Ready に変えて pending_wakeup なし」の場合、
        // recv_blocking が yield 後に再スリープしてしまうため、必ずメッセージを使う。
        crate::syscall::ipc::send_from_kernel(tid.as_u64(), &[]);
    }
}

/// スレッドを終了させる
///
/// 指定されたスレッドをTerminated状態にして削除
pub fn terminate_thread(id: ThreadId) {
    set_thread_state(id, ThreadState::Terminated);

    // 現在のスレッドの場合は次のスレッドにスケジューリング
    if Some(id) == current_thread_id() {
        set_current_thread(None, None);
        yield_now();
    }

    crate::syscall::process::clear_futex_waiter(id);
    // スレッドをキューから削除し、カーネルスタックを解放
    if let Some(thread) = remove_thread(id) {
        crate::task::free_kernel_stack(thread.kernel_stack_base());
    }
}

/// 現在のタスクを終了させる（exitシステムコール用）
///
/// 現在のスレッドをTerminated状態にして削除し、次のスレッドにスケジューリング
pub fn exit_current_task(exit_code: u64) -> ! {
    if let Some(current_id) = current_thread_id() {
        crate::debug!("Exiting thread {:?} with code {}", current_id, exit_code);
        let current_pid = with_thread(current_id, |thread| thread.process_id());

        set_thread_state(current_id, ThreadState::Terminated);

        if let Some(pid) = current_pid {
            let mut has_other_live_threads = false;
            crate::task::for_each_thread(|thread| {
                if thread.process_id() == pid
                    && thread.id() != current_id
                    && thread.state() != ThreadState::Terminated
                {
                    has_other_live_threads = true;
                }
            });
            if !has_other_live_threads {
                crate::task::mark_process_exited(pid, exit_code);
                // 親プロセスが IPC でブロックしている可能性があるので起床させる
                wake_parent_ipc_waiter(pid);
                // 親プロセスへ SIGCHLD を送達する
                crate::syscall::signal::deliver_sigchld_to_parent(pid);
            }
        }

        // 現在のスレッドをクリア（先にクリアしないとschedule()が正しく動作しない）
        set_current_thread(None, None);

        x86_64::instructions::interrupts::without_interrupts(|| {
            // 次のスレッドにスケジューリング（戻ってこない）
            if let Some((next_id, next_slot)) = schedule_with_slot() {
                crate::debug!("Switching from exited thread to {:?}", next_id);

                // スレッドをキューから削除（コンテキストスイッチ前に削除）
                crate::syscall::process::clear_futex_waiter(current_id);
                let kstack_base = with_thread(current_id, |t| t.kernel_stack_base()).unwrap_or(0);
                remove_thread(current_id);

                // カーネルスタックをフリーリストへ返却（スイッチ直前、まだスタックは有効）
                crate::task::free_kernel_stack(kstack_base);

                // コンテキストスイッチを実行（終了したスレッドのコンテキストは保存しない）
                // old_context_ptr = None を渡すことで、現在のコンテキストを保存せずに次のスレッドにジャンプ
                unsafe {
                    switch_to_thread_with_slots(None, None, next_id, next_slot);
                }

                crate::audit::log(
                    crate::audit::AuditEventKind::Fault,
                    "scheduler switch_to_thread returned unexpectedly after exit",
                );
                x86_64::instructions::interrupts::disable();
                loop {
                    x86_64::instructions::hlt();
                }
            }
        });

        // スレッドをキューから削除
        crate::syscall::process::clear_futex_waiter(current_id);
        if let Some(thread) = remove_thread(current_id) {
            crate::task::free_kernel_stack(thread.kernel_stack_base());
        }
    }

    // スレッドがない場合は永久にhaltして待機
    crate::audit::log(
        crate::audit::AuditEventKind::Fault,
        "scheduler observed no more user threads",
    );
    x86_64::instructions::interrupts::disable();
    loop {
        x86_64::instructions::hlt();
    }
}

/// スケジューリングしてコンテキストスイッチを実行
///
/// タイマー割り込みハンドラから呼び出される
pub fn schedule_and_switch() {
    if !is_scheduler_enabled() {
        return;
    }

    x86_64::instructions::interrupts::without_interrupts(|| {
        let current = current_thread_id();
        let current_slot = current_thread_slot();

        // 次のスレッドを選択
        if let Some((next_id, next_slot)) = schedule_with_slot() {
            // 次のスレッドが現在のスレッドと異なる場合のみ切り替え
            if Some(next_id) != current {
                unsafe {
                    switch_to_thread_with_slots(current, current_slot, next_id, next_slot);
                }
            }
        }
    });
}

/// 最初のスレッドを起動
///
/// スケジューラを開始して最初のスレッドにジャンプ
pub fn start_scheduling() -> ! {
    // 最初のスレッドを選択
    if let Some(first_id) = super::thread::peek_next_thread() {
        x86_64::instructions::interrupts::without_interrupts(|| {
            // 最初のスレッドを Running 状態に設定
            set_thread_state(first_id, ThreadState::Running);
            with_thread(first_id, |thread| {
                crate::info!(
                    "Starting first thread: {} (id={:?})",
                    thread.name(),
                    thread.id()
                );
            });

            // 最初のスレッドへ switch_to_thread でジャンプ（戻ってこない）
            // user/kernel どちらも switch_context 経由で正しく動作する
            unsafe {
                if let Some((pid, interactive_score, cpu_burst_score)) =
                    with_thread(first_id, |thread| {
                        (
                            thread.process_id(),
                            thread.interactive_score(),
                            thread.cpu_burst_score(),
                        )
                    })
                {
                    let (process_priority, is_foreground) = crate::task::with_process(pid, |process| {
                        (process.priority(), process.is_foreground())
                    })
                    .unwrap_or((0, false));
                    let first_slice = time_slice_ticks_for_thread(
                        is_foreground,
                        process_priority,
                        interactive_score,
                        cpu_burst_score,
                    );
                    SCHEDULER.lock().set_time_slice(first_slice.max(1));
                }
                let first_slot = THREAD_QUEUE.lock().slot_index(first_id)
                    .expect("First thread must exist in queue");
                switch_to_thread_with_slots(None, None, first_id, first_slot);
            }
        });

        crate::audit::log(
            crate::audit::AuditEventKind::Fault,
            "start_scheduling switch_to_thread returned unexpectedly",
        );
        x86_64::instructions::interrupts::disable();
        loop {
            x86_64::instructions::hlt();
        }
    } else {
        crate::audit::log(
            crate::audit::AuditEventKind::Fault,
            "start_scheduling found no threads to schedule",
        );
        x86_64::instructions::interrupts::disable();
        loop {
            x86_64::instructions::hlt();
        }
    }
}

/// プロセス終了用のエイリアス（ページフォルトハンドラなどから呼び出される）
///
/// 現在のプロセス/スレッドを終了させる
pub fn exit_current_process(exit_code: i32) -> ! {
    exit_current_task(exit_code as u64)
}
