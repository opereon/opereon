use tracing_subscriber::{Layer, registry};
use tracing::{Subscriber, Event, Id};
use tracing_subscriber::layer::Context;
use tracing::span::Attributes;
use crate::{Level, DiscardLogger};
use std::path::{PathBuf, Path};
use slog::{Drain, Never, Logger, Discard, o, KV, Record, Serializer};
use std::fs::OpenOptions;
use std::sync::Mutex;
use std::ops::Deref;
use tracing::field::{Visit, Field};
use std::fmt::Debug;

#[derive(Clone)]
struct SlogLogger(slog::Logger);

impl SlogLogger {
    pub fn clone_inner(&self) -> slog::Logger {
        self.0.clone()
    }
}

impl Deref for SlogLogger {
    type Target = slog::Logger;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for SlogLogger {
    fn drop(&mut self) {
        println!("logger dropped");
    }
}


struct FileSpan {
    kvs: Vec<(&'static str, String)>
}

impl FileSpan {
    pub fn from_attrs(attrs: &Attributes<'_>) -> FileSpan{
        let mut s = FileSpan {
            kvs: vec![]
        };
        attrs.record(&mut s);
        s
    }
}

impl Visit for FileSpan {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.kvs.push((field.name(), format!("{:?}", value)))
    }
}

impl KV for FileSpan {
    fn serialize(&self, _record: &Record<'_>, serializer: &mut dyn Serializer) -> slog::Result<()> {
        for (k, v ) in &self.kvs {
            serializer.emit_str(k, v)?
        }
        Ok(())
    }
}


pub struct FileLayer {
    level: Level,
    file_path: PathBuf,
    root_logger: Logger
}

impl FileLayer {
    pub fn new(level: &Level, file_path: &Path) -> Self {
        FileLayer {
            level: level.clone(),
            file_path: file_path.to_path_buf(),
            root_logger: slog::Logger::root(Discard, o!())
        }
    }

    pub fn init(&mut self) {
        let file_drain = build_file_drain(self.file_path.clone(), self.level.into());

        self.root_logger = slog::Logger::root(file_drain, o!())
    }
}

impl<S> Layer<S> for FileLayer
    where
        S: Subscriber + for<'a> registry::LookupSpan<'a>, {
    fn new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        eprintln!("new span {:?}", id);
        let span = ctx.span(id).unwrap();
        let mut ext = span.extensions_mut();
        if ext.get_mut::<SlogLogger>().is_none() {
            let parent_logger = if let Some(parent) = span.parent() {
                let e = parent.extensions();
                e.get::<SlogLogger>().unwrap().clone_inner()
            } else {
                self.root_logger.clone()
            };
            let kvs = o!(FileSpan::from_attrs(attrs));
            let logger = SlogLogger(parent_logger.new(kvs));
            ext.insert(logger);
        }
    }


    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let current = ctx.current_span();

        let l = if let Some(id) = current.id() {
            let span = ctx.span(id).unwrap();
            let e = span.extensions();
            e.get::<SlogLogger>().unwrap().clone_inner()
        } else {
            self.root_logger.clone()
        };
        // let level = event.metadata().level().clone();
        // let level = Level::from(level);
        // let level: slog::Level = level.into();
        slog::info!(l, "test")
        // println!("file: on_event {:?}, span: {:?}", event, ctx.current_span());
    }

    fn on_close(&self, _id: Id, _ctx: Context<'_, S>) {
        eprintln!("span closed = {:?}", _id);
    }
}


pub fn build_file_drain<P: AsRef<Path>>(
    log_path: P,
    level: slog::Level,
) -> impl Drain<Ok=(), Err=Never> {
    if let Some(log_dir) = log_path.as_ref().parent() {
        std::fs::create_dir_all(log_dir).expect("Cannot create log dir");
    }

    let mut open_opts = OpenOptions::new();

    open_opts.create(true).append(true);

    let log_file = open_opts.open(log_path).expect("Cannot open log file");

    let drain = slog_bunyan::default(log_file);

    //    let decorator = slog_term::PlainSyncDecorator::new(log_file.try_clone().unwrap());
    //    let drain = slog_term::FullFormat::new(decorator).build();
    let drain = slog::LevelFilter::new(Mutex::new(drain), level);
    drain.fuse()
}