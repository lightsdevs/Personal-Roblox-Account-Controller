use std::{
    io,
    mem::{size_of, size_of_val},
    os::windows::{
        ffi::OsStringExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{ERROR_MORE_DATA, HANDLE},
    System::{JobObjects::*, Threading::*},
    UI::Shell::{AssocQueryStringW, ASSOCF_IS_PROTOCOL, ASSOCSTR_EXECUTABLE},
};

const JOB_NAME: &str = "Local\\PersonalRobloxAccountController.Background";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn player() -> Result<PathBuf, String> {
    let mut path = vec![0u16; 32768];
    let mut length = path.len() as u32;
    let result = unsafe {
        AssocQueryStringW(
            ASSOCF_IS_PROTOCOL,
            ASSOCSTR_EXECUTABLE,
            wide("roblox-player").as_ptr(),
            wide("open").as_ptr(),
            path.as_mut_ptr(),
            &mut length,
        )
    };
    if result != 0 || length < 2 {
        return Err(
            "Could not locate the registered Roblox Player for a background launch.".into(),
        );
    }
    let path = PathBuf::from(std::ffi::OsString::from_wide(&path[..length as usize - 1]));
    if !path.is_absolute()
        || !path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("RobloxPlayerBeta.exe"))
    {
        return Err(
            "Background mode requires the standard Roblox Player as the roblox-player handler."
                .into(),
        );
    }
    Ok(path)
}

fn cpu_job(name: &str) -> io::Result<OwnedHandle> {
    let handle = unsafe { CreateJobObjectW(null(), wide(name).as_ptr()) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let job = unsafe { OwnedHandle::from_raw_handle(handle) };
    Ok(job)
}

fn set_limit(job: &OwnedHandle, percent: u32) -> io::Result<()> {
    let limit = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
        ControlFlags: JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
        Anonymous: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0 {
            CpuRate: percent * 100,
        },
    };
    if unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectCpuRateControlInformation,
            (&raw const limit).cast(),
            size_of_val(&limit) as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// Attach the job during process creation. This also covers child processes,
// without enumerating clients or racing another account's launch.
fn start_in_job(job: &OwnedHandle, program: &Path, arguments: &str) -> io::Result<OwnedHandle> {
    use std::os::windows::ffi::OsStrExt;
    let path: Vec<u16> = program.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut command = vec![b'"' as u16];
    command.extend_from_slice(&path[..path.len() - 1]);
    command.extend(wide(&format!("\" {arguments}")));
    let mut bytes = 0;
    unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes) };
    if bytes == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut storage = vec![0usize; bytes.div_ceil(size_of::<usize>())];
    let list = storage.as_mut_ptr().cast();
    if unsafe { InitializeProcThreadAttributeList(list, 1, 0, &mut bytes) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let handle = job.as_raw_handle();
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of_val(&startup) as u32;
    startup.lpAttributeList = list;
    let mut process = PROCESS_INFORMATION::default();
    let created = unsafe {
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
            (&raw const handle).cast(),
            size_of::<HANDLE>(),
            null_mut(),
            null(),
        ) != 0
            && CreateProcessW(
                path.as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW,
                null(),
                null(),
                &startup.StartupInfo,
                &mut process,
            ) != 0
    };
    let error = (!created).then(io::Error::last_os_error);
    unsafe { DeleteProcThreadAttributeList(list) };
    if let Some(error) = error {
        return Err(error);
    }
    unsafe {
        drop(OwnedHandle::from_raw_handle(process.hThread));
        Ok(OwnedHandle::from_raw_handle(process.hProcess))
    }
}

pub fn configure(percent: u32) -> Result<&'static OwnedHandle, String> {
    static JOB: std::sync::OnceLock<OwnedHandle> = std::sync::OnceLock::new();
    if JOB.get().is_none() {
        let job = cpu_job(JOB_NAME)
            .map_err(|e| format!("Could not configure the background CPU limit ({e})."))?;
        let _ = JOB.set(job);
    }
    let job = JOB.get().expect("job initialized");
    set_limit(job, percent)
        .map_err(|e| format!("Could not set the background CPU limit ({e})."))?;
    Ok(job)
}

pub fn launch(uri: &str, percent: u32) -> Result<(), String> {
    let path = player()?;
    start_in_job(configure(percent)?, &path, &format!("\"{uri}\""))
        .map(drop)
        .map_err(|e| format!("Windows could not start Roblox with the background CPU limit ({e})."))
}

