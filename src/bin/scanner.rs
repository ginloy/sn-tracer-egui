use log::{debug, error};
use std::time::Instant;

const DELAY_MICROS: u128 = 10000;

fn main() {
    env_logger::Builder::from_default_env().init();
    let mut events: Vec<(String, Instant)> = Vec::new();
    if let Err(e) = rdev::listen(move |event| match (event.name, events.last()) {
        (None, _) => {}
        (Some(s), None) => {
            events.push((s, Instant::now()));
        }
        (Some(s), Some((_, last_t)))
            if last_t.elapsed().as_micros() < DELAY_MICROS && s == "\r".to_string() =>
        {
            debug!("{}", last_t.elapsed().as_micros());
            println!(
                "{}",
                events.iter().map(|(s, _)| s).cloned().collect::<String>()
            );
            events.clear();
        }
        (Some(s), Some((_, last_t))) if last_t.elapsed().as_micros() < DELAY_MICROS => {
            debug!("{}", last_t.elapsed().as_micros());
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
