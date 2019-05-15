#![deny(warnings)]
use criterion::{black_box, criterion_group, criterion_main, Bencher, Criterion, Fun};
#[allow(unused_imports)] //Depending on the build config this might be unused
use failure::bail;
use failure::Fallible;
#[allow(unused_imports)] //Depending on the build config this might be unused
use std::process::Child;
use tracers_macros::{init_provider, probe, tracer};

static INT_ARG: usize = 324;
static STRING_ARG: &str =
    "fear is the mind killer I will face my fear I will let it pass through me";

#[tracer]
trait ProbeBenchmarks {
    fn no_args();
    fn int_arg1(arg0: usize);
    fn int_arg3(arg0: usize, arg1: usize, arg2: usize);
    fn int_arg6(arg0: usize, arg1: usize, arg2: usize, arg3: usize, arg4: usize, arg5: usize);
    fn int_arg12(
        arg0: usize,
        arg1: usize,
        arg2: usize,
        arg3: usize,
        arg4: usize,
        arg5: usize,
        arg6: usize,
        arg7: usize,
        arg8: usize,
        arg9: usize,
        arg10: usize,
        arg11: usize,
    );
    fn string_arg1(arg0: &str);
    fn string_arg3(arg0: &str, arg1: &str, arg2: &str);
    fn string_arg6(arg0: &str, arg1: &str, arg2: &str, arg3: &str, arg4: &str, arg5: &str);
    fn string_arg12(
        arg0: &str,
        arg1: &str,
        arg2: &str,
        arg3: &str,
        arg4: &str,
        arg5: &str,
        arg6: &str,
        arg7: &str,
        arg8: &str,
        arg9: &str,
        arg10: &str,
        arg11: &str,
    );
}

fn bench_no_args(b: &mut Bencher, _arg: &()) {
    b.iter(|| probe!(ProbeBenchmarks::no_args()))
}

fn bench_int_arg1(b: &mut Bencher, _arg: &()) {
    b.iter(|| probe!(ProbeBenchmarks::int_arg1(black_box(INT_ARG))))
}

fn bench_int_arg3(b: &mut Bencher, _arg: &()) {
    b.iter(|| {
        probe!(ProbeBenchmarks::int_arg3(
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG)
        ))
    })
}

fn bench_int_arg6(b: &mut Bencher, _arg: &()) {
    b.iter(|| {
        probe!(ProbeBenchmarks::int_arg6(
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG)
        ))
    })
}

fn bench_int_arg12(b: &mut Bencher, _arg: &()) {
    b.iter(|| {
        probe!(ProbeBenchmarks::int_arg12(
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG)
        ))
    })
}

fn bench_string_arg1(b: &mut Bencher, _arg: &()) {
    b.iter(|| probe!(ProbeBenchmarks::string_arg1(black_box(STRING_ARG))))
}

fn bench_string_arg3(b: &mut Bencher, _arg: &()) {
    b.iter(|| {
        probe!(ProbeBenchmarks::string_arg3(
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG)
        ))
    })
}

fn bench_string_arg6(b: &mut Bencher, _arg: &()) {
    b.iter(|| {
        probe!(ProbeBenchmarks::string_arg6(
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG)
        ))
    })
}

fn bench_string_arg12(b: &mut Bencher, _arg: &()) {
    b.iter(|| {
        probe!(ProbeBenchmarks::string_arg12(
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG)
        ))
    })
}

fn bench_fire_disabled(c: &mut Criterion) {
    bench_fire(c, false);
}

fn bench_fire_enabled(c: &mut Criterion) {
    match enable_tracing() {
        Err(e) => eprintln!("Unable to run benchmarks with probes enabled: {}", e),
        Ok(trace) => {
            bench_fire(c, true);
            disable_tracing(trace);
        }
    }
}

/// Invokes an external command that will enable the tracing probes in this process.
#[cfg(all(target_os = "linux", any(static_stap_enabled, dynamic_stap_enabled)))]
fn enable_tracing() -> Fallible<Child> {
    use failure::{format_err, ResultExt};
    use std::env;
    use std::io::ErrorKind;
    use std::path::{Path, PathBuf};
    use std::process::{self, Command};

    fn is_root() -> bool {
        env::var("USER")
            .map(|user| user == "root")
            .unwrap_or_default()
    }

    fn find_executable<P>(exe_name: P) -> Option<PathBuf>
    where
        P: AsRef<Path>,
    {
        env::var_os("PATH").and_then(|paths| {
            env::split_paths(&paths)
                .filter_map(|dir| {
                    let full_path = dir.join(&exe_name);
                    if full_path.is_file() {
                        Some(full_path)
                    } else {
                        None
                    }
                })
                .next()
        })
    }

    //This should be done with `bpftrace`, however as of this writing (2019-05-10) this bug:
    //https://github.com/iovisor/bpftrace/issues/612 prevents USDT probes using semaphore from
    //functioning properly.  Thankfully the older `funccount` utility in the `bcc` toolkit does
    //work.
    //
    //Less fortunately, it is not compatible with Python3, so it needs to be invoked explicitly
    //with the python2 interpreter, which we blithely assume is installed on the system
    if !is_root() {
        bail!("Benchmarking with probes enabled requires running as root");
    }

    let provider_info = init_provider!(ProbeBenchmarks).expect("Provider init failed");

    if !provider_info.contains("static_stap") && !provider_info.contains("dynamic_stap") {
        bail!("Don't know how to enable {}", provider_info);
    }

    //Find where in the path the `funccount` utility is
    let funccount_path = find_executable("funccount").ok_or_else(|| {
        format_err!("Unable to locate `funccount` in the path; are the BCC tools installed?")
    })?;

    //Invoke `python2` with this funccount path and the args
    //
    //funccount's args will be:
    //  --pid $PID   - The PID of this bencher process
    //  'u:$EXE:*'   - Filter expression, `$EXE` is the fully qualified path to this executable,
    //                 `*` means all probes in the process
    let exe_path = env::current_exe().context("current_exe failed")?;

    let trace = match Command::new("python2")
        .arg(funccount_path)
        .args(&["--pid", &format!("{}", process::id())])
        .arg(&format!("u:{}:*", exe_path.display()))
        .spawn()
    {
        Err(ref e) if e.kind() == ErrorKind::NotFound => Err(format_err!(
            "The `python2` executable wasn't found; make sure it's installed and in the path"
        )),
        Err(e) => Err(e.into()),
        Ok(child) => Ok(child),
    }?;

    Ok(trace)
}

