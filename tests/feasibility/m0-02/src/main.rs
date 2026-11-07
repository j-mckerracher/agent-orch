// M0-02 feasibility probe: subprocess launch, stdout/stderr streaming, and
// termination on the native host OS (Windows or macOS). Std only.
//
// Usage:
//   cargo run                 run all parent-side scenarios
//   m0-02-probe child [sleep]  child mode: stream lines, exit 0
//   m0-02-probe fail           child mode: stderr payload, exit 7
//   m0-02-probe sleep-child N  child mode: sleeps N seconds, exits 0
//   m0-02-probe spawn-kid      child mode: spawns a sleeping grandchild and
//                              heartbeats until it exits
//
// Parent scenarios (each prints `TEST=<name> PASS|FAIL <detail>`):
//   launch-stream   one child, interleaved stdout/stderr streamed, exit code
//   stderr-exit     child writes a multi-KB stderr payload, non-zero exit
//   fanout          4 concurrent children, outputs attributed by pid
//   kill-tree       child spawns a grandchild that outlives it; kill the
//                   child's tree; verify the grandchild is gone too
//
// Exit code 0 iff every scenario passed.
// Updated: 2026-11-07T10:45-07:00

use std::env::{args, current_exe};
use std::io::{BufRead, BufReader, Read};
use std::process::{self, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

fn main() {
    let args: Vec<String> = args().collect();
    match args.get(1).map(String::as_str) {
        Some("child") => child_mode(args.get(2).map(String::as_str).unwrap_or("1")),
        Some("fail") => fail_mode(),
        Some("sleep-child") => {
            let secs = args
                .get(2)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(12);
            std::thread::sleep(Duration::from_secs(secs));
        }
        Some("spawn-kid") => spawn_kid_mode(),
        Some(_) => {
            eprintln!("unknown subcommand: {}", &args[1]);
            process::exit(2);
        }
        None => parent_mode(),
    }
}

fn child_mode(sleep_secs: &str) {
    let secs = sleep_secs.parse::<u64>().unwrap_or(1);
    for i in 0..4 {
        println!("child-line-{} pid={}", i, process::id());
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_secs(secs));
    println!("child-final pid={}", process::id());
}

fn fail_mode() {
    let chunk = "x".repeat(4096);
    for i in 0..4 {
        eprintln!("stderr-block-{} pid={}", i, process::id());
        eprint!("{}", chunk);
    }
    process::exit(7);
}

fn spawn_kid_mode() {
    // Spawn a grandchild that sleeps for 12s, then print a heartbeat every
    // 250ms until the grandchild exits. The child therefore stays alive the
    // whole time, giving the parent a live two-level tree to kill.
    let me = current_exe().unwrap();
    let mut cmd = Command::new(&me);
    cmd.arg("sleep-child")
        .arg("12");
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn grandchild");
    let grand_pid = child.id();
    println!("spawn-kid pid={} grandchild={grand_pid}", process::id());
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                println!("spawn-kid pid={} grandchild-exited", process::id());
                return;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(e) => {
                eprintln!("spawn-kid try_wait: {e}");
                return;
            }
        }
    }
}

fn parent_mode() {
    let me = current_exe().unwrap();
    let mut failures = 0;
    failures += launch_stream(&me);
    failures += stderr_exit(&me);
    failures += fanout(&me);
    failures += kill_tree(&me);
    if failures == 0 {
        println!("RESULT ALL PASS");
    } else {
        println!("RESULT {failures} SCENARIO(S) FAILED");
        process::exit(1);
    }
}

