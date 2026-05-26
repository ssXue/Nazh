//! SOEM 编译脚本：根据目标平台选择 OSAL/OSHW 后端，编译为静态库并生成 FFI 绑定。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let soem_src = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("soem-src");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    generate_ec_options(&out_dir);

    let mut core_files: Vec<PathBuf> = [
        "src/ec_base.c",
        "src/ec_coe.c",
        "src/ec_config.c",
        "src/ec_dc.c",
        "src/ec_eoe.c",
        "src/ec_foe.c",
        "src/ec_main.c",
        "src/ec_print.c",
        "src/ec_soe.c",
    ]
    .iter()
    .map(|f| soem_src.join(f))
    .collect();

    let include_dir = soem_src.join("include");
    let target = env::var("TARGET").unwrap();
    let (osal_dir, oshw_dir, extra_libs) = if target.contains("linux") {
        let osal = soem_src.join("osal/linux");
        let oshw = soem_src.join("oshw/linux");
        core_files.push(osal.join("osal.c"));
        core_files.push(oshw.join("nicdrv.c"));
        core_files.push(oshw.join("oshw.c"));
        (osal, oshw, vec!["pthread"])
    } else if target.contains("darwin") {
        let osal = soem_src.join("contrib/osal/macosx");
        let oshw = soem_src.join("contrib/oshw/macosx");
        apply_macos_patches(&soem_src);
        core_files.push(osal.join("osal.c"));
        core_files.push(oshw.join("nicdrv.c"));
        core_files.push(oshw.join("oshw.c"));
        (osal, oshw, vec!["pcap"])
    } else if target.contains("windows") {
        let osal = soem_src.join("osal/win32");
        let oshw = soem_src.join("oshw/win32");
        core_files.push(osal.join("osal.c"));
        core_files.push(oshw.join("nicdrv.c"));
        core_files.push(oshw.join("oshw.c"));
        let wpcap_include = oshw.join("wpcap/Include");
        println!("cargo:include={}", wpcap_include.display());
        (osal, oshw, vec!["wpcap", "Ws2_32"])
    } else {
        panic!("不支持的 SOEM 目标平台: {target}");
    };

    let osal_common = soem_src.join("osal");

    let mut build = cc::Build::new();
    build
        .warnings(false)
        .opt_level(2)
        .include(&include_dir)
        .include(&osal_common)
        .include(&osal_dir)
        .include(&oshw_dir)
        .include(&out_dir);

    if target.contains("darwin") {
        if let Ok(sdk) = std::process::Command::new("xcrun")
            .args(["--show-sdk-path"])
            .output()
        {
            if sdk.status.success() {
                let sdk_path = String::from_utf8_lossy(&sdk.stdout).trim().to_owned();
                build.include(format!("{sdk_path}/usr/include"));
            }
        }
    }

    for file in &core_files {
        build.file(file);
    }
    build.compile("soem");

    for lib in &extra_libs {
        println!("cargo:rustc-link-lib={lib}");
    }

    let mut bindgen_builder = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", include_dir.display()))
        .clang_arg(format!("-I{}", osal_common.display()))
        .clang_arg(format!("-I{}", osal_dir.display()))
        .clang_arg(format!("-I{}", oshw_dir.display()))
        .clang_arg(format!("-I{}", out_dir.display()))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    if target.contains("darwin") {
        if let Ok(sdk) = std::process::Command::new("xcrun")
            .args(["--show-sdk-path"])
            .output()
        {
            if sdk.status.success() {
                let sdk_path = String::from_utf8_lossy(&sdk.stdout).trim().to_owned();
                bindgen_builder = bindgen_builder.clang_arg(format!("-I{sdk_path}/usr/include"));
            }
        }
    }

    let bindings = bindgen_builder.generate().expect("无法生成 SOEM FFI 绑定");

    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("无法写入 bindings.rs");

    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=patch/macosx/osal.c");
    println!("cargo:rerun-if-changed=patch/macosx/osal_defs.h");
    for file in &core_files {
        println!("cargo:rerun-if-changed={}", file.display());
    }
}

