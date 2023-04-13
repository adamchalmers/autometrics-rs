// Use the unstable `doc_cfg` feature when docs.rs is building the documentation
// https://stackoverflow.com/questions/61417452/how-to-get-a-feature-requirement-tag-in-the-documentation-generated-by-cargo-do/61417700#61417700
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![cfg_attr(docsrs, feature(doc_cfg_hide))]
#![cfg_attr(docsrs, doc(cfg_hide(doc)))]
#![doc = include_str!("../README.md")]

mod constants;
mod labels;
pub mod objectives;
#[cfg(feature = "prometheus-exporter")]
mod prometheus_exporter;
mod task_local;
mod tracker;

/// # Autometrics
///
/// Autometrics instruments your functions with automatically generated metrics
/// and writes Prometheus queries for you, making it easy for you to observe and
/// understand how your system performs in production.
///
/// ```
/// use autometrics::autometrics;
///
/// #[autometrics]
/// pub fn my_http_handler() {
///     // ...
/// }
/// ```
///
/// By default, Autometrics uses a counter and a histogram to track
/// the request rate, error rate, latency of calls to your functions.
///
/// For all of the generated metrics, Autometrics attaches the following labels:
/// - `function` - the name of the function
/// - `module` - the module path of the function (with `::` replaced by `.`)
///
/// For the function call counter, Autometrics attaches these additional labels:
/// - `result` - if the function returns a `Result`, this will either be `ok` or `error`
/// - `caller` - the name of the (autometrics-instrumented) function that called the current function
/// - (optional) `ok`/`error` - if the inner type implements `Into<&'static str>`, that value will be used as this label's value
///
/// ## Optional Parameters
///
/// ### `ok_if` and `error_if`
///
/// Example:
/// ```rust
/// # use autometrics::autometrics;
/// #[autometrics(ok_if = Option::is_some)]
/// pub fn db_load_key(key: &str) -> Option<String> {
///   None
/// }
/// ```
///
/// If the function does not return a `Result`, you can use `ok_if` and `error_if` to specify
/// whether the function call was "successful" or not, as far as the metrics are concerned.
///
/// For example, if a function returns an HTTP response, you can use `ok_if` or `error_if` to
/// add the `result` label based on the status code:
/// ```rust
/// # use autometrics::autometrics;
/// # use http::{Request, Response};
///
/// fn is_success<T>(res: &Response<T>) -> bool {
///     res.status().is_success()
/// }
///
/// #[autometrics(ok_if = is_success)]
/// pub async fn my_handler(req: Request<()>) -> Response<()> {
/// # Response::new(())
///     // ...
/// }
/// ```
///
/// Note that the function must be callable as `f(&T) -> bool`, where `T` is the return type
/// of the instrumented function.
///
/// ### `track_concurrency`
///
/// Example:
/// ```rust
/// # use autometrics::autometrics;
/// #[autometrics(track_concurrency)]
/// pub fn queue_task() { }
/// ```
///
/// Pass this argument to track the number of concurrent calls to the function (using a gauge).
/// This may be most useful for top-level functions such as the main HTTP handler that
/// passes requests off to other functions.
///
/// ### `objective`
///
/// Example:
/// ```rust
/// use autometrics::{autometrics, objectives::*};
///
/// const API_SLO: Objective = Objective::new("api")
///     .success_rate(ObjectivePercentile::P99_9);
///
/// #[autometrics(objective = API_SLO)]
/// pub fn handler() {
///    // ...
/// }
/// ```
///
/// Include this function's metrics in the specified objective or SLO.
///
/// See the docs for [Objective](https://docs.rs/autometrics/latest/autometrics/objectives/struct.Objective.html) for details on how to create objectives.
///
/// ## Instrumenting `impl` blocks
///
/// In addition to instrumenting functions, you can also instrument `impl` blocks.
///
/// Example:
/// ```rust
/// # use autometrics::autometrics;
/// struct MyStruct;
///
/// #[autometrics]
/// impl MyStruct {
///     #[skip_autometrics]
///     pub fn new() -> Self {
///        Self
///     }
///
///     fn my_function(&self) {
///        // ...
///    }
/// }
/// ```
///
/// This will instrument all functions in the `impl` block, except for those that have the `skip_autometrics` attribute.
///
pub use autometrics_macros::autometrics;

/// # Autometrics custom error labelling
///
/// The ErrorLabels derive macro allows to specify
/// inside an enumeration whether variants should be considered as errors or
/// successes as far as the [automatic metrics](autometrics) are concerned.
///
/// For example, this would allow you to put all the client-side errors in a
/// HTTP webserver (4**) as successes, since it means the handler function
/// _successfully_ rejected a bad request, and that should not affect the SLO or
/// the success rate of the function in the metrics.
///
/// Putting such a policy in place would look like this in code:
///
/// ```rust,ignore
/// use autometrics::ErrorLabels
///
/// #[derive(ErrorLabels)]
/// pub enum ServiceError {
///     // By default, the variant will be labeled as an error,
///     // so you do not need to decorate every variant
///     Database,
///     // It is possible to mention it as well of course.
///     // Only "error" and "ok" are accepted values
///     #[label(result = "error")]
///     Network,
///     #[label(result = "ok")]
///     Authentication,
///     #[label(result = "ok")]
///     Authorization,
/// }
///
/// pub type ServiceResult<T> = Result<T, ServiceError>;
/// ```
///
/// With these types, whenever a function returns a `ServiceResult`, having a
/// `ServiceError::Authentication` or `Authorization` would _not_ count as a
/// failure from your handler that should trigger alerts and consume the "error
/// budget" of the service.
pub use autometrics_macros::ErrorLabels;

// Optional exports
#[cfg(feature = "prometheus-exporter")]
pub use self::prometheus_exporter::*;

/// We use the histogram buckets recommended by the OpenTelemetry specification
/// https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/metrics/sdk.md#explicit-bucket-histogram-aggregation
#[cfg(any(feature = "prometheus", feature = "prometheus-exporter"))]
pub(crate) const HISTOGRAM_BUCKETS: [f64; 14] = [
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0,
];

// Not public API
// Note that this needs to be publicly exported (despite being called private)
// because it is used by code generated by the autometrics macro.
// We could move more or all of the code into the macro itself.
// However, the compiler would need to compile a lot of duplicate code in every
// instrumented function. It's also harder to develop and maintain macros with
// too much generated code, because rust-analyzer treats the macro code as a kind of string
// so you don't get any autocompletion or type checking.
#[doc(hidden)]
pub mod __private {
    use crate::task_local::LocalKey;
    use std::{cell::RefCell, thread_local};

    pub use crate::labels::*;
    pub use crate::tracker::{AutometricsTracker, TrackMetrics};

    /// Task-local value used for tracking which function called the current function
    pub static CALLER: LocalKey<&'static str> = {
        // This does the same thing as the tokio::thread_local macro with the exception that
        // it initializes the value with the empty string.
        // The tokio macro does not allow you to get the value before setting it.
        // However, in our case, we want it to simply return the empty string rather than panicking.
        thread_local! {
            static CALLER_KEY: RefCell<Option<&'static str>> = const { RefCell::new(Some("")) };
        }

        LocalKey { inner: CALLER_KEY }
    };
}
