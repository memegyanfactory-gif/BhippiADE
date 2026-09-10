//! Nothing Bhippi starts is allowed to outlive Bhippi.
//!
//! The studio spawns real OS processes — the Godot editor, the game, headless exports, PTY
//! shells, the Computer Use watchers — and each of them used to be stopped by *asking*: a
//! stop signal on a watch channel that an async task had to observe before it could kill
//! anything. That works while the app is alive and fails in exactly the three cases the
//! person sees:
//!
//! * **Closing the window.** Tauri's event loop ends the process as soon as the `Exit`
//!   handler returns, so the tasks holding the children are never polled again: the signal
//!   is sent to nobody and Godot keeps running.
//! * **Stopping the app from a debugger** (or Task Manager, or a crash). No Rust code runs
//!   at all, so nothing is asked to stop.
//! * **Grandchildren.** Play from inside the Godot *editor* launches the game as a child of
//!   the editor. Killing the pid Bhippi spawned never touched it.
//!
//! None of the three can be fixed by more careful bookkeeping, because each of them removes
//! the thing doing the bookkeeping. What fixes them is handing the guarantee to the kernel.
//!
//! On Windows that is a **job object**. Bhippi's own process joins one limited with
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; every process it spawns joins automatically, and so
//! does every process *those* spawn. The dying app's handle is the last handle to the job
//! however it dies — clean exit, kill, crash — and when it closes the kernel terminates the
//! whole job. No code of ours has to run.
//!
//! A few launches must escape it: "Open in VS Code", "Open a terminal here", "Open in
//! Explorer" hand a path to a program that belongs to the person, not to Bhippi. Those spawn
//! with [`DETACH_FROM_APP`] in their creation flags, which the job permits because it is
//! created with `JOB_OBJECT_LIMIT_BREAKAWAY_OK`.
//!
//! There is deliberately **no** "terminate everything now" call. `TerminateJobObject` has no
//! way to spare the caller, and the caller is in the job: it would kill Bhippi mid-shutdown
//! and report exit code 1 to whatever launched it. It would also be redundant —
//! `KILL_ON_JOB_CLOSE` fires as the dying app's handle closes, with no window in between for
//! a child to slip through. What the exit path does instead is stop the children it *has*
//! handles for, in order, synchronously, so the game window goes at the same instant as the
//! app rather than a few milliseconds later.
//!
//! Off Windows there is no equivalent that survives a `SIGKILL`. [`install`] says so in the
//! log and the explicit kills — [`kill_pid`] on the way out, `kill_on_drop` everywhere else —
//! stay the whole story there.
//!
//! This is INV-052's "leaves no orphan … running Godot child process", moved from a promise
//! the app keeps to one the OS keeps.

/// The creation flag a launch uses to say *this program is the person's, not Bhippi's*.
///
/// Zero off Windows, so a `creation_flags` call site needs no `cfg` of its own.
#[cfg(windows)]
pub const DETACH_FROM_APP: u32 = windows_sys::Win32::System::Threading::CREATE_BREAKAWAY_FROM_JOB;

/// The creation flag a launch uses to say *this program is the person's, not Bhippi's*.
#[cfg(not(windows))]
pub const DETACH_FROM_APP: u32 = 0;

/// Put this process, and everything it goes on to spawn, under an OS-level kill-together
/// guarantee. Called once, before any child can exist.
///
/// Failure is not fatal: the app runs exactly as it did before, and the log says the
/// guarantee is missing.
pub fn install() {
    imp::install();
}

/// Pull an already-running child into the guarantee.
///
/// Redundant when [`install`] succeeded — a child inherits the job — and the whole point
/// when it did not. Cheap, and silent about a process that is already in the job or already
/// gone.
pub fn adopt(pid: u32) {
    imp::adopt(pid);
}

/// Kill one process now, on the calling thread.
///
/// The exit handler's kill: by the time it returns the child is gone, rather than having
/// been *asked* to go by a task that will never be polled again.
pub fn kill_pid(pid: u32) {
    imp::kill_pid(pid);
}

/// Start a program that is the **person's**, not Bhippi's: their editor, their terminal,
/// their file manager, their browser. It must still be there after Bhippi closes.
///
/// Sets [`DETACH_FROM_APP`] on top of `extra_flags` and spawns. If Windows refuses the
/// breakaway — which happens when something outside Bhippi (a debugger's run-and-stop, a CI
/// runner) has put this app in a job of its own that does not allow it — the launch is
/// retried without the flag rather than failing. The person gets their editor; it is bound
/// to Bhippi's lifetime instead of theirs, which is worse than intended and much better than
/// a button that does nothing.
///
/// # Errors
///
/// The spawn's own error, when the program cannot be started at all.
pub fn spawn_for_the_person(
    command: &mut std::process::Command,
    extra_flags: u32,
) -> std::io::Result<std::process::Child> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(extra_flags | DETACH_FROM_APP);
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(error) => {
                tracing::debug!(
                    %error,
                    "this app cannot let a launch break away; starting it bound instead"
                );
                command.creation_flags(extra_flags);
            }
        }
    }
    #[cfg(not(windows))]
    let _unused = extra_flags;
    command.spawn()
}

