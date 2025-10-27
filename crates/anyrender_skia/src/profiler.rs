use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use hashbrown::HashMap;
use tracing::Subscriber;
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

static FUNCTION_DURATIONS: once_cell::sync::Lazy<Mutex<HashMap<String, Duration>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

pub fn record_duration(name: &str, duration: Duration) {
    let mut map = FUNCTION_DURATIONS.lock().unwrap();
    let total = map.entry(name.to_string()).or_insert(Duration::new(0, 0));
    *total += duration;
}

pub fn print_summary() {
    println!("--- Function Execution Summary ---");
    let map = FUNCTION_DURATIONS.lock().unwrap();
    let mut sorted_times: Vec<(&String, &Duration)> = map.iter().collect();
    sorted_times.sort_by(|a, b| b.1.cmp(a.1));

    for (name, duration) in sorted_times {
        let total_seconds = duration.as_secs_f64();
        println!("| {name} | Total Time: {:.3}s", total_seconds);
    }
    println!("----------------------------------");
}

pub struct ProfilingLayer;

impl<S> Layer<S> for ProfilingLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_close(&self, id: tracing::Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(&id) {
            let meta = span.metadata();
            if meta.target().starts_with("anyrender_skia") {
                let elapsed = span.extensions().get::<Instant>().map(|i| i.elapsed());

                if let Some(duration) = elapsed {
                    let function_name = meta.name();
                    record_duration(function_name, duration);
                }
            }
        }
    }

    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::Id,
        ctx: Context<'_, S>,
    ) {
        let span = ctx.span(id).expect("Span not found");
        span.extensions_mut().insert(Instant::now());
    }
}