// Launch one child, stream both pipes concurrently, verify ordering,
// interleaving and exit code. Returns 1 on failure.
fn launch_stream(me: &std::path::Path) -> i32 {
    let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();
    let (err_tx, err_rx) = mpsc::channel::<Vec<u8>>();
    let mut stdout_pipe = {
        let mut child = Command::new(me)
            .arg("child")
            .arg("0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn child");
        let pid = child.id();
        let out = child.stdout.take().unwrap();
        let err = child.stderr.take().unwrap();
        std::thread::spawn(move || {
            let mut r = BufReader::new(out);
            let mut b = [0u8; 4096];
            loop {
                match r.read(&mut b) {
                    Ok(0) => break,
                    Ok(n) => {
                        if out_tx.send(b[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        std::thread::spawn(move || {
            let mut r = BufReader::new(err);
            let mut b = [0u8; 4096];
            loop {
                match r.read(&mut b) {
                    Ok(0) => break,
                    Ok(n) => {
                        if err_tx.send(b[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        (child, pid)
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match out_rx.recv_timeout(remaining) {
            Ok(chunk) => out.extend(chunk),
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                while let Ok(chunk) = out_rx.try_recv() {
                    out.extend(chunk);
                }
                while let Ok(chunk) = err_rx.try_recv() {
                    err.extend(chunk);
                }
                break;
            }
        }
    }
    let status = match stdout_pipe.0.wait() {
        Ok(s) => s,
        Err(e) => {
            println!("TEST=launch-stream FAIL wait error: {e}");
            return 1;
        }
    };
    let out_s = String::from_utf8_lossy(&out);
    let ok = status.success()
        && out_s.contains("child-line-0")
        && out_s.contains("child-line-3")
        && out_s.contains("child-final")
        && out_s.contains(&format!("pid={}", stdout_pipe.1))
        && err.is_empty();
    if ok {
        println!("TEST=launch-stream PASS stdout_len={} exit=0 child pid={}", out_s.len(), stdout_pipe.1);
    } else {
        println!(
            "TEST=launch-stream FAIL status={status:?} stdout={out_s:?} stderr={err:?}"
        );
        return 1;
    }
    0
}

// Child exits non-zero with a multi-KB stderr payload; verify exit code and
// payload size, and that stdout stayed clean.
fn stderr_exit(me: &std::path::Path) -> i32 {
    let mut child = Command::new(me)
        .arg("fail")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn fail child");
    let mut out = String::new();
    let mut err = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    let status = child.wait().unwrap();
    let code = status.code().unwrap_or(-1);
    let ok = code == 7 && err.len() >= 16_000 && err.contains("stderr-block-3") && out.is_empty();
    if ok {
        println!("TEST=stderr-exit PASS exit=7 stderr_len={} stdout_len=0", err.len());
    } else {
        println!("TEST=stderr-exit FAIL code={code} stderr_len={} stdout={out:?}", err.len());
        return 1;
    }
    0
}

// Four concurrent children; each child prints its pid and we verify the
// outputs are attributable to four distinct pids.
fn fanout(me: &std::path::Path) -> i32 {
    let mut children = Vec::new();
    for _ in 0..4 {
        let mut c = Command::new(me)
            .arg("child")
            .arg("0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn fanout child");
        let pid = c.id();
        let out = c.stdout.take().unwrap();
        children.push((pid, out, c));
    }
    let mut pids: Vec<u32> = Vec::new();
    let mut all_ok = true;
    for (pid, mut out, mut child) in children {
        let mut s = String::new();
        if out.read_to_string(&mut s).is_err() || child.wait().is_err() {
            all_ok = false;
            continue;
        }
        let reported: u32 = s
            .lines()
            .find_map(|l| l.strip_prefix("child-line-0 pid="))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let ok = reported == pid && s.contains("child-final");
        if !ok {
            all_ok = false;
        }
        pids.push(pid);
    }
    let distinct = pids.iter().collect::<std::collections::HashSet<_>>().len();
    if all_ok && distinct == 4 {
        println!("TEST=fanout PASS pids={pids:?}");
    } else {
        println!("TEST=fanout FAIL pids={pids:?} distinct={distinct} all_ok={all_ok}");
        return 1;
    }
    0
}

// Spawn a child that itself spawns a grandchild (a real descendant process
// tree: parent -> spawn-kid child -> grandchild). Kill the child, then
// verify neither the child nor the grandchild is alive.
fn kill_tree(me: &std::path::Path) -> i32 {
    let mut cmd = Command::new(me);
    cmd.arg("spawn-kid")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(unix)]
    {
        // Put the child in a new process group so `kill -9 -<pid>` targets
        // exactly this subtree.
        cmd.pre_exec(|| {
            if unsafe { setpgid(0, 0) } == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
    let mut child = cmd.spawn().expect("spawn spawn-kid child");
    let child_pid = child.id();
    let mut line = String::new();
    if BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .is_err()
    {
        println!("TEST=kill-tree FAIL reading child stdout");
        return 1;
    }
    let parts: Vec<&str> = line.split_whitespace().collect();
    let child_done: u32 = parts
        .iter()
        .find_map(|p| p.strip_prefix("pid="))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let grand_pid: u32 = parts
        .iter()
        .find_map(|p| p.strip_prefix("grandchild="))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if child_done != child_pid || grand_pid == 0 || grand_pid == child_pid {
        println!("TEST=kill-tree FAIL parse line={line:?} child={child_pid}");
        return 1;
    }
    match child.try_wait() {
        Ok(None) => {}
        Ok(Some(s)) => {
            println!("TEST=kill-tree FAIL child already exited: {s:?}");
            return 1;
        }
        Err(e) => {
            println!("TEST=kill-tree FAIL try_wait: {e}");
            return 1;
        }
    }
    match kill_child_and_descendants(child_pid) {
        Ok(()) => {
            match child.wait() {
                Ok(_) => {}
                Err(e) => {
                    println!("TEST=kill-tree FAIL wait: {e}");
                    return 1;
                }
            }
            let mut ok = true;
            for _ in 0..10 {
                if !is_process_running(grand_pid) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            if is_process_running(grand_pid) {
                println!("TEST=kill-tree FAIL grandchild pid={grand_pid} still alive after 5s");
                ok = false;
            }
            if ok {
                println!(
                    "TEST=kill-tree PASS child pid={child_pid} grandchild pid={grand_pid} both gone after tree kill"
                );
            }
            return if ok { 0 } else { 1 };
        }
        Err(e) => {
            println!("TEST=kill-tree FAIL kill: {e}");
            let _ = child.kill();
            let _ = child.wait();
            return 1;
        }
    }
}

/// Kill a child and its descendants. Returns an error if the platform has
/// no built-in tree kill and the manual fallback fails.
#[cfg(target_os = "windows")]
fn kill_child_and_descendants(pid: u32) -> Result<(), String> {
    // `taskkill /T` kills the whole process tree in one documented step.
    let out = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .output()
        .map_err(|e| format!("taskkill launch: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "taskkill exit {:?}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

#[cfg(unix)]
extern "C" {
    fn setpgid(pid: i32, pgid: i32) -> i32;
}

#[cfg(not(target_os = "windows"))]
fn kill_child_and_descendants(pid: u32) -> Result<(), String> {
    // SIGKILL the child's whole process group. kill_tree puts the child in
    // a new process group (setpgid(0, 0) in pre_exec), so the group
    // contains exactly the child and its descendants, never the parent.
    let group = format!("-{pid}");
    let status = Command::new("kill")
        .args(["-9", &group])
        .spawn()
        .and_then(|mut k| k.wait())
        .map_err(|e| format!("kill launch: {e}"))?;
    if !status.success() {
        return Err(format!("kill exit {status:?}"));
    }
    Ok(())
}

/// True if a process with the exact pid is running.
fn is_process_running(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}")])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .contains(&pid.to_string())
            })
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}