/// Whether [`install`] took the guarantee. False off Windows, and on a Windows that refused
/// the job.
#[must_use]
pub fn is_installed() -> bool {
    imp::is_installed()
}

/// Whether one running process will die with this app.
///
/// The question worth asking about a process nobody here spawned — the game the Godot
/// *editor* launches on Play is a grandchild, and it is exactly the process that used to
/// survive a closed studio. False off Windows, where there is nothing to ask.
#[must_use]
pub fn is_bound(pid: u32) -> bool {
    imp::is_bound(pid)
}

// ── Windows ──────────────────────────────────────────────────────────────────────────

// The second `unsafe` module in the product, under the same rules as the first
// (`godot_embed::win`, ADR-0045): handle in, status out, a SAFETY note on every block.
// Nothing here allocates, retains a pointer past its call, or outlives the process.
#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };

    /// The job, as a plain integer so it can live in a static.
    ///
    /// A `HANDLE` is a raw pointer and so not `Sync`; the value behind it is an entry in this
    /// process's handle table, which is exactly the kind of thing that *is* safe to share. It
    /// is opened once and never closed — closing the last handle is what kills the job, and
    /// the process dying is the only moment that should happen.
    static JOB: AtomicUsize = AtomicUsize::new(0);

    /// The exit code a terminated child reports. Non-zero: it did not finish, it was stopped.
    const KILLED: u32 = 1;

    fn job() -> Option<HANDLE> {
        match JOB.load(Ordering::Acquire) {
            0 => None,
            raw => Some(raw as HANDLE),
        }
    }

    pub(super) fn is_installed() -> bool {
        job().is_some()
    }

    pub(super) fn install() {
        if is_installed() {
            return;
        }

        // SAFETY: no security attributes and no name — an unnamed job private to this
        // process. Returns null on failure, which is all that is done with the result.
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            // SAFETY: reads this thread's last-error slot, set by the call above.
            let error = unsafe { GetLastError() };
            tracing::warn!(
                error,
                "no job object; children may outlive a hard kill of the app"
            );
            return;
        }

        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        // KILL_ON_JOB_CLOSE is the guarantee. BREAKAWAY_OK is what lets an "open this in
        // your editor" launch escape it; without it `CREATE_BREAKAWAY_FROM_JOB` fails and
        // the person's VS Code would die with Bhippi.
        limits.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;
        let size =
            u32::try_from(std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).unwrap_or(0);

        // SAFETY: `handle` is the job just created, the pointer is to a live local of exactly
        // the type the information class names, and `size` is that type's size.
        let set = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of!(limits).cast(),
                size,
            )
        };
        if set == 0 {
            // SAFETY: reads this thread's last-error slot, set by the call above.
            let error = unsafe { GetLastError() };
            tracing::warn!(error, "the job object refused its limits; not using it");
            // SAFETY: `handle` is live, owned here, and never used again.
            unsafe { CloseHandle(handle) };
            return;
        }

        // SAFETY: `GetCurrentProcess` returns a pseudo-handle valid for the life of the
        // process and never closed; `handle` is the job just configured.
        let assigned = unsafe { AssignProcessToJobObject(handle, GetCurrentProcess()) };
        if assigned == 0 {
            // Nested jobs are a Windows 8 feature, so this is close to unreachable — but a
            // refusal must not cost the app its start, and `adopt` still covers each child.
            // SAFETY: reads this thread's last-error slot, set by the call above.
            let error = unsafe { GetLastError() };
            tracing::warn!(
                error,
                "could not join the job object; falling back to adopting each child"
            );
        }

        JOB.store(handle as usize, Ordering::Release);
        tracing::debug!(
            joined = assigned != 0,
            "child processes are now bound to this app's lifetime"
        );
    }

    pub(super) fn adopt(pid: u32) {
        let Some(job) = job() else {
            return;
        };
        // SET_QUOTA is what `AssignProcessToJobObject` needs; TERMINATE is what the job will
        // need to be able to do to it later.
        // SAFETY: a pid and access rights in, a handle or null out.
        let process = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid) };
        if process.is_null() {
            return;
        }
        // SAFETY: both handles are live and owned here.
        let assigned = unsafe { AssignProcessToJobObject(job, process) };
        if assigned == 0 {
            // SAFETY: reads this thread's last-error slot, set by the call above.
            let error = unsafe { GetLastError() };
            // Already in the job — the normal case once this process joined it — reports
            // access denied. That is the guarantee already holding, not a failure.
            if error != ERROR_ACCESS_DENIED {
                tracing::debug!(pid, error, "could not bind a child to the app's lifetime");
            }
        }
        // SAFETY: `process` is live, owned here, and never used again. Closing it does not
        // take the process back out of the job.
        unsafe { CloseHandle(process) };
    }

    pub(super) fn is_bound(pid: u32) -> bool {
        let Some(job) = job() else {
            return false;
        };
        // SAFETY: a pid and access rights in, a handle or null out.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return false;
        }
        let mut member = 0;
        // SAFETY: both handles are live and owned here, and `member` is a live local of the
        // `BOOL` the call writes.
        let asked = unsafe { IsProcessInJob(process, job, std::ptr::addr_of_mut!(member)) };
        // SAFETY: `process` is live, owned here, and never used again.
        unsafe { CloseHandle(process) };
        asked != 0 && member != 0
    }

    pub(super) fn kill_pid(pid: u32) {
        // SAFETY: a pid and access rights in, a handle or null out.
        let process = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
        if process.is_null() {
            return;
        }
        // SAFETY: `process` is live and was opened for exactly this.
        unsafe { TerminateProcess(process, KILLED) };
        // SAFETY: `process` is live, owned here, and never used again.
        unsafe { CloseHandle(process) };
    }
}

