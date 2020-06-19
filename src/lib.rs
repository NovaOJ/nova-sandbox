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
    /// 时间限制（以 ms 为单位）
    pub time_limit: u32,
    /// 内存限制（以 Byte 为单位）
    pub memory_limit: u64,

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

pub struct Sandbox {
    /// Rootfs 目录
    pub sandbox_id: String,
    pub rootfs_directory: String,
    /// Work Directory  
    /// 执行时只有这个目录是可写的  
    /// Rootfs 是以**只读方式**挂载的  
    /// **Warn: 程序对这个目录有全部权限**
    /// **Warn: Do not set "/tmp" value for this var**
    pub work_directory: String,
    pub cur_cgroup: cgroups_fs::AutomanagedCgroup,
}

impl Sandbox {
    //{{{
    pub fn create(rootfs_directory: &str, work_directory: &str) -> Result<Sandbox, Box<dyn Error>> {
        //{{{
        let sandbox_id = uuid::Uuid::new_v4().to_string();
        let target_rootfs_directory = &format!("/tmp/sandbox-{}", sandbox_id);
        let target_work_directory = &format!("{}/sandbox-{}", rootfs_directory, sandbox_id);

        let str_none: Option<&str> = None;
        log::info!("Init Sandbox");
        // Create && Check Directory

        // Check swapaccount
        if std::path::Path::new("/sys/fs/cgroup/memory/memory.memsw.usage_in_bytes").exists()
            == false
        {
            log::error!("Need \"cgroup_enable=memory swapaccount=1\" kernel parameter");
            return Err(String::from("Didn't have sawpaccount!").into());
        }

        // Check
        if std::path::Path::new(rootfs_directory).exists() == false {
            log::error!("Rootfs Directory isn't exist!");
            return Err(String::from("Rootfs Directory isn't exist!").into());
        }
        if std::path::Path::new(work_directory).exists() == false {
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
        log::debug!("Create Dir Done!");

        // Mount Directory
        // Rootfs
        nix::mount::mount(
            Some(OsStr::new(rootfs_directory)),
            OsStr::new(target_rootfs_directory),
            str_none,
            nix::mount::MsFlags::MS_RDONLY
                | nix::mount::MsFlags::MS_BIND
                | nix::mount::MsFlags::MS_REC,
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
            Some(OsStr::new(work_directory)),
            OsStr::new(target_work_directory),
            str_none,
            nix::mount::MsFlags::MS_BIND | nix::mount::MsFlags::MS_REC,
            str_none,
        )?;
        log::info!("Done!");

        // Create new cgroup
        log::debug!("New cgroup {} create", sandbox_id);
        let cur_cgroup = cgroups_fs::CgroupName::new(&sandbox_id);
        let cur_cgroup = cgroups_fs::AutomanagedCgroup::init(&cur_cgroup, "memory")?;

        Ok(Sandbox {
            sandbox_id,
            cur_cgroup,
            rootfs_directory: String::from(target_rootfs_directory),
            work_directory: String::from(target_work_directory),
        })
    } //}}}
    pub fn exec(&self, config: SandboxConfig) -> Result<SandboxStatus, Box<dyn Error>> {
        //{{{
        use cgroups_fs::CgroupsCommandExt;
        let time_limit = ((config.time_limit + 500) / 1000 + 1).to_string();
        let mut status = SandboxStatusKind::Success;

        log::debug!("Memory Limit {}", config.memory_limit * 2);
        self.cur_cgroup
            .set_value("memory.limit_in_bytes", config.memory_limit * 2)?;
        self.cur_cgroup
            .set_value("memory.memsw.limit_in_bytes", config.memory_limit * 2)?;
        // Run command
        log::info!(
            "Chroot {} to run '{}'",
            self.rootfs_directory,
            config.command
        );
        let time_start = time::Instant::now();
        let return_code = std::process::Command::new("timeout")
            .args(&[&time_limit, "bash", "-c", &config.command])
            .cgroups(&[&self.cur_cgroup])
            .current_dir(String::from(&self.work_directory))
            .chroot(String::from(&self.rootfs_directory))
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
        let max_memory = self
            .cur_cgroup
            .get_value::<u64>("memory.memsw.max_usage_in_bytes")?;
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
    } //}}}
    fn remove(&self) {
        log::info!("Remove Sandbox");
        // Umount rootfs
        let handle_err = |err| {
            log::error!("Failed to umount sandbox: {}", err);
        };

        nix::mount::umount2(
            OsStr::new(&self.work_directory),
            nix::mount::MntFlags::MNT_DETACH,
        )
        .unwrap_or_else(handle_err);
        nix::mount::umount2(
            OsStr::new(&self.rootfs_directory),
            nix::mount::MntFlags::MNT_DETACH,
        )
        .unwrap_or_else(handle_err);

        // Remove Directory
        let handle_err = |err| {
            log::error!("Failed to remove sandbox: {}", err);
        };
        std::fs::remove_dir(&self.work_directory).unwrap_or_else(handle_err);
        std::fs::remove_dir(&self.rootfs_directory).unwrap_or_else(handle_err);
        log::info!("Done!");
    }
} //}}}

impl Drop for Sandbox {
    fn drop(&mut self) {
        self.remove();
    }
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
