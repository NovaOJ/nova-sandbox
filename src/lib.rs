//! Nova Sandbox
//!
//! 一个致力用于 OJ/判题环境 的 Sandbox
use std::error::Error;
use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::process::Stdio;

use time::prelude::*;

/// Sandbox 的配置
#[derive(Debug)]
pub struct SandboxConfig {
    /// Rootfs 目录
    pub rootfs_directory: String,
    /// 时间限制（以 ms 为单位）
    pub time_limit: u32,
    /// 内存限制（以 Byte 为单位）
    pub memory_limit: u64,

    /// Work Directory  
    /// 执行时只有这个目录是可写的  
    /// Rootfs 是以**只读方式**挂载的  
    /// **Warn: 程序对这个目录有全部权限**
    /// **Warn: Do not set "/tmp" value for this var**
    pub work_directory: String,

    /// 要执行的命令
    pub command: String,

    pub stdin: Stdio,
    pub stdout: Stdio,
    pub stderr: Stdio,
}

/// Sandbox 运行状态种类
#[derive(Debug)]
pub enum SandboxStatusKind {
    /// 超时
    TimeLimitExceeded,
    /// 内存超限
    MemoryLimitExceeded,
    /// 运行时错误/返回值非 0
    RuntimeError,
    /// 正常
    Success, // tle > mle > re > seccess
}

/// 沙箱运行状态
#[derive(Debug)]
pub struct SandboxStatus {
    /// 分类
    pub status: SandboxStatusKind,
    /// 使用时间
    pub used_time: i128,
    /// 使用内存
    pub max_memory: u64,
    /// 程序返回值
    pub return_code: i32,
}

fn create_sandbox(
    config: &SandboxConfig,
    target_rootfs_directory: &str,
    target_work_directory: &str,
) -> Result<(), Box<dyn Error>> {
    let str_none: Option<&str> = None;

    log::info!("Init Sandbox");
    // Create && Check Directory
    if std::path::Path::new("/sys/fs/cgroup/memory/memory.memsw.usage_in_bytes").exists() == false {
        log::error!("Need \"cgroup_enable=memory swapaccount=1\" kernel parameter");
        return Err(String::from("Didn't have sawpaccount!").into());
    }

    // Check
    if std::path::Path::new(&config.rootfs_directory).exists() == false {
        log::error!("Rootfs Directory isn't exist!");
        return Err(String::from("Rootfs Directory isn't exist!").into());
    }
    if std::path::Path::new(&config.work_directory).exists() == false {
        log::error!("Work Directory isn't exist!");
        return Err(String::from("Work Directory isn't exist!").into());
    }

    // Create
    if std::path::Path::new(target_rootfs_directory).exists() == false {
        std::fs::create_dir(target_rootfs_directory)?;
    }
    if std::path::Path::new(target_work_directory).exists() == false {
        std::fs::create_dir(target_work_directory)?;
    }

    // Mount Directory
    // Rootfs
    nix::mount::mount(
        Some(OsStr::new(&config.rootfs_directory)),
        OsStr::new(target_rootfs_directory),
        str_none,
        nix::mount::MsFlags::MS_RDONLY | nix::mount::MsFlags::MS_BIND | nix::mount::MsFlags::MS_REC,
        str_none,
    )?;
    nix::mount::mount(
        str_none,
        OsStr::new(target_rootfs_directory),
        str_none,
        nix::mount::MsFlags::MS_RDONLY
            | nix::mount::MsFlags::MS_BIND
            | nix::mount::MsFlags::MS_REMOUNT
            | nix::mount::MsFlags::MS_REC,
        str_none,
    )?;
    // Work
    nix::mount::mount(
        Some(OsStr::new(&config.work_directory)),
        OsStr::new(target_work_directory),
        str_none,
        nix::mount::MsFlags::MS_BIND | nix::mount::MsFlags::MS_REC,
        str_none,
    )?;
    log::info!("Done!");

    Ok(())
}

fn exec_sandbox(
    config: SandboxConfig,
    target_rootfs_directory: &str,
    cgroup_name: &str,
) -> Result<SandboxStatus, Box<dyn Error>> {
    use cgroups_fs::CgroupsCommandExt;

    let time_limit = ((config.time_limit + 500) / 1000 + 1).to_string();
    let mut status = SandboxStatusKind::Success;

    // Create new cgroup
    log::debug!("New cgroup {} create", cgroup_name);
    let cur_cgroup = cgroups_fs::CgroupName::new(cgroup_name);
    let cur_cgroup = cgroups_fs::AutomanagedCgroup::init(&cur_cgroup, "memory")?;
    log::debug!("Memory Limit {}", config.memory_limit * 2);
    cur_cgroup.set_value("memory.limit_in_bytes", config.memory_limit * 2)?;
    cur_cgroup.set_value("memory.memsw.limit_in_bytes", config.memory_limit * 2)?;

    // Run command
    log::info!(
        "Chroot {} to run '{}'",
        target_rootfs_directory,
        config.command
    );

    let time_start = time::Instant::now();
    let return_code = std::process::Command::new("timeout")
        .args(&[&time_limit, "bash", "-c", &config.command])
        .cgroups(&[&cur_cgroup])
        .chroot(String::from(target_rootfs_directory))
        .chdir(String::from(format!("/sandbox-{}", cgroup_name)))
        .stdin(config.stdin)
        .stdout(config.stdout)
        .stderr(config.stderr)
        .status()?
        .code();
    let time_end = time::Instant::now();

    log::debug!("Return code: {:?}", return_code);
    log::debug!(
        "Start at {:?}, End at {:?}, Use at {:?}",
        time_start,
        time_end,
        time_end - time_start
    );

    // Get return code
    let return_code = match return_code {
        // Rust Crashes
        Some(101) => {
            log::error!("Failed to run command");
            return Err(String::from("Failed to run command").into());
        }
        Some(code) => code,
        None => -1,
    };
    if return_code != 0 {
        status = SandboxStatusKind::RuntimeError;
    }

    // Calc Memory
    let max_memory = cur_cgroup.get_value::<u64>("memory.memsw.max_usage_in_bytes")?;
    if max_memory > config.memory_limit {
        status = SandboxStatusKind::MemoryLimitExceeded;
    }

    // Calc time
    let used_time = time_end - time_start;
    log::debug!("{:?}", used_time);
    if used_time > config.time_limit.milliseconds() {
        status = SandboxStatusKind::TimeLimitExceeded;
    }
    let used_time = used_time.whole_milliseconds();

    Ok(SandboxStatus {
        status,
        max_memory,
        used_time,
        return_code,
    })
}