fn job_process_ids(job: &OwnedHandle) -> io::Result<Vec<u32>> {
    // The Windows structure ends in a variable-length array of pointer-sized IDs.
    let mut storage = vec![0usize; 32];
    loop {
        let bytes = u32::try_from(size_of_val(storage.as_slice()))
            .map_err(|_| io::Error::other("Background process list is too large."))?;
        let ok = unsafe {
            QueryInformationJobObject(
                job.as_raw_handle(),
                JobObjectBasicProcessIdList,
                storage.as_mut_ptr().cast(),
                bytes,
                null_mut(),
            )
        };
        if ok == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_MORE_DATA as i32) {
                storage.resize(storage.len() * 2, 0);
                continue;
            }
            return Err(error);
        }
        let header = unsafe { &*storage.as_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() };
        if header.NumberOfAssignedProcesses > header.NumberOfProcessIdsInList {
            storage.resize(storage.len() * 2, 0);
            continue;
        }
        let start = std::mem::offset_of!(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList)
            / size_of::<usize>();
        return Ok(
            storage[start..start + header.NumberOfProcessIdsInList as usize]
                .iter()
                .map(|&pid| pid as u32)
                .collect(),
        );
    }
}

fn trim_member(job: &OwnedHandle, pid: u32) -> bool {
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA,
            0,
            pid,
        )
    };
    if handle.is_null() {
        return false;
    }
    let process = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut member = 0;
    // A listed process may have exited and its PID may now belong to a main.
    unsafe {
        IsProcessInJob(process.as_raw_handle(), job.as_raw_handle(), &mut member) != 0
            && member != 0
            && SetProcessWorkingSetSize(process.as_raw_handle(), usize::MAX, usize::MAX) != 0
    }
}

pub fn trim_ram() -> Result<String, String> {
    let job =
        cpu_job(JOB_NAME).map_err(|e| format!("Could not access background clients ({e})."))?;
    let ids =
        job_process_ids(&job).map_err(|e| format!("Could not list background clients ({e})."))?;
    if ids.is_empty() {
        return Ok("No PRAC background clients are running.".into());
    }
    let trimmed = ids.iter().filter(|&&pid| trim_member(&job, pid)).count();
    Ok(format!(
        "Windows trimmed {trimmed} of {} background processes.",
        ids.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_launches_child_in_capped_job() {
        let job = cpu_job(&format!("Local\\PRAC.Test.{}", std::process::id())).unwrap();
        set_limit(&job, 15).unwrap();
        set_limit(&job, 33).unwrap();
        let child = start_in_job(
            &job,
            &std::env::current_exe().unwrap(),
            "--ignored --exact background::tests::check_child_cpu_rate",
        )
        .unwrap();
        let mut member = 0;
        assert_ne!(
            unsafe { IsProcessInJob(child.as_raw_handle(), job.as_raw_handle(), &mut member) },
            0
        );
        assert_ne!(member, 0);
        assert_eq!(
            unsafe { WaitForSingleObject(child.as_raw_handle(), 10000) },
            0
        );
        let mut code = 0;
        assert_ne!(
            unsafe { GetExitCodeProcess(child.as_raw_handle(), &mut code) },
            0
        );
        assert_eq!(code, 0);
    }
    #[test]
    #[ignore = "invoked inside the CPU-limited child"]
    fn check_child_cpu_rate() {
        let mut info = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION::default();
        assert_ne!(
            unsafe {
                QueryInformationJobObject(
                    null_mut(),
                    JobObjectCpuRateControlInformation,
                    (&raw mut info).cast(),
                    size_of_val(&info) as u32,
                    null_mut(),
                )
            },
            0
        );
        assert_ne!(info.ControlFlags & JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP, 0);
        assert_eq!(unsafe { info.Anonymous.CpuRate }, 3300);
        // This helper exits after the test, so it can safely join its own nested job.
        let job = cpu_job(&format!("Local\\PRAC.TrimTest.{}", std::process::id())).unwrap();
        assert!(job_process_ids(&job).unwrap().is_empty());
        assert!(!trim_member(&job, std::process::id()));
        assert_ne!(
            unsafe { AssignProcessToJobObject(job.as_raw_handle(), GetCurrentProcess()) },
            0
        );
        assert_eq!(job_process_ids(&job).unwrap(), vec![std::process::id()]);
        assert!(trim_member(&job, std::process::id()));
    }
}
