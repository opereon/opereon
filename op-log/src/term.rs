use tracing::field::Field;
use tracing_subscriber::field::Visit;
use std::fmt::Debug;
use tracing_subscriber::{Layer, registry};
use tracing::{Subscriber, Event, Id, Span};
use tracing::span::Attributes;
use tracing_subscriber::layer::Context;

struct TermEvent<'a> {
    verb_field: &'a Field,
    verbosity: Option<u64>,
    target_verb: u64,
    recording: bool,
    kvs: Vec<(&'static str, String)>,
    message: Option<String>
}

impl<'a> TermEvent<'a> {
    pub fn new(verb_field: &'a Field, target_verb: u64) -> Self {
        TermEvent {
            verb_field,
            verbosity: None,
            target_verb,
            recording: true,
            kvs: vec![],
            message: None
        }
    }

    pub fn verbosity(&self) -> &Option<u64> {
        &self.verbosity
    }

    pub fn write<T: std::io::Write>(buf: T) {

    }
}

impl <'a> Visit for TermEvent<'a>  {
    fn record_u64(&mut self, field: &Field, value: u64) {
        if self.verb_field == field {
            self.verbosity = Some(value);
            // skip remaining fields if event verbosity lower than expected
            // this is necessary because Event API limitations https://github.com/tokio-rs/tracing/issues/680
            self.recording = value <= self.target_verb
        } else {
            self.record_debug(field, &value)
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        if self.recording {
            if field.name() == "message" {
                self.message = Some(format!("{:?}", value))
            } else {
                self.kvs.push((field.name(), format!("{:?}", value)))
            }
        }
    }
}

pub struct SpanFields {
    kvs: Vec<(&'static str, String)>,
}

impl SpanFields {
    pub fn new(attrs: &Attributes<'_>) -> Self {
        let mut fields = SpanFields {
            kvs: vec![],
        };
        attrs.record(&mut fields);
        fields
    }
}

impl Visit for SpanFields {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.kvs.push((field.name(), format!("{:?}", value)))
    }
}

const VERBOSITY_KEY: &str = "verb";

pub struct TermLayer {
    verbosity: u8
}

impl TermLayer {
    pub fn new(verbosity: u8) -> Self {
        TermLayer { verbosity }
    }
}

impl<S> Layer<S> for TermLayer
    where
        S: Subscriber + for<'a> registry::LookupSpan<'a>, {

    fn new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        let mut ext = span.extensions_mut();
        if ext.get_mut::<SpanFields>().is_none() {
            let fields = SpanFields::new(attrs);
            ext.insert(fields);
        }
    }


    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let verbosity = event.metadata().fields().field(VERBOSITY_KEY);
        if verbosity.is_none() {
            return;
        }
        let verbosity = verbosity.unwrap();
        let mut evt = TermEvent::new(&verbosity, self.verbosity as u64);

        event.record(&mut evt);

        if evt.verbosity().is_none() {
            // verbosity have incompatible type
            return;
        }

        let current = ctx.current_span();

        let mut buf = String::new();


        if let Some(current_id) = current.id() {
            let mut current = ctx.span(current_id).unwrap();
            let parents = current.parents();
            for parent in parents {
                eprintln!("parent.name() = {:?}", parent.name());
            }
        }
    }
}