use std::path::PathBuf;
use testscript_rs::testscript;

#[test]
fn testscripts() {
    // Get the path to the target/debug directory
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target_dir = manifest_dir.join("target").join("debug");

    testscript::run("tests/testdata")
        .setup(move |env| {
            // Build krust binary
            std::process::Command::new("cargo")
                .args(["build", "--bin", "krust"])
                .status()
                .expect("Failed to build krust");

            // Copy binary to test work directory so it's available in PATH
            let krust_src = target_dir.join("krust");
            let krust_dst = env.work_dir.join("krust");

            std::fs::copy(&krust_src, &krust_dst)?;

            // Make it executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&krust_dst)?.permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&krust_dst, perms)?;
            }

            Ok(())
        })
        .execute()
        .unwrap();
}
