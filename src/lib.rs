use std::error::Error;
use std::os::unix::process::CommandExt;
use std::process::Stdio;

pub struct SandboxConfig {
    pub command: String,
    pub time_limit: u64,
    pub memory_limit: u64,
    pub pids_limit: u16,
    pub stdin: Stdio,
    pub stdout: Stdio,
    pub stderr: Stdio,
}

impl SandboxConfig {
    pub fn new<T>(
        time_limit: u64,
        memory_limit: u64,
        pids_limit: u16,
        command: T,
        stdin: Stdio,
        stdout: Stdio,
        stderr: Stdio,
    ) -> SandboxConfig
    where
        T: std::fmt::Display,
    {
        SandboxConfig {
            time_limit: time_limit,
            memory_limit: memory_limit,
            pids_limit: pids_limit,
            command: command.to_string(),
            stdin,
            stdout,
            stderr,
        }
    }
}

struct SandboxCgroup {
    freezer: cgroups_fs::AutomanagedCgroup,
    memory: cgroups_fs::AutomanagedCgroup,
    pids: cgroups_fs::AutomanagedCgroup,
    cpuacct: cgroups_fs::AutomanagedCgroup,
}

// TODO: use impl to set/get cgroup value
impl SandboxCgroup {
    fn new(cgroup_name: &str) -> Result<SandboxCgroup, Box<dyn Error>> {
        use cgroups_fs::*;
        let cur_cgroup = CgroupName::new(cgroup_name);
        Ok(SandboxCgroup {
            memory: AutomanagedCgroup::init(&cur_cgroup, "memory")?,
            pids: AutomanagedCgroup::init(&cur_cgroup, "pids")?,
            freezer: AutomanagedCgroup::init(&cur_cgroup, "freezer")?,
            cpuacct: AutomanagedCgroup::init(&cur_cgroup, "cpuacct")?,
        })
    }
    pub fn is_empty(&self) -> Result<bool, Box<dyn Error>> {
        log::trace!("Current task list: {:?}", self.freezer.get_tasks()?);
        Ok(self.freezer.get_tasks()?.is_empty())
    }
    pub fn get_cpu_time(&self) -> Result<std::time::Duration, Box<dyn Error>> {
        Ok(std::time::Duration::from_nanos(
            self.cpuacct.get_value::<u64>("cpuacct.usage")?,
        ))
    }
    pub fn get_max_memory(&self) -> Result<u64, Box<dyn Error>> {
        Ok(self
            .memory
            .get_value::<u64>("memory.memsw.max_usage_in_bytes")?)
    }
    pub fn clear(&self) -> Result<(), Box<dyn Error>> {
        self.memory
            .set_value("memory.memsw.max_usage_in_bytes", 0)?;
        self.cpuacct.set_value("cpuacct.usage", 0)?;

        Ok(())
    }
    pub fn set_memory_limit(&self, memory_limit: u64) -> Result<(), Box<dyn Error>> {
        self.memory
            .set_value("memory.limit_in_bytes", memory_limit * 2)?;
        self.memory
            .set_value("memory.memsw.limit_in_bytes", memory_limit * 2)?;

        Ok(())
    }
    pub fn set_pids_limit(&self, pids_limit: u16) -> Result<(), Box<dyn Error>> {
        self.pids.set_value("pids.max", pids_limit)?;

        Ok(())
    }
    pub fn kill_all_tasks(&self, timeout: std::time::Duration) -> Result<(), Box<dyn Error>> {
        let freezer = &self.freezer;
        let delay = std::time::Duration::from_millis(100);
        let mut timeout = timeout;

        log::info!("Try kill all in cgroup {:?}", &freezer);
        log::trace!("Current task list {:?}", freezer.get_tasks()?);

        if self.is_empty()? == true {
            return Ok(());
        };

        freezer.set_value::<&str>("freezer.state", "FROZEN")?;

        while timeout > std::time::Duration::from_millis(0) {
            if freezer.get_value::<String>("freezer.state")? == "FROZEN" {
                break;
            }
            std::thread::sleep(delay);
            timeout -= delay;
        }

        freezer.send_signal_to_all_tasks(nix::sys::signal::Signal::SIGKILL)?;

        freezer.set_value::<&str>("freezer.state", "THAWED")?;
        while timeout > std::time::Duration::from_millis(0) {
            log::trace!("{:?}: checking...", timeout);
            if self.is_empty()? == true {
                return Ok(());
            }
            std::thread::sleep(delay);
            timeout -= delay;
        }

        Err("Failed to kill all tasks".to_string().into())
    }
}

#[derive(Debug)]
pub struct Sandbox {
    pub sandbox_directory: std::path::PathBuf,
    work_directory: std::path::PathBuf,
    rootfs_directory: std::path::PathBuf,
    mounted: bool,
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
    pub used_time: u128,
    /// 使用内存
    pub max_memory: u64,
    /// 程序返回值
    pub return_code: i32,
}

