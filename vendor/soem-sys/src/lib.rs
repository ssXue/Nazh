//! SOEM FFI 绑定 —— 由 build.rs 自动生成。
//!
//! 原始绑定通过 `include!` 引入；本文件额外提供线程安全声明和高层安全封装。

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(dead_code)]
#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

// SOEM context 通过内部 OSAL mutex 保证线程安全。
unsafe impl Send for ecx_context {}
unsafe impl Sync for ecx_context {}

/// `*mut ecx_context` 的 Send 包装，用于传递给后台线程。
/// 生命周期由 SoemBackend 管理：线程退出后才 drop context。
#[derive(Copy, Clone)]
pub struct ContextPtr(pub *mut ecx_context);
unsafe impl Send for ContextPtr {}

impl ContextPtr {
    /// 调用 SOEM send_processdata。
    pub fn send_processdata(self) -> i32 {
        unsafe { ecx_send_processdata(self.0) }
    }

    /// 调用 SOEM receive_processdata。
    pub fn receive_processdata(self, timeout: i32) -> i32 {
        unsafe { ecx_receive_processdata(self.0, timeout) }
    }
}

/// 安全零初始化 ecx_context。
pub fn zero_context() -> ecx_context {
    let mut ctx = std::mem::MaybeUninit::<ecx_context>::uninit();
    unsafe { ctx.as_mut_ptr().write_bytes(0, 1) };
    unsafe { ctx.assume_init() }
}

/// 安全封装：ecx_writestate 从 slavelist[slave].state 读取期望状态并写入。
pub fn safe_ecx_writestate(context: &mut ecx_context, slave: u16) -> i32 {
    unsafe { ecx_writestate(context, slave) }
}

/// 安全封装：ecx_init。
pub fn safe_ecx_init(context: &mut ecx_context, ifname: *const std::os::raw::c_char) -> i32 {
    unsafe { ecx_init(context, ifname) }
}

/// 安全封装：ecx_close。
pub fn safe_ecx_close(context: &mut ecx_context) {
    unsafe { ecx_close(context) }
}

/// 安全封装：ecx_config_init。
pub fn safe_ecx_config_init(context: &mut ecx_context) -> i32 {
    unsafe { ecx_config_init(context) }
}

/// 安全封装：ecx_config_map_group。
pub fn safe_ecx_config_map_group(
    context: &mut ecx_context,
    pIOmap: *mut std::ffi::c_void,
    group: u8,
) -> i32 {
    unsafe { ecx_config_map_group(context, pIOmap, group) }
}

/// 安全封装：ecx_configdc。
pub fn safe_ecx_configdc(context: &mut ecx_context) -> u8 {
    unsafe { ecx_configdc(context) }
}

/// 安全封装：ecx_statecheck。
pub fn safe_ecx_statecheck(
    context: &mut ecx_context,
    slave: u16,
    reqstate: u16,
    timeout: i32,
) -> u16 {
    unsafe { ecx_statecheck(context, slave, reqstate, timeout) }
}

/// 安全封装：ecx_readstate。
pub fn safe_ecx_readstate(context: &mut ecx_context) -> i32 {
    unsafe { ecx_readstate(context) }
}

/// 安全封装：ecx_send_processdata。
pub fn safe_ecx_send_processdata(context: &mut ecx_context) -> i32 {
    unsafe { ecx_send_processdata(context) }
}

/// 安全封装：ecx_receive_processdata。
pub fn safe_ecx_receive_processdata(context: &mut ecx_context, timeout: i32) -> i32 {
    unsafe { ecx_receive_processdata(context, timeout) }
}

/// 读取从站名称（C char 数组 → Rust String）。
pub fn read_slave_name(slave: &ec_slave) -> String {
    let ptr = slave.name.as_ptr() as *const u8;
    let len = (0..41)
        .position(|j| unsafe { *ptr.add(j) == 0 })
        .unwrap_or(41);
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len) })
        .unwrap_or("unknown")
        .to_owned()
}

/// 读取从站输入数据。
pub fn read_slave_inputs(slave: &ec_slave) -> Vec<u8> {
    let len = slave.Ibytes as usize;
    if len == 0 {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(slave.inputs, len) }.to_vec()
}

/// 写入从站输出数据。
pub fn write_slave_outputs(slave: &ec_slave, data: &[u8]) -> bool {
    let len = slave.Obytes as usize;
    if data.len() != len || len == 0 {
        return false;
    }
    unsafe { std::slice::from_raw_parts_mut(slave.outputs, len) }.copy_from_slice(data);
    true
}

/// 后台周期 TX/RX 线程运行体。在 soem-sys 中定义以封装 `*mut ecx_context` 的 Send。
pub fn cyclic_loop(
    context: ContextPtr,
    shutdown_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    cycle_duration: std::time::Duration,
) -> Result<std::thread::JoinHandle<()>, String> {
    let cycle_us = cycle_duration.as_micros() as u64;

    std::thread::Builder::new()
        .name("soem-cyclic".to_owned())
        .spawn(move || {
            let mut consecutive_errors = 0u64;
            while !shutdown_flag.load(std::sync::atomic::Ordering::Relaxed) {
                let expected_wk = context.send_processdata();
                let wk = context.receive_processdata(EC_TIMEOUTRET as i32);
                if wk < 0 || (expected_wk > 0 && wk != expected_wk) {
                    consecutive_errors = consecutive_errors.saturating_add(1);
                    if consecutive_errors == 1 || consecutive_errors.is_multiple_of(100) {
                        tracing::warn!(wk, expected_wk, consecutive_errors, "SOEM 周期刷新失败");
                    }
                } else {
                    consecutive_errors = 0;
                }

                if cycle_us > 50 {
                    std::thread::sleep(std::time::Duration::from_micros(cycle_us));
                } else {
                    std::hint::spin_loop();
                }
            }
        })
        .map_err(|e| format!("创建 SOEM 后台线程失败: {e}"))
}
