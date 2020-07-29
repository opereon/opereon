use colored::Colorize;
use std::fmt::Debug;
use tracing::field::Field;

use tracing::span::{Attributes, Record};
use tracing::{Event, Id, Level, Span, Subscriber};
use tracing_subscriber::field::Visit;
use tracing_subscriber::layer::Context;
use tracing_subscriber::{registry, Layer};

struct VerbosityVisitor<'a> {
    verb_field: &'a Field,
    verbosity: Option<u64>,
}

impl<'a> VerbosityVisitor<'a> {
    pub fn new(verb_field: &'a Field, event: &'_ Event<'_>) -> Self {
        let mut evt = VerbosityVisitor {
            verb_field,
            verbosity: None,
        };
        event.record(&mut evt);
        evt
    }

    fn set_verbosity(&mut self, verbosity: u64) {
        self.verbosity = Some(verbosity);
    }

    pub fn verbosity(&self) -> &Option<u64> {
        &self.verbosity
    }
}

impl<'a> Visit for VerbosityVisitor<'a> {
    fn record_i64(&mut self, field: &Field, value: i64) {
        if self.verb_field == field {
            self.set_verbosity(value as u64)
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if self.verb_field == field {
            self.set_verbosity(value)
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn Debug) {}
}

const VERBOSITY_KEY: &str = "verb";

pub struct TermLayer<S> {
    verbosity: u8,
    inner: tracing_subscriber::fmt::Layer<S>,
}

impl<S> TermLayer<S> {
    pub fn new(verbosity: u8) -> Self {
        let inner = tracing_subscriber::fmt::Layer::new();
        TermLayer { verbosity, inner }
    }
}

impl<S> Layer<S> for TermLayer<S>
where
    S: Subscriber + for<'a> registry::LookupSpan<'a>,
{
    fn new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        self.inner.new_span(attrs, id, ctx)
    }

    fn on_record(&self, span: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        self.inner.on_record(span, values, ctx)
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let verbosity = event.metadata().fields().field(VERBOSITY_KEY);
        if verbosity.is_none() {
            return;
        }
        let verbosity = verbosity.unwrap();
        let evt = VerbosityVisitor::new(&verbosity, event);

        if let Some(verb) = evt.verbosity {
            if verb <= self.verbosity as u64 {
                self.inner.on_event(event, ctx)
            }
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        self.inner.on_enter(id, ctx)
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        self.inner.on_exit(id, ctx)
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        self.inner.on_close(id, ctx)
    }
}
