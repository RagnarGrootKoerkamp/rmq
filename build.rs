// Compiles the C++ RMQ libraries behind the FFI modules:
// - BbST (BbST/) through ffi/bbst_shim.cpp, once per value type and variant.
//   See src/bbst.rs.
// - FL-RMQ (FL-RMQ/) through ffi/flrmq_shim.cpp, once per value type, with
//   every epsilon in FLRMQ_EPSILONS. See src/flrmq.rs.

const TYPES: &[(&str, &str, &str)] = &[
    ("u8", "uint8_t", "UINT8_MAX"),
    ("u16", "uint16_t", "UINT16_MAX"),
    ("u32", "uint32_t", "UINT32_MAX"),
    ("u64", "uint64_t", "UINT64_MAX"),
    ("i32", "int32_t", "INT32_MAX"),
];

/// Epsilons FL-RMQ is instantiated for; `FLRMQFamily<EPSILON>` accepts only these.
const FLRMQ_EPSILONS: &[usize] = &[16, 32, 64, 128, 256, 512, 1024, 2048, 4096];

fn main() {
    println!("cargo:rerun-if-changed=ffi");
    println!("cargo:rerun-if-changed=BbST/bbst.cpp");
    println!("cargo:rerun-if-changed=BbST/bbst.h");
    println!("cargo:rerun-if-changed=FL-RMQ/include");

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let native = native_cpu_flag();
    build_bbst(&out_dir, native);
    build_flrmq(&out_dir, native);
}

/// The C++ counterpart of `-C target-cpu=native` (set in .cargo/config.toml), which
/// cc doesn't forward. -O3 already enables the loop and SLP vectorizers.
fn native_cpu_flag() -> &'static str {
    assert_eq!(
        std::env::var("HOST").unwrap(),
        std::env::var("TARGET").unwrap(),
        "the C++ libraries are compiled for the local CPU, so cross-compiling isn't supported"
    );
    // On AArch64, -mcpu selects both the instruction set and the tuning.
    let candidates: &[&str] = if std::env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "aarch64" {
        &["-mcpu=native", "-march=native"]
    } else {
        &["-march=native"]
    };
    let mut probe = cc::Build::new();
    probe.cpp(true);
    candidates
        .iter()
        .copied()
        .find(|f| probe.is_flag_supported(f).unwrap_or(false))
        .expect("the C++ compiler supports neither -march=native nor -mcpu=native")
}

fn build_bbst(out_dir: &std::path::Path, native: &str) {
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++11")
        .opt_level(3)
        .flag(native)
        // ffi/include first, so its empty omp.h is used instead of the real one.
        .include("ffi/include")
        .include("BbST")
        .flag_if_supported("-Wno-unknown-pragmas")
        .flag_if_supported("-Wno-sign-compare")
        .flag_if_supported("-Wno-mismatched-new-delete")
        .flag_if_supported("-Wno-shift-op-parentheses")
        .flag_if_supported("-Wno-dangling-else")
        .flag_if_supported("-Wno-parentheses")
        .flag_if_supported("-Wno-unused-private-field");

    for &(rust_ty, c_ty, c_max) in TYPES {
        for mini in [false, true] {
            let prefix = format!("bbst{}_{rust_ty}", if mini { "2" } else { "" });
            // One wrapper TU per instance, so cc gives each its own object file.
            let wrapper = out_dir.join(format!("{prefix}.cpp"));
            let shim = std::fs::canonicalize("ffi/bbst_shim.cpp").unwrap();
            let mini_def = if mini { "#define MINI_BLOCKS\n" } else { "" };
            std::fs::write(
                &wrapper,
                format!(
                    "{mini_def}#define BBST_VALUE_T {c_ty}\n#define BBST_VALUE_MAX {c_max}\n\
                     #define BBST_PREFIX {prefix}\n#include \"{}\"\n",
                    shim.display()
                ),
            )
            .unwrap();
            build.file(wrapper);
        }
    }
    build.compile("bbst");
}

fn build_flrmq(out_dir: &std::path::Path, native: &str) {
    let sdsl = "FL-RMQ/libs/sdsl-lite/include";
    assert!(
        std::path::Path::new(sdsl).is_dir(),
        "FL-RMQ needs sdsl-lite: run `git submodule update --init libs/sdsl-lite` in FL-RMQ/"
    );

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++20")
        .opt_level(3)
        .flag(native)
        .include("ffi/include")
        .include("FL-RMQ/include")
        .include(sdsl)
        .flag_if_supported("-Wno-unknown-pragmas")
        .flag_if_supported("-Wno-sign-compare")
        .flag_if_supported("-Wno-unused-variable")
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-#warnings")
        .flag_if_supported("-Wno-shift-op-parentheses");

    let shim = std::fs::canonicalize("ffi/flrmq_shim.cpp").unwrap();
    let for_each_eps: String = FLRMQ_EPSILONS.iter().map(|e| format!(" X({e})")).collect();
    let mut rust_ops = String::new();
    for &(rust_ty, c_ty, _) in TYPES {
        let wrapper = out_dir.join(format!("flrmq_{rust_ty}.cpp"));
        std::fs::write(
            &wrapper,
            format!(
                "#define FLRMQ_VALUE_T {c_ty}\n#define FLRMQ_NAME {rust_ty}\n\
                 #define FLRMQ_FOR_EACH_EPS(X){for_each_eps}\n#include \"{}\"\n",
                shim.display()
            ),
        )
        .unwrap();
        build.file(wrapper);
        rust_ops += &format!("flrmq_value!({rust_ty}, {FLRMQ_EPSILONS:?});\n");
    }
    build.compile("flrmq");
    // Declares the matching extern functions on the Rust side.
    std::fs::write(out_dir.join("flrmq_ops.rs"), rust_ops).unwrap();
}