// ── everywhere else ──────────────────────────────────────────────────────────────────

#[cfg(not(windows))]
mod imp {
    pub(super) fn is_installed() -> bool {
        false
    }

    pub(super) fn install() {
        tracing::debug!(
            "no kill-together guarantee on this platform; children are stopped explicitly"
        );
    }

    pub(super) fn adopt(_pid: u32) {}

    pub(super) fn is_bound(_pid: u32) -> bool {
        false
    }

    /// `kill(2)` without a libc dependency. The exit path calls this a handful of times at
    /// most, and only on a platform the studio does not ship on yet.
    pub(super) fn kill_pid(pid: u32) {
        let _ignored = std::process::Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    /// A process that will not end on its own inside the life of a test.
    fn sleeper() -> Child {
        let mut command = if cfg!(windows) {
            let mut command = Command::new("cmd");
            command.args(["/c", "ping -n 120 127.0.0.1 > nul"]);
            command
        } else {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", "sleep 120"]);
            command
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the stand-in child starts")
    }

    /// The exit path's promise: by the time the call returns the process is stopping, with
    /// nothing left to poll and nobody left to ask. A two-minute sleeper that is reaped in
    /// under five seconds was killed, not waited out.
    #[test]
    fn kill_pid_stops_a_process_that_would_have_run_on() {
        install();
        let mut child = sleeper();
        let pid = child.id();

        kill_pid(pid);

        let started = Instant::now();
        let status = child.wait().expect("the killed child is reaped");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "kill_pid must not leave the caller waiting on the child's own schedule"
        );
        assert!(
            !status.success(),
            "a killed process did not finish its work: {status:?}"
        );
    }

    /// The headline case: Play from inside the Godot *editor* launches the game as a child
    /// of the editor, so Bhippi never had its pid and killing what it did spawn never
    /// touched it. Under the job, a grandchild is bound to the app exactly like a child.
    #[cfg(windows)]
    #[test]
    fn a_grandchild_is_bound_to_the_app_just_like_a_child() {
        use std::io::{BufRead, BufReader};

        install();
        assert!(is_installed(), "Windows must take the guarantee");

        // The child prints the pid of the process *it* starts, then stays alive so both are
        // still there to be asked about.
        let mut child = Command::new("powershell")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$p = Start-Process -FilePath 'cmd.exe' \
                 -ArgumentList '/c','ping -n 120 127.0.0.1 > nul' \
                 -WindowStyle Hidden -PassThru; \
                 Write-Output $p.Id; Start-Sleep -Seconds 120",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the stand-in editor starts");

        let child_pid = child.id();
        let mut line = String::new();
        let read = child
            .stdout
            .take()
            .map(|pipe| BufReader::new(pipe).read_line(&mut line));
        assert!(
            matches!(read, Some(Ok(n)) if n > 0),
            "the stand-in editor must report the pid it launched"
        );
        let grandchild_pid: u32 = line
            .trim()
            .parse()
            .unwrap_or_else(|error| panic!("it reports a pid, not {line:?}: {error}"));

        assert!(is_bound(child_pid), "the child Bhippi spawned is bound");
        assert!(
            is_bound(grandchild_pid),
            "the game the editor launched is bound too — this is the process that used to \
             keep running after the studio closed"
        );

        kill_pid(grandchild_pid);
        kill_pid(child_pid);
        let _ignored = child.wait();
    }

    /// The flag is the real `CREATE_BREAKAWAY_FROM_JOB` on Windows and inert elsewhere, so a
    /// call site can pass it unconditionally.
    #[test]
    fn detach_flag_is_the_platform_one() {
        if cfg!(windows) {
            assert_eq!(DETACH_FROM_APP, 0x0100_0000);
        } else {
            assert_eq!(DETACH_FROM_APP, 0);
        }
    }

    /// Installing is idempotent, and on Windows it takes the guarantee. Every other entry
    /// point has to be safe before it, after it, and without it.
    #[test]
    fn install_is_idempotent_and_the_rest_tolerate_anything() {
        install();
        install();
        assert_eq!(is_installed(), cfg!(windows));

        // Pid 0 is the system idle process on Windows and the caller's own group on Unix:
        // neither can be opened for termination, so both calls must be quiet no-ops.
        adopt(0);
        kill_pid(0);
        // A pid that cannot plausibly exist is the same story from the other side.
        adopt(u32::MAX);
        kill_pid(u32::MAX);
    }
}
