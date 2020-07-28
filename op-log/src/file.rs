use crate::Level;
use slog::{o, Discard, Drain, Never, Record, Serializer, KV};
use std::fmt::Debug;
use std::fs::OpenOptions;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::field::{Field, Visit};
use tracing::span::Attributes;
use tracing::{Event, Id, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::{registry, Layer};

#[derive(Clone)]
struct SlogLogger(slog::Logger);

impl Deref for SlogLogger {
    type Target = slog::Logger;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

struct FileSpan {
    kvs: Vec<(&'static str, String)>,
}

impl FileSpan {
    pub fn from_attrs(attrs: &Attributes<'_>) -> FileSpan {
        let mut s = FileSpan { kvs: vec![] };
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
        for (k, v) in &self.kvs {
            serializer.emit_str(k, v)?
        }
        Ok(())
    }
}

struct FileEvent {
    kvs: Vec<(&'static str, String)>,
    message: Option<String>,
}

impl FileEvent {
    pub fn new(event: &Event<'_>) -> Self {
        let mut evt = FileEvent {
            kvs: vec![],
            message: None,
        };
        event.record(&mut evt);
        evt
    }
}

impl Visit for FileEvent {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        let val = format!("{:?}", value);
        if field.name() == "message" {
            self.message = Some(val)
        } else {
            self.kvs.push((field.name(), val))
        }
    }
}

impl KV for FileEvent {
    fn serialize(&self, _record: &Record<'_>, serializer: &mut dyn Serializer) -> slog::Result<()> {
        for (k, v) in &self.kvs {
            serializer.emit_str(k, v)?
        }
        Ok(())
    }
}

pub struct FileLayer {
    level: Level,
    file_path: PathBuf,
    root_logger: SlogLogger,
}

impl FileLayer {
    pub fn new(level: Level, file_path: &Path) -> Self {
        FileLayer {
            level,
            file_path: file_path.to_path_buf(),
            root_logger: SlogLogger(slog::Logger::root(Discard, o!())),
        }
    }

    pub fn init(&mut self) {
        let file_drain = build_file_drain(self.file_path.clone(), self.level.into());

        self.root_logger = SlogLogger(slog::Logger::root(file_drain, o!()))
    }
}

impl<S> Layer<S> for FileLayer
where
    S: Subscriber + for<'a> registry::LookupSpan<'a>,
{
    fn new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        let mut ext = span.extensions_mut();
        if ext.get_mut::<SlogLogger>().is_none() {
            let parent_logger = if let Some(parent) = span.parent() {
                let e = parent.extensions();
                e.get::<SlogLogger>().unwrap().clone()
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
            e.get::<SlogLogger>().unwrap().clone()
        } else {
            self.root_logger.clone()
        };

        let mut evt = FileEvent::new(event);

        let msg = evt.message.take().unwrap_or("".into());

        let module = event.metadata().module_path().unwrap_or("");

        match *event.metadata().level() {
            tracing::Level::TRACE => slog::trace!(l, "{}", msg; evt, "module"=> module),
            tracing::Level::DEBUG => slog::debug!(l, "{}", msg; evt, "module"=>module),
            tracing::Level::INFO => slog::info!(l, "{}", msg; evt, "module"=>module),
            tracing::Level::WARN => slog::warn!(l, "{}", msg; evt, "module"=>module),
            tracing::Level::ERROR => slog::error!(l, "{}", msg; evt, "module"=>module),
        }
    }
}

pub fn build_file_drain<P: AsRef<Path>>(
    log_path: P,
    level: slog::Level,
) -> impl Drain<Ok = (), Err = Never> {
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
