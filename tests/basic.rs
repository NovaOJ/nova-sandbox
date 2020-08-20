use nova_sandbox::*;
use std::fs;
use std::process::Stdio;

#[test]
fn test_pid() {
    pretty_env_logger::init();
    let work_directory = format!("/tmp/{}", uuid::Uuid::new_v4().to_string());
    let sandbox_directory = format!("/tmp/{}", uuid::Uuid::new_v4().to_string());
    fs::create_dir(&work_directory).unwrap();
    fs::create_dir(&sandbox_directory).unwrap();

    let sandbox = Sandbox::new(
        "/work/package/debs/debian-rootfs",
        &work_directory,
        &sandbox_directory,
    )
    .unwrap();

    let config = SandboxConfig::new(
        1000,
        512 * 1024 * 1024,
        1,
        "echo 'Hello, World'".to_string(),
        //"for i in $(seq 1 1000000000); do echo $i; done".to_string(),
        //"sleep 2".to_string(),
        Stdio::inherit(),
        Stdio::inherit(),
        Stdio::inherit(),
    );
    println!("{:?}", sandbox.run(config).unwrap());

    drop(sandbox);
    fs::remove_dir_all(work_directory).unwrap();
    fs::remove_dir_all(sandbox_directory).unwrap();
}
