# 📖 Rust [Tracing](https://crates.io/crates/tracing) Crate Guide

## Tracing Crate
While the `log` crate provides simple logging, the `Tracing` crate offers more powerful and structured diagnostic information. It is especially optimized for event tracing in complex systems such as asynchronous code, multi-threaded applications, and microservice environments.

`tracing` was designed to overcome the limitations of traditional logging.
- Track the entire request processing flow as a single path
- Log with structured data format
- Maintain execution context while logging
- Track operations in asynchronous and parallel code

## Configuration
### 1. Adding Dependencies
First, add the necessary dependencies to `Cargo.toml`.
```toml
[dependencies]
tracing = "0.1"
tracing-subscriber = "0.3"
tracing-appender = "0.2" # Used for file storage, etc.
```
### 2. Basic Configuration
Initialize `tracing` at the program's entry point.
```rust
fn main() {
    // Initialize default subscriber
    tracing_subscriber::fmt()
        .init();

    // tracing implementation...
}
```
## Basic Usage
### 1. Simple Logging
`tracing` provides 5 macros compatible with the `log` crate.
```rust
use tracing::{error, warn, info, debug, trace};

fn basic_logging() {
    // Log output for each level
    error!("Critical error occurred!");
    warn!("Situation requiring attention");
    info!("General information");
    debug!("Debugging information");
    trace!("Detailed trace information");
}
```
### 2. Setting Log Levels
Control log levels through environment variables
```rust
use tracing_subscriber::EnvFilter;

fn setup_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        // Or specify the level directly
        // .with_env_filter("debug")
        .init();
}
```
### 3. Saving Logs to File
```rust
use tracing_appender::rolling::{RollingFileAppender, Rotation};

fn main() {
    let _guard = setup_file_logging();

    error!("error!");

    std::thread::sleep(std::time::Duration::from_secs(10));
}

fn setup_file_logging() -> impl Drop {
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("app.log")
        .build("logs")
        .expect("Failed to create log file");

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt().with_writer(non_blocking).init();

    guard
}
```
### 4. Structured Logging
You can include additional information in logs.
```rust
fn main() {
    // 2025-02-20T07:08:43.569942Z  INFO tracing_example: User login user_id=123 action="login"
    structured_logging(123);
}

fn structured_logging(user_id: u32) {
    info!(
        user_id = user_id,
        action = "login",
        "User login"
    );
}
```
### 5. Using Spans
Spans are used to track operations that have a beginning and an end.
```rust
use tracing::{info, info_span};

fn main() {
    /*
    2025-02-20T07:10:30.049866Z  INFO data_processing{data_length=13}: tracing_example: Starting data processing
    2025-02-20T07:10:30.049885Z  INFO data_processing{data_length=13}: tracing_example: Data processing complete
    */
    process_data("Hello, World!");
}

fn process_data(data: &str) {
    // Create span
    let span = info_span!("data_processing", data_length = data.len());
    // Enter span
    let _enter = span.enter();

    info!("Starting data processing");
    // ... process data ...
    info!("Data processing complete");
    // Span automatically ends when it goes out of scope
}
```
## Span Detailed Guide
### 1. Creating Spans and Attributes
Spans can be created and assigned attributes in various ways.
```rust
fn span_examples() {
    // Basic span creation
    let span = span!(Level::INFO, "my_span");

    // Span with fields
    let span_with_fields = span!(
        Level::DEBUG,
        "process_data",
        data_size = 100,
        category = "important"
    );

    // Using level-specific convenience macros
    let debug_span = debug_span!("debug_operation");
    let info_span = info_span!("info_operation");
    let warn_span = warn_span!("warn_operation");
    let error_span = error_span!("error_operation");
}
```
For example, here's a practical example using these concepts:
```rust
fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .init();

    span_examples();
}

fn span_examples() {
    let span = span!(Level::INFO, "my_span").entered();
    info!("Info log inside my_span");

    {
        let span_with_fields = span!(
            Level::DEBUG,
            "process_data",
            data_size = 100,
            category = "important"
        )
        .entered();
        debug!("Debug log inside process_data span");
    }

    {
        let _debug_span = debug_span!("debug_operation").entered();
        debug!("Log inside debug span");
    }
    {
        let _info_span = info_span!("info_operation").entered();
        info!("Log inside info span");
    }
    {
        let _warn_span = warn_span!("warn_operation").entered();
        warn!("Log inside warn span");
    }
    {
        let _error_span = error_span!("error_operation").entered();
        error!("Log inside error span");
    }
}
```
Running the above code will produce the following output:
```bash
2025-02-20T07:22:38.085320Z  INFO my_span: tracing_example: Info log inside my_span
2025-02-20T07:22:38.085348Z DEBUG my_span:process_data{data_size=100 category="important"}: tracing_example: Debug log inside process_data span
2025-02-20T07:22:38.085368Z DEBUG my_span:debug_operation: tracing_example: Log inside debug span
2025-02-20T07:22:38.085381Z  INFO my_span:info_operation: tracing_example: Log inside info span
2025-02-20T07:22:38.085399Z  WARN my_span:warn_operation: tracing_example: Log inside warn span
2025-02-20T07:22:38.085414Z ERROR my_span:error_operation: tracing_example: Log inside error span
```
### 2. Setting Span Relationships
Spans can have various relationships with each other.
```rust
use tracing::{info, span, Level};

fn span_relationships() {
    // Parent-child relationship
    let parent = span!(Level::INFO, "parent_operation");
    let child = span!(parent: &parent, Level::DEBUG, "child_operation");

    // Sequential relationship
    let span1 = span!(Level::INFO, "first_operation");
    let span2 = span!(Level::INFO, "second_operation");
    span2.follows_from(&span1);
}
```
### 3. Span Lifecycle Management
Here's how to effectively manage span lifecycles:
```rust
use tracing::{info, info_span};

fn span_lifecycle() {
    // Automatic lifecycle management (recommended)
    {
        let _enter = info_span!("automatic_span").entered();
        info!("This event occurs inside the span");
    } // Span automatically ends

    // Manual lifecycle management
    let span = info_span!("manual_span");
    let _guard = span.enter();
    info!("Working inside span");
    // Span ends when _guard goes out of scope
}
```
### 4. Reusing Spans
The same span can be used multiple times.
```rust
use tracing::info_span;

fn reuse_span() {
    let span = info_span!("reusable_span");

    // First use
    {
        let _guard = span.enter();
        // Perform work
    }

    // Perform other work

    // Second use
    {
        let _guard = span.enter();
        // Perform other work
    }
}
```
### 5. Conditional Span Creation
Different spans can be created depending on the situation.
```rust
use tracing::{info_span, Level, span};

fn conditional_spans(condition: bool, importance: &str) {
    let span = if condition {
        // Span based on importance when condition is true
        match importance {
            "high" => span!(Level::ERROR, "high_priority"),
            "medium" => span!(Level::WARN, "medium_priority"),
            _ => span!(Level::INFO, "low_priority"),
        }
    } else {
        // Default span when condition is false
        span!(Level::TRACE, "conditional_disabled")
    };

    let _guard = span.enter();
    // Perform work
}
```
### 6. Advanced Span Usage in Asynchronous Contexts
According to the `tracing` documentation, if a guard created with `.enter()` (an object representing the active state of a span) is held across `await` points, the asynchronous execution context may change, potentially producing inaccurate trace information.
To solve this problem, instead of entering a span directly with `span.enter()`, it is recommended to use the `.in_scope()` method, the `Instrument` trait, or the `#[instrument]` macro.