/// Disables tracing previously enabled by invoking `funccount`.  When this function returns the
/// `funccount` process will have been terminated
#[cfg(all(target_os = "linux", any(static_stap_enabled, dynamic_stap_enabled)))]
fn disable_tracing(mut child: Child) {
    use nix::sys::signal::{self, Signal};
    use nix::unistd::Pid;

    //Send SIGTERM to the child process, and collect it's stdoutput waiting for it to die
    //That way we can see the function counts it prints and visually confirm the probes fired as
    //expected
    let pid = Pid::from_raw(child.id() as i32);
    signal::kill(pid, Signal::SIGINT).expect("Failed to send SIGINT to child process");

    child
        .wait()
        .expect("waiting for child process to terminate");
}

/// Invokes an external command that will enable the tracing probes in this process.
#[cfg(all(target_os = "linux", static_lttng_enabled))]
fn enable_tracing() -> Fallible<()> {
    //The lttng command line tool doesn't require superuser privs so it's much easier than the
    //dtract or systemtap versions
    lttng(&["create"])?;
    lttng(&["enable-event", "--userspace", "tracers_probe_benchmarks:*"])?;
    lttng(&["start"])?;

    Ok(())
}

/// Invokes the lttng CLI with specified args, as the current user
#[cfg(all(target_os = "linux", static_lttng_enabled))]
fn lttng(args: &[&str]) -> Fallible<()> {
    use failure::format_err;
    use std::io::ErrorKind;
    use std::process::{Command, Stdio};

    print!("lttng ");
    for arg in args.iter() {
        print!(" {}", arg);
    }
    println!();

    match Command::new("lttng")
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
    {
        Err(ref e) if e.kind() == ErrorKind::NotFound => Err(format_err!(
            "The `lttng` executable was not found; is the lttng-tools package installed?"
        )),
        Err(e) => Err(e.into()),
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                Err(format_err!(
                    "The lttng command failed with exit code {}",
                    output.status
                ))
            }
        }
    }
}

/// Disables a previously enabled lttng tracing session
#[cfg(all(target_os = "linux", static_lttng_enabled))]
fn disable_tracing(_unused: ()) {
    lttng(&["destroy"]).expect("failed to destroy LTTng session");
}

#[cfg(all(
    target_os = "linux",
    not(any(static_lttng_enabled, static_stap_enabled, dynamic_stap_enabled))
))]
fn enable_tracing() -> Fallible<()> {
    bail!("No supported tracing implementations are enabled")
}

#[cfg(all(
    target_os = "linux",
    not(any(static_lttng_enabled, static_stap_enabled, dynamic_stap_enabled))
))]
fn disable_tracing(_unused: ()) {
    unreachable!()
}

#[cfg(not(target_os = "linux"))]
fn enable_tracing() -> Fallible<()> {
    bail!("Enabling probes on non-Linux targets is not yet supported")
}

#[cfg(not(target_os = "linux"))]
fn disable_tracing(_unused: ()) {
    unreachable!()
}

fn bench_fire(c: &mut Criterion, enabled: bool) {
    let funcs = vec![
        Fun::new("no args", bench_no_args),
        Fun::new("int_arg1", bench_int_arg1),
        Fun::new("int_arg3", bench_int_arg3),
        Fun::new("int_arg6", bench_int_arg6),
        Fun::new("int_arg12", bench_int_arg12),
        Fun::new("string_arg1", bench_string_arg1),
        Fun::new("string_arg3", bench_string_arg3),
        Fun::new("string_arg6", bench_string_arg6),
        Fun::new("string_arg12", bench_string_arg12),
    ];

    let provider_info = init_provider!(ProbeBenchmarks).expect("Provider init failed");

    c.bench_functions(
        &format!(
            "{}abled '{}'",
            if enabled { "en" } else { "dis" },
            provider_info
        ),
        funcs,
        (),
    );
}

criterion_group!(benches, bench_fire_disabled, bench_fire_enabled);
criterion_main!(benches);