impl Sandbox {
    pub fn new<T, U, V>(
        rootfs_directory: T,
        work_directory: U,
        sandbox_directory: V,
    ) -> Result<Sandbox, Box<dyn Error>>
    where
        T: AsRef<std::path::Path>,
        U: AsRef<std::path::Path>,
        V: AsRef<std::path::Path>,
    {
        let rootfs_directory = std::path::PathBuf::from(rootfs_directory.as_ref());
        let work_directory = std::path::PathBuf::from(work_directory.as_ref());
        let sandbox_directory = std::path::PathBuf::from(sandbox_directory.as_ref());

        let log_and_panic = |err: &str| -> Result<(), Box<dyn Error>> {
            log::error!("{}", err);
            return Err(String::from(err).into());
        };

        let check_directory = |directory: &std::path::PathBuf| -> Result<(), Box<dyn Error>> {
            if directory.exists() == false {
                log_and_panic(&format!("{:?} Not Found!", directory))?;
            }
            Ok(())
        };

        // Check swapaccount
        if std::path::Path::new("/sys/fs/cgroup/memory/memory.memsw.usage_in_bytes").exists()
            == false
        {
            log_and_panic("Need \"cgroup_enable=memory swapaccount=1\" kernel parameter")?;
        }

        check_directory(&rootfs_directory)?;
        check_directory(&work_directory)?;
        check_directory(&sandbox_directory)?;

        // Mount Directory
        let lower_dirs = [&rootfs_directory];
        libmount::Overlay::writable(
            lower_dirs.iter().map(|x| x.as_ref()),
            &work_directory,
            &sandbox_directory,
            &sandbox_directory,
        )
        .mount()?;

        Ok(Sandbox {
            sandbox_directory,
            work_directory,
            rootfs_directory,
            mounted: true,
        })
    }
    pub fn run(&self, config: SandboxConfig) -> Result<SandboxStatus, Box<dyn Error>> {
        use cgroups_fs::CgroupsCommandExt;
        use std::time::Duration;
        use wait_timeout::ChildExt;

        // Init
        let cgroup = SandboxCgroup::new(&uuid::Uuid::new_v4().to_string()).unwrap();
        let time_limit = Duration::from_millis(config.time_limit + 500);
        let mut status = SandboxStatusKind::Success;
        let mut used_time = time_limit;

        // Set cgroup limit
        cgroup.clear()?;
        cgroup.set_memory_limit(config.memory_limit * 2)?;
        cgroup.set_pids_limit(config.pids_limit)?;

        let mut return_code = Some(0);
        match nix::unistd::fork() {
            Err(_) => log::error!("Fork error!"),
            Ok(nix::unistd::ForkResult::Child) => {
                log::trace!("forked!");
                nix::sched::unshare(nix::sched::CloneFlags::CLONE_NEWPID).unwrap();
                // Create Child
                let mut child_exec = std::process::Command::new("bash")
                    .args(&["-c", &config.command])
                    .current_dir(&self.sandbox_directory)
                    .cgroups(&[
                        &cgroup.memory,
                        &cgroup.pids,
                        &cgroup.freezer,
                        &cgroup.cpuacct,
                    ])
                    .chroot(self.sandbox_directory.to_str().unwrap().to_string())
                    .stdin(config.stdin)
                    .stdout(config.stdout)
                    .stderr(config.stderr)
                    .spawn()
                    .unwrap();

                let return_code = match child_exec.wait_timeout(time_limit * 2).unwrap() {
                    Some(status) => status.code(),
                    _ => {
                        log::debug!("Time Limit: {:?}, TLE", time_limit);
                        child_exec.kill().unwrap();
                        child_exec.wait().unwrap().code()
                    }
                };
                log::debug!("forked: {:?}", return_code);
                std::process::exit(return_code.unwrap_or_else(|| -1));
            }
            Ok(nix::unistd::ForkResult::Parent { child, .. }) => {
                use nix::sys::wait::WaitStatus::Exited;
                let mut timeout = time_limit;
                let delay = Duration::from_millis(100);
                let zero_time = Duration::from_millis(0);

                // Wait for child task start
                std::thread::sleep(delay);

                // Look up until timeout or no task in cgroup
                while timeout > zero_time {
                    if cgroup.is_empty()? == true {
                        break;
                    }
                    std::thread::sleep(delay);
                    timeout -= delay;
                    log::trace!("less time {:?}", timeout);
                }

                nix::sys::signal::kill(child, nix::sys::signal::Signal::SIGKILL).unwrap();
                return_code = match nix::sys::wait::waitpid(child, None)? {
                    Exited(_pid, status) => Some(status),
                    _ => None,
                };
                log::trace!("{:?}", return_code);

                if timeout == zero_time {
                    used_time = std::cmp::max(time_limit + delay, cgroup.get_cpu_time()?);
                } else {
                    used_time = cgroup.get_cpu_time()?;
                }
            }
        };

        cgroup
            .kill_all_tasks(std::time::Duration::from_millis(1000))
            .unwrap_or_else(|err| {
                log::warn!("{}", err);
            });

        // Get return code
        log::debug!("{:?}", return_code);
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
        let max_memory = cgroup.get_max_memory()?;
        if max_memory > config.memory_limit {
            status = SandboxStatusKind::MemoryLimitExceeded;
        }

        // Calc time
        if used_time > std::time::Duration::from_millis(config.time_limit) {
            status = SandboxStatusKind::TimeLimitExceeded;
        }
        let used_time = used_time.as_millis();

        log::debug!(
            "status: {:?}, used_time: {}, return_code: {}, max_memory: {}",
            status,
            used_time,
            return_code,
            max_memory
        );
        Ok(SandboxStatus {
            status,
            max_memory,
            used_time,
            return_code,
        })
    }
    fn remove(&mut self) {
        use std::ffi::OsStr;
        if self.mounted == false {
            log::warn!("Try to remove an umounted sandbox");
            return;
        }
        log::info!("Remove sandbox on {:?}", &self);
        nix::mount::umount(OsStr::new(&self.sandbox_directory))
            .unwrap_or_else(|err| log::error!("Failed to umount :{}", err));
        self.mounted = false;
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        log::trace!("DROP {:?}", self);
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
        use std::ffi::OsStr;
        log::debug!("Chroot to {}", dir);
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
        use std::ffi::OsStr;
        unsafe {
            self.pre_exec(move || {
                nix::unistd::chdir(OsStr::new(&dir)).unwrap();
                Ok(())
            })
        }
    }
}
