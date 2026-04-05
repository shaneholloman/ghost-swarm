use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

/// Pinned ghostty commit. Update this to pull a newer version.
const GHOSTTY_REPO: &str = "https://github.com/ghostty-org/ghostty.git";
const GHOSTTY_COMMIT: &str = "bebca84668947bfc92b9a30ed58712e1c34eee1d";
const ZIG_VERSION: &str = "0.15.2";

fn main() {
    // docs.rs has no Zig toolchain. The checked-in bindings in src/bindings.rs
    // are enough for generating documentation, so skip the entire native
    // build when running under docs.rs.
    if env::var("DOCS_RS").is_ok() {
        return;
    }

    println!("cargo:rerun-if-env-changed=LIBGHOSTTY_VT_SYS_NO_VENDOR");
    println!("cargo:rerun-if-env-changed=GHOSTTY_SOURCE_DIR");
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-env-changed=HOST");
    println!("cargo:rerun-if-changed=crates/libghostty-vt-sys/build.rs");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR must be set"));
    let target = env::var("TARGET").expect("TARGET must be set");
    let host = env::var("HOST").expect("HOST must be set");

    ensure_zig_version();

    // Locate ghostty source: env override > fetch into OUT_DIR.
    let ghostty_dir = match env::var("GHOSTTY_SOURCE_DIR") {
        Ok(dir) => {
            let p = PathBuf::from(dir);
            assert!(
                p.join("build.zig").exists(),
                "GHOSTTY_SOURCE_DIR does not contain build.zig: {}",
                p.display()
            );
            p
        }
        Err(_) => fetch_ghostty(&out_dir),
    };

    // Build libghostty-vt via zig.
    let install_prefix = out_dir.join("ghostty-install");

    let mut build = Command::new("zig");
    build
        .arg("build")
        .arg("-Demit-lib-vt")
        .arg("-Doptimize=ReleaseFast")
        .arg("--prefix")
        .arg(&install_prefix)
        .current_dir(&ghostty_dir);

    // Only pass -Dtarget when cross-compiling. For native builds, let zig
    // auto-detect the host (matches how ghostty's own CMakeLists.txt works).
    if target != host {
        let zig_target = zig_target(&target);
        build.arg(format!("-Dtarget={zig_target}"));
    }

    run(build, "zig build");

    let lib_dir = install_prefix.join("lib");
    let include_dir = install_prefix.join("include");

    let lib_name = if target.contains("darwin") {
        "libghostty-vt.0.1.0.dylib"
    } else {
        "libghostty-vt.so.0.1.0"
    };

    assert!(
        lib_dir.join(lib_name).exists(),
        "expected shared library at {}",
        lib_dir.join(lib_name).display()
    );
    assert!(
        include_dir.join("ghostty").join("vt.h").exists(),
        "expected header at {}",
        include_dir.join("ghostty").join("vt.h").display()
    );

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static:+whole-archive=ghostty-vt");

    let dep_search_roots = [
        install_prefix.join("lib"),
        ghostty_dir.join(".zig-cache"),
        ghostty_dir.join("zig-out"),
        out_dir.clone(),
    ];
    emit_static_dep(&dep_search_roots, "libsimdutf.a", "simdutf");
    emit_static_dep(&dep_search_roots, "libhighway.a", "highway");
    emit_static_dep(&dep_search_roots, "libutfcpp.a", "utfcpp");

    println!("cargo:rustc-link-lib=dylib=stdc++");
    println!("cargo:include={}", include_dir.display());
}

fn emit_static_dep(search_roots: &[PathBuf], archive_name: &str, lib_name: &str) {
    for root in search_roots {
        if let Some(archive_path) = find_file(root, archive_name) {
            if let Some(parent) = archive_path.parent() {
                println!("cargo:rustc-link-search=native={}", parent.display());
                println!("cargo:rustc-link-lib=static={lib_name}");
                return;
            }
        }
    }

    panic!(
        "failed to locate {archive_name} in {}",
        format_search_roots(search_roots)
    );
}

fn find_file(root: &Path, needle: &str) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];

    while let Some(path) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };

        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                stack.push(entry_path);
                continue;
            }

            if entry_path.file_name().and_then(|name| name.to_str()) == Some(needle) {
                return Some(entry_path);
            }
        }
    }

    None
}

fn format_search_roots(search_roots: &[PathBuf]) -> String {
    search_roots
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Clone ghostty at the pinned commit into OUT_DIR/ghostty-src.
/// Reuses an existing clone if the commit matches.
fn fetch_ghostty(out_dir: &Path) -> PathBuf {
    let src_dir = out_dir.join("ghostty-src");
    let stamp = src_dir.join(".ghostty-commit");

    // Skip fetch if we already have the right commit.
    if stamp.exists()
        && let Ok(existing) = std::fs::read_to_string(&stamp)
        && existing.trim() == GHOSTTY_COMMIT
    {
        return src_dir;
    }

    // Clean and clone fresh.
    if src_dir.exists() {
        std::fs::remove_dir_all(&src_dir)
            .unwrap_or_else(|e| panic!("failed to remove {}: {e}", src_dir.display()));
    }

    eprintln!("Fetching ghostty {GHOSTTY_COMMIT} ...");

    let mut clone = Command::new("git");
    clone
        .arg("clone")
        .arg("--filter=blob:none")
        .arg("--no-checkout")
        .arg(GHOSTTY_REPO)
        .arg(&src_dir);
    run(clone, "git clone ghostty");

    let mut checkout = Command::new("git");
    checkout
        .arg("checkout")
        .arg(GHOSTTY_COMMIT)
        .current_dir(&src_dir);
    run(checkout, "git checkout ghostty commit");

    std::fs::write(&stamp, GHOSTTY_COMMIT).unwrap_or_else(|e| panic!("failed to write stamp: {e}"));

    src_dir
}

fn run(mut command: Command, context: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("failed to execute {context}: {error}"));
    assert!(status.success(), "{context} failed with status {status}");
}

fn ensure_zig_version() {
    let output = Command::new("zig")
        .arg("version")
        .output()
        .unwrap_or_else(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                panic!("failed to execute zig version: zig {ZIG_VERSION} is required but was not found in PATH");
            }
            _ => panic!("failed to execute zig version: {error}"),
        });
    assert!(
        output.status.success(),
        "zig version failed with status {}",
        output.status
    );

    let version = String::from_utf8(output.stdout)
        .unwrap_or_else(|error| panic!("zig version output was not valid UTF-8: {error}"));
    let version = version.trim();
    assert!(
        version == ZIG_VERSION,
        "zig {ZIG_VERSION} is required, found zig {version}"
    );
}

fn zig_target(target: &str) -> String {
    let value = match target {
        "x86_64-unknown-linux-gnu" => "x86_64-linux-gnu",
        "x86_64-unknown-linux-musl" => "x86_64-linux-musl",
        "aarch64-unknown-linux-gnu" => "aarch64-linux-gnu",
        "aarch64-unknown-linux-musl" => "aarch64-linux-musl",
        "aarch64-apple-darwin" => "aarch64-macos-none",
        "x86_64-apple-darwin" => "x86_64-macos-none",
        other => panic!("unsupported Rust target for vendored build: {other}"),
    };
    value.to_owned()
}
