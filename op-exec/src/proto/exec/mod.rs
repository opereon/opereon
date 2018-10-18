use super::*;

use std::cell::RefCell;

thread_local!(static EXEC_PATH: RefCell<PathBuf> = RefCell::new(PathBuf::new()));

mod args;
mod proc;
mod run;
mod step;
mod task;

pub use self::args::*;
pub use self::proc::*;
pub use self::run::*;
pub use self::step::*;
pub use self::task::*;


#[cfg(test)]
mod tests {
    use super::*;

    fn resource_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources")
    }

    #[test]
    fn proc_exec_serialize() {
        let m = ModelRef::read(Metadata::default(), resource_path().join("model")).unwrap();
        let m = m.lock();
        let p = m.get_proc_path(&Opath::parse("$.proc.yum.procs.yum_check").unwrap()).unwrap();

        let mut e = ProcExec::new(Utc::now());
        e.prepare(&m, p, &m.metadata().path().join(".op")).unwrap();
        e.store().unwrap();

        println!("{}", serde_yaml::to_string(&e).unwrap());
    }
}