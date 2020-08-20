use nova_sandbox::*;

mod common;

#[test]
fn hello_world() {
    pretty_env_logger::init();
    let status = common::run_sandbox("echo 'Hello, World!'");
    log::debug!("{:?}", status);
    if let SandboxStatusKind::Success = status.status {
        log::info!("Test success");
    } else {
        panic!("Wrong return type!");
    }
}

#[test]
fn time_limit() {
    let status = common::run_sandbox("sleep 2");
    log::debug!("{:?}", status);
    if let SandboxStatusKind::TimeLimitExceeded = status.status {
        log::info!("Test success");
    } else {
        panic!("Wrong return type!");
    }
}

#[test]
fn memory_limit() {
    let status = common::run_sandbox("for i in $(seq 1 10000000000); do echo $i; done;");
    log::debug!("{:?}", status);
    if let SandboxStatusKind::MemoryLimitExceeded = status.status {
        log::info!("Test success");
    } else {
        panic!("Wrong return type!");
    }
}

#[test]
fn run_time() {
    let status = common::run_sandbox("exit -1");
    log::debug!("{:?}", status);
    if let SandboxStatusKind::RuntimeError = status.status {
        log::info!("Test success");
    } else {
        panic!("Wrong return type!");
    }
}
