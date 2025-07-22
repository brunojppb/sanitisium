use std::env;
use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::{MetricExporter, SpanExporter, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::metrics::{SdkMeterProvider, periodic_reader_with_async_runtime};
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, span_processor_with_async_runtime};
use opentelemetry_sdk::{Resource, runtime};
use opentelemetry_semantic_conventions::resource::{DEPLOYMENT_ENVIRONMENT_NAME, SERVICE_VERSION};
use tracing::{Subscriber, subscriber::set_global_default};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_log::LogTracer;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::Registry;
use tracing_subscriber::{
    EnvFilter,
    fmt::{self, MakeWriter},
    layer::SubscriberExt,
};

pub fn get_telemetry_subscriber<Sink>(
    name: &'static str,
    version: &'static str,
    env_name: &'static str,
    env_filter: &'static str,
    sink: Sink,
) -> impl Subscriber + Send + Sync
where
    Sink: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(env_filter));
    let formatting_layer = BunyanFormattingLayer::new(name.into(), sink);

    // Optionally, add another transport layer so we get
    // log outputs on a file to inspect once Sake stops running.
    let maybe_file_layer = match env::var("SANITISIUM_LOGS_DIR") {
        Ok(logs_dir) => {
            let file_appender =
                tracing_appender::rolling::never(logs_dir, format!("{}.log", &name));
            let file_layer = fmt::layer().with_writer(file_appender);
            Some(file_layer)
        }
        Err(_) => None,
    };

    let span_exporter = SpanExporter::builder()
        .with_http()
        .with_http_client(reqwest::Client::new())
        .with_protocol(opentelemetry_otlp::Protocol::HttpJson)
        .build()
        .expect("Could not create SpanExporter");

    let batch_processor = span_processor_with_async_runtime::BatchSpanProcessor::builder(
        span_exporter,
        runtime::Tokio,
    )
    .build();

    // Automatically export metrics every 2 seconds so we can monitor CPU and RAM utilization.
    let metrics_exporter = MetricExporter::builder()
        .with_http()
        .with_http_client(reqwest::Client::new())
        .with_protocol(opentelemetry_otlp::Protocol::HttpJson)
        .build()
        .expect("could not create MetricExporter");

    let periodic_reader = periodic_reader_with_async_runtime::PeriodicReader::builder(
        metrics_exporter,
        runtime::Tokio,
    )
    .with_interval(Duration::from_secs(2))
    .build();

    let tracer = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_span_processor(batch_processor)
        .with_sampler(Sampler::AlwaysOn)
        .with_id_generator(RandomIdGenerator::default())
        .with_max_events_per_span(64)
        .with_max_attributes_per_span(16)
        .with_resource(get_resource(name, version, env_name))
        .build()
        .tracer(name);

    let meter_provider = SdkMeterProvider::builder()
        .with_reader(periodic_reader)
        .with_resource(get_resource(name, version, env_name))
        .build();

    let opentelemetry_layer: OpenTelemetryLayer<Registry, _> = OpenTelemetryLayer::new(tracer);

    opentelemetry::global::set_meter_provider(meter_provider);

    tracing_subscriber::registry()
        .with(opentelemetry_layer)
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer)
        .with(maybe_file_layer)
}

/// Generate a resource with all the common markers for our traces and metrics
fn get_resource(service_name: &str, version: &str, env_name: &str) -> Resource {
    Resource::builder()
        .with_service_name(service_name.to_owned())
        .with_attribute(KeyValue::new(SERVICE_VERSION, version.to_owned()))
        .with_attribute(KeyValue::new(
            DEPLOYMENT_ENVIRONMENT_NAME,
            env_name.to_owned(),
        ))
        .with_attribute(KeyValue::new("env", env_name.to_owned()))
        .build()
}

/// Initialise the telemetry stack by setting up the global
/// default telemetry subscriber. The subscriber will handle log and tracing
/// events based on the pre-configured layers.
pub fn init_telemetry_subscriber(subscriber: impl Subscriber + Send + Sync) {
    LogTracer::init().expect("Could not set LogTracer as global logger");
    set_global_default(subscriber).expect("Failed to set subscriber");
}
