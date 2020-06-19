// Here is an example.

use nova_sandbox::Sandbox;
use nova_sandbox::SandboxConfig;
use std::process::Stdio;

fn main() {
    pretty_env_logger::init();
    log::info!("Start Program");
    let sandbox =
        Sandbox::create("/work/package/debs/debian-rootfs", "/home/woshiluo/tmp").unwrap();

    let status = sandbox
        .exec(SandboxConfig {
            time_limit: 10000,
            memory_limit: 1024 * 1024 * 512,

            //        command: "echo \"Hello, World\"".to_string(),
            //        command: String::from( "sleep 5" ),
            //        command: String::from( "ls -l /tmp" ),
            //        command: String::from( "echo 1 > /qwq" ),
            //        command: String::from( "rm /tmp/a.out" ),
            //command: String::from("ls"),
            //command: String::from("g++ temp.cpp"),
            command: String::from("./a.out"),
            stdin: Stdio::null(),
            stdout: Stdio::inherit(),
            stderr: Stdio::inherit(),
        })
        .unwrap_or_else(|err| {
            log::error!("Failed to run sandbox: {}", err);
            panic!("{}", err);
        });

    log::debug!("{:?}", status);
}
