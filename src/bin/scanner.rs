#![cfg_attr(
    all(target_os = "windows", not(feature = "console")),
    windows_subsystem = "windows"
)]
use clap::Parser;
use log::{debug, error};
use std::time::Instant;
use sysinfo::{System, SystemExt};

const DELAY_MILLIS: u128 = 50;

#[derive(Debug, Parser)]
struct Args {
    #[arg(short, long, value_name = "PARENT_PID")]
    parent: Option<sysinfo::Pid>,
}

fn main() {
    env_logger::Builder::from_default_env().init();
    let args = Args::parse();
    if let Some(parent_id) = args.parent {
        debug!("Parent PID: {}", parent_id);
        std::thread::spawn(move || {
            let mut sys = System::new_all();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(1000));
                if !sys.refresh_process(parent_id) {
                    debug!("Parent process is dead, exiting");
                    std::process::exit(0);
                }
            }
        });
    }
    let mut events: Vec<(String, Instant)> = Vec::new();
    if let Err(e) = rdev::listen(move |event| match (event.name, events.last()) {
        (None, _) => {}
        (Some(s), None) => {
            events.push((s, Instant::now()));
        }
        (Some(s), Some((_, last_t)))
            if last_t.elapsed().as_millis() < DELAY_MILLIS && s == "\r".to_string() =>
        {
            let res = events.iter().map(|(s, _)| s).cloned().collect::<String>();
            trace!("{}", last_t.elapsed().as_millis());
            info!("Scanned: {res}");
            println!("{res}");
            events.clear();
        }
        (Some(s), Some((_, last_t))) if last_t.elapsed().as_millis() < DELAY_MILLIS => {
            trace!("{}", last_t.elapsed().as_millis());
            events.push((s, Instant::now()));
        }
        (Some(s), _) => {
            events.clear();
            events.push((s, Instant::now()));
        }
    }) {
        error!("Error: {:?}", e);
    }
}
