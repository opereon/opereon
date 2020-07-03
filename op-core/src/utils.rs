use shared_child::unix::SharedChildExt as OriginalSharedChildExt;
use shared_child::SharedChild;
use std::thread;

pub trait SharedChildExt {
    fn send_sigterm(&self);
}

impl SharedChildExt for SharedChild {
    fn send_sigterm(&self) {
        if let Err(err) = self.send_signal(libc::SIGTERM) {
            eprintln!("error sending sigterm signal = {:?}", err);
        }
    }
}