For example:
```rust
// Potentially problematic approach
use tracing::{debug_span};

async fn problematic_function() {
    let span = debug_span!("async_operation");
    let _guard = span.enter(); // This guard crosses await points

    some_async_operation().await; // Trace information may be incomplete
}

// Recommended approach
use tracing::{debug_span, Instrument};

async fn recommended_function() {
    let span = debug_span!("async_operation");

    // Using in_scope
    span.in_scope(|| async {
        some_async_operation().await;
    }).await;

    // Or using instrument
    async {
        some_async_operation().await;
    }
    .instrument(debug_span!("async_operation"))
    .await;
}
// Or using the instrument macro
#[instrument]
async fn some_async_operation() {
    // Function content
}
```

The first method, `in_scope`, executes an asynchronous code block within the context of the given span. This is a functional approach where asynchronous operations run inside a closure. The span is active while the operation is in progress and ends when the operation completes.

The second method, the `instrument` trait, is a more declarative approach. It attaches a span to an asynchronous Future, automatically activating and deactivating the span each time the Future is polled. This is particularly useful for asynchronous code.

The third method, the `#[instrument]` attribute macro, is declared above a function definition. This macro automatically includes function arguments as span fields and rewrites the function at compile time to insert tracing code. It allows you to trace the entire function execution as a single span with the most concise syntax.

The `instrument` trait approach and `#[instrument]` macro approach are generally more recommended for the following reasons:
- More concise
- Correctly maintains span context even when async tasks are suspended and resumed
- Trace information is preserved across `.await` points
### 7. Dynamic Span Field Updates
Span fields can be updated during execution.
```rust
use tracing::info_span;

fn updating_span_fields() {
    let span = info_span!("processing", state = "starting", items = 0);
    let _guard = span.enter();

    // Update fields
    span.record("state", "processing");
    span.record("items", 5);

    // Perform work
    span.record("state", "completed");
}
```
