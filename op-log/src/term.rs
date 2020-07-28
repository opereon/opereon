use colored::Colorize;
use std::fmt::Debug;
use tracing::field::Field;
use tracing::span::Attributes;
use tracing::{Event, Id, Level, Span, Subscriber};
use tracing_subscriber::field::Visit;
use tracing_subscriber::layer::Context;
use tracing_subscriber::{registry, Layer};

struct TermEvent<'a, 'b> {
    verb_field: &'a Field,
    target_verb: u64,
    event: &'a Event<'b>,
    verbosity: Option<u64>,
    recording: bool,
    kvs: Vec<(&'static str, String)>,
    message: Option<String>,
}

impl<'a, 'b> TermEvent<'a, 'b> {
    pub fn new(verb_field: &'a Field, event: &'b Event<'_>, target_verb: u64) -> Self {
        let mut evt = TermEvent {
            verb_field,
            target_verb,
            event,
            verbosity: None,
            recording: true,
            kvs: vec![],
            message: None,
        };
        event.record(&mut evt);
        evt
    }

    fn set_verbosity(&mut self, verbosity: u64) {
        self.verbosity = Some(verbosity);
        // skip remaining fields if event verbosity lower than expected
        // this is necessary because Event API limitations https://github.com/tokio-rs/tracing/issues/680
        self.recording = verbosity <= self.target_verb
    }

    pub fn verbosity(&self) -> &Option<u64> {
        &self.verbosity
    }

    pub fn print(&self) {
        if let Some(verbosity) = self.verbosity() {
            if *verbosity > self.target_verb {
                return;
            }
            let level = self.event.metadata().level();

            let level = match *level {
                Level::TRACE => "TRACE".white(),
                Level::DEBUG => "DEBUG".bright_black(),
                Level::INFO => "INFO".blue(),
                Level::WARN => "WARN".yellow(),
                Level::ERROR => "ERROR".bright_red(),
            };

            let fields = self
                .kvs
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join(", ");

            let fields = if fields.is_empty() {
                String::new()
            } else {
                format!("{{ {} }}", fields)
            };

            println!(
                "{level} {message} {fields}",
                level = level,
                message = self.message.as_ref().unwrap_or(&String::new()),
                fields = fields
            )
        }
    }
}

impl<'a, 'b> Visit for TermEvent<'a, 'b> {
    fn record_i64(&mut self, field: &Field, value: i64) {
        if self.verb_field == field {
            self.set_verbosity(value as u64)
        } else {
            self.record_debug(field, &value)
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if self.verb_field == field {
            self.set_verbosity(value)
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

const VERBOSITY_KEY: &str = "verb";

pub struct TermLayer {
    verbosity: u8,
}

impl TermLayer {
    pub fn new(verbosity: u8) -> Self {
        TermLayer { verbosity }
    }
}

impl<S> Layer<S> for TermLayer
where
    S: Subscriber + for<'a> registry::LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let verbosity = event.metadata().fields().field(VERBOSITY_KEY);
        if verbosity.is_none() {
            return;
        }
        let verbosity = verbosity.unwrap();
        let mut evt = TermEvent::new(&verbosity, event, self.verbosity as u64);
        evt.print()
    }
}