/// macOS OSAL 本地补丁：覆盖 submodule 中的上游原版。
///
/// 上游 SOEM 的 macOS OSAL 使用 `clock_nanosleep`（macOS 不支持），
/// 本补丁替换为 `nanosleep` 并补充 `ec_timet` / `osal_mutext` 类型定义。
///
/// 仅在文件内容不同时才写入，避免反复覆盖导致 submodule dirty →
/// Cargo 误判 build.rs 输出过期 → 无限重建循环。
fn apply_macos_patches(soem_src: &Path) {
    let patch_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("patch/macosx");
    let target_dir = soem_src.join("contrib/osal/macosx");
    for name in &["osal.c", "osal_defs.h"] {
        let src = patch_dir.join(name);
        let dst = target_dir.join(name);
        if src.exists() {
            let src_content = fs::read(&src).unwrap_or_else(|e| {
                panic!("读取补丁 {name} 失败: {e}");
            });
            let needs_write = match fs::read(&dst) {
                Ok(dst_content) => dst_content != src_content,
                Err(_) => true,
            };
            if needs_write {
                fs::write(&dst, &src_content).unwrap_or_else(|e| {
                    panic!("写入 macOS OSAL 补丁 {name} 失败: {e}");
                });
            }
        }
    }
}

/// 从 SOEM CMake 默认值生成 `ec_options.h`。
///
/// 仅在文件内容不同时才写入，避免更新 mtime 触发 Cargo 无限重建。
fn generate_ec_options(out_dir: &Path) {
    let soem_subdir = out_dir.join("soem");
    fs::create_dir_all(&soem_subdir).expect("无法创建 soem include 目录");

    let content = format!(r#"/*
 * Generated by soem-sys build.rs — defaults from SOEM CMakeLists.txt
 */
#ifndef _ec_options_
#define _ec_options_

#ifdef __cplusplus
extern "C" {{
#endif

#define EC_BUFSIZE 1514
#define EC_MAXBUF 16
#define EC_MAXEEPBITMAP 128
#define EC_MAXEEPBUF (EC_MAXEEPBITMAP << 5)
#define EC_LOGGROUPOFFSET 16
#define EC_MAXELIST 64
#define EC_MAXNAME 40
#define EC_MAXSLAVE 200
#define EC_MAXGROUP 2
#define EC_MAXIOSEGMENTS 64
#define EC_MAXMBX 1486
#define EC_MBXPOOLSIZE 32
#define EC_MAXEEPDO 0x200
#define EC_MAXSM 8
#define EC_MAXFMMU 4
#define EC_MAXLEN_ADAPTERNAME 128
#define EC_MAX_MAPT 1
#define EC_MAXODLIST 1024
#define EC_MAXOELIST 256
#define EC_SOE_MAXNAME 60
#define EC_SOE_MAXMAPPING 64

#define EC_TIMEOUTRET 2000
#define EC_TIMEOUTRET3 (EC_TIMEOUTRET * 3)
#define EC_TIMEOUTSAFE 20000
#define EC_TIMEOUTEEP 20000
#define EC_TIMEOUTTXM 20000
#define EC_TIMEOUTRXM 700000
#define EC_TIMEOUTSTATE 2000000
#define EC_DEFAULTRETRIES 3

#define EC_PRIMARY_MAC_ARRAY {{0x01,0x01,0x01,0x01,0x01,0x01}}
#define EC_SECONDARY_MAC_ARRAY {{0x04,0x04,0x04,0x04,0x04,0x04}}

#ifdef __cplusplus
}}
#endif

#endif
"#);

    write_if_changed(out_dir.join("soem/ec_options.h"), &content);
    write_if_changed(soem_subdir.join("ec_options.h"), &content);
}

/// 仅在内容不同时写入文件，避免无谓的 mtime 更新触发 Cargo 重建。
fn write_if_changed(path: PathBuf, content: &str) {
    let existing = fs::read(&path).ok();
    let new_bytes = content.as_bytes();
    if existing.as_deref() != Some(new_bytes) {
        fs::write(&path, new_bytes).unwrap_or_else(|e| {
            panic!("写入 {} 失败: {e}", path.display());
        });
    }
}
