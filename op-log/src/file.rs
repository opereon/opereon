use tracing_subscriber::{Layer, registry};
use tracing::{Subscriber, Event, Id};
use tracing_subscriber::layer::Context;
use tracing::span::Attributes;
use crate::Level;
use std::path::{PathBuf, Path};

pub struct FileLayer {
    level: Level,
    file_path: PathBuf,
}

impl FileLayer {
    pub fn new(level: &Level, file_path: &Path) -> Self {
        FileLayer {
            level: level.clone(),
            file_path: file_path.to_path_buf(),
        }
    }
}

impl<S> Layer<S> for FileLayer
    where
        S: Subscriber + for<'a> registry::LookupSpan<'a>, {
    fn new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        // let span = ctx.span(id).unwrap();
        // let mut ext = span.extensions_mut();
        // if ext.get_mut::<SpanFields>().is_none() {
        //     let fields = SpanFields::new(attrs);
        //     ext.insert(fields);
        // }
    }


    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {

        // println!("file: on_event {:?}, span: {:?}", event, ctx.current_span());
    }
}
