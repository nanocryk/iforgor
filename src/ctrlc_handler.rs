use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy)]
pub enum Mode {
    Ignore,
    Kill,
}

pub fn set_mode(mode: Mode) {
    static MODE: OnceLock<Mutex<Mode>> = OnceLock::new();
    let mutex = MODE.get_or_init(|| {
        // One first change we setup or custom Ctrl+C handler as it should
        // be setuped once.
        ctrlc::set_handler(|| {
            match *MODE
                .get()
                .expect("static MODE to be initialized")
                .lock()
                .unwrap()
            {
                Mode::Ignore => (),
                Mode::Kill => std::process::exit(1),
            }
        })
        .expect("Error setting Ctrl-C handler");

        Mutex::new(mode)
    });

    *mutex.lock().unwrap() = mode;
}
