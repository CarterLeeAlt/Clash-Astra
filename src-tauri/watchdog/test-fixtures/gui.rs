use std::os::windows::process::CommandExt;
use std::{fs, path::PathBuf, process::Command, thread, time::Duration};
#[path = "../../src/infrastructure/external_process.rs"]
mod external_process;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = &args[1];
    let dir = PathBuf::from(&args[2]);
    if mode == "external" || mode == "owned-child" {
        let pid_file = if mode == "external" {
            "external.pid"
        } else {
            "owned.pid"
        };
        fs::write(dir.join(pid_file), std::process::id().to_string()).unwrap();
        loop {
            thread::sleep(Duration::from_millis(50));
        }
    }
    let launches = dir.join("launches.txt");
    let previous = fs::read_to_string(&launches).unwrap_or_default();
    let count = previous.lines().count() + 1;
    fs::write(&launches, format!("{previous}{}\n", std::process::id())).unwrap();
    fs::write(dir.join("args.txt"), args.join("\n")).unwrap();
    match mode.as_str() {
        "retry-once" if count == 1 => std::process::exit(7),
        "restart" if count == 1 => std::process::exit(42),
        "fail" => std::process::exit(7),
        "hold" | "external-survives" => {
            if mode == "external-survives" {
                let mut command = Command::new(std::env::current_exe().unwrap());
                command.arg("external").arg(&dir);
                external_process::configure_external(&mut command)
                    .spawn()
                    .unwrap();
                Command::new(std::env::current_exe().unwrap())
                    .arg("owned-child")
                    .arg(&dir)
                    .creation_flags(0x08000000)
                    .spawn()
                    .unwrap();
            }
            while !dir.join("exit").exists() {
                thread::sleep(Duration::from_millis(50));
            }
        }
        _ => {}
    }
}
