// use nova_sandbox::*;
use clap::{App, Arg};
use std::process::Stdio;

fn main() {
    let matches = App::new("Nova Sandbox Bin")
        .version("0.1.1")
        .author("Woshiluo Luo <woshiluo.luo@outlook.com>")
        .about("use nova-sandbox to exec command in sandbox")
        .arg(
            Arg::with_name("rootfs")
                .short("r")
                .long("rootfs")
                .value_name("PATH")
                .help("Rootfs directory. If you don't have a rootfs, maybe this script https://gist.github.com/woshiluo/a4bb8d913b805feb665e4b5392ba6a92 will help you.")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("work")
                .short("w")
                .long("work")
                .value_name("PATH")
                .help("Work directory")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("target")
                .short("t")
                .long("target")
                .value_name("PATH")
                .help("The path which will be the mount point of sandbox")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("pids")
                .short("pid")
                .long("p")
                .value_name("INT")
                .help("Pids limit")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("time")
                .long("time")
                .value_name("INT")
                .help("Time limit in ms")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("memory")
                .long("memory")
                .short("m")
                .value_name("INT")
                .help("Memory limit in KiB")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("debug")
                .long("debug")
                .value_name("DEBUG LEVEL")
                .help("WARN/INFO/DEBUG/TRACE")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("command")
                .help("The command which you want to run.")
                .index(1)
                .required(true),
        )
        .get_matches();

    let mut builder = pretty_env_logger::formatted_builder();
    builder.parse_filters(matches.value_of("debug").unwrap_or_else(|| "INFO"));
    builder.try_init().unwrap();

    let config = nova_sandbox::SandboxConfig::new(
        matches.value_of("time").unwrap().parse::<u64>().unwrap(),
        matches.value_of("memory").unwrap().parse::<u64>().unwrap() * 1024,
        matches.value_of("pids").unwrap().parse::<u16>().unwrap(),
        matches.value_of("command").unwrap(),
        Stdio::inherit(),
        Stdio::inherit(),
        Stdio::inherit(),
    );
    log::trace!("{:?}", config);

    let sandbox = nova_sandbox::Sandbox::new(
        matches.value_of("rootfs").unwrap(),
        matches.value_of("work").unwrap(),
        matches.value_of("target").unwrap(),
    )
    .unwrap();

    let status = sandbox.run(config);
    log::info!("{:?}", status);
}