fn delete_sandbox(
    target_rootfs_directory: &str,
    target_work_directory: &str,
) -> Result<(), Box<dyn Error>> {
    log::info!("Remove Sandbox");
    // Umount rootfs
    let handle_err = |err| {
        log::error!("Failed to umount sandbox: {}", err);
        println!("{}", err);
    };

    nix::mount::umount2(
        OsStr::new(target_work_directory),
        nix::mount::MntFlags::MNT_DETACH,
    )
    .unwrap_or_else(handle_err);
    nix::mount::umount2(
        OsStr::new(target_rootfs_directory),
        nix::mount::MntFlags::MNT_DETACH,
    )
    .unwrap_or_else(handle_err);

    // Remove Directory
    let handle_err = |err| {
        log::error!("Failed to remove sandbox: {}", err);
    };
    std::fs::remove_dir(target_work_directory).unwrap_or_else(handle_err);
    std::fs::remove_dir(target_rootfs_directory).unwrap_or_else(handle_err);
    log::info!("Done!");
    Ok(())
}

/// 根据 SandboxConfig 来执行命令
///
/// # Examples
///
/// ```
/// use std::process::Stdio;
/// use nova_sandbox::*;
///
/// let work_directory = String::from("/work/novaoj/nova-sandbox/tests");
/// let rootfs_directory = String::from("/work/package/debs/debian-rootfs/");
/// sandbox_run( SandboxConfig{
///     rootfs_directory,
///     time_limit: 1000,
///     memory_limit: 512 * 1024 * 1024,
///
///     work_directory: work_directory,
///
///     command: "echo \"Hello, World\"".to_string(),
///
///     stdin: Stdio::null(),
///     stdout: Stdio::inherit(),
///     stderr: Stdio::inherit() } ).unwrap_or_else( |err|{
///         log::error!( "Failed to run sandbox: {}", err );
///         panic!( "{}", err );
/// });
/// ```
pub fn sandbox_run(config: SandboxConfig) -> Result<SandboxStatus, Box<dyn Error>> {
    let cgroup_name = uuid::Uuid::new_v4().to_string();
    let target_rootfs_directory = &format!("/tmp/sandbox-{}", cgroup_name);
    let target_work_directory = &format!("{}/sandbox-{}", config.rootfs_directory, cgroup_name);

    match create_sandbox(&config, &target_rootfs_directory, &target_work_directory) {
        Ok(()) => (),
        Err(err) => {
            delete_sandbox(&target_rootfs_directory, &target_work_directory).unwrap();
            log::error!("Failed create sandbox!");
            return Err(String::from(format!("Failed create sandbox: {}", err)).into());
        }
    };

    let sanbox_status;
    match exec_sandbox(config, &target_rootfs_directory, &cgroup_name) {
        Ok(status) => sanbox_status = status,
        Err(err) => {
            delete_sandbox(&target_rootfs_directory, &target_work_directory).unwrap();
            log::error!("Failed run command in sandbox!");
            return Err(String::from(format!("Failed run command in sandbox: {}", err)).into());
        }
    };

    delete_sandbox(&target_rootfs_directory, &target_work_directory)?;

    Ok(sanbox_status)
}

pub trait SandboxCommandExt {
    fn chroot(&mut self, dir: String) -> &mut Self;
    fn chdir(&mut self, dir: String) -> &mut Self;
}

impl SandboxCommandExt for std::process::Command {
    /// 用于 Command 执行前 Chroot 进入沙箱  
    /// 应该在所有需要修改/读取 sysfs/procfs 的函数之后使用
    fn chroot(&mut self, dir: String) -> &mut Self {
        unsafe {
            self.pre_exec(move || {
                nix::unistd::chroot(OsStr::new(&dir)).unwrap();
                Ok(())
            })
        }
    }
    /// 用于在 Chroot 之后确定目录  
    /// 应在 `SandboxCommandExt::chroot()` 后使用
    fn chdir(&mut self, dir: String) -> &mut Self {
        unsafe {
            self.pre_exec(move || {
                nix::unistd::chdir(OsStr::new(&dir)).unwrap();
                Ok(())
            })
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn hello_world() {
        sandbox_run(SandboxConfig {
            rootfs_directory: "/work/package/debs/debian-rootfs/".to_string(),
            time_limit: 1000,
            memory_limit: 512 * 1024 * 1024,

            work_directory: "/home/woshiluo/tmp".to_string(),

            command: "echo \"Hello, World\"".to_string(),

            stdin: Stdio::null(),
            stdout: Stdio::inherit(),
            stderr: Stdio::inherit(),
        })
        .unwrap_or_else(|err| {
            log::error!("Failed to run sandbox: {}", err);
            panic!("{}", err);
        });
    }
}
