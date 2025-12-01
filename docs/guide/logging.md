# Rust Logging Guide

### The log Crate

Rust tends not to easily add new features to the standard library. This is because they are cautious about adding new functionality until they are confident that APIs included in the (permanent) standard library will be maintained for a long time, ensuring its stability. For this reason, `Logging` cannot be resolved through the standard library.

However, since there is a separate official package called `log`, this crate has become the standard logging solution in the Rust ecosystem. Most libraries use this `log` crate when implementing simple logging functionality.

### Log Macros

There are five logging levels, each with a corresponding macro. In order of importance (highest to lowest): **_error, warn, info, debug, trace_**.

Note that the exclamation mark (!) is not part of the macro name. The exclamation mark is part of the syntax for calling macros. So when importing macros, you don't include the exclamation mark, but when calling them, you append it after the macro name.

```rust
use log::{error, warn, info, debug, trace};

error!("Serious stuff");
warn!("Pay attention");
info!("Useful info");
debug!("Extra info");
trace!("All the things");
```

At program runtime, the log level is set to one of the five levels. Messages at or above the set level are output, while lower-level messages are ignored. For example, if the log level is set to ERROR, only ERROR level messages will be output. Each macro represents its own log level (e.g., warn! represents the WARN level) and takes a log message as an argument.

### Different Syntax from Functions: Macros

In some cases, you can specify a `target` as the first argument.

```rust
// [2025-02-14T06:03:21Z WARN  puzzle] Pay attention
warn!(target: "puzzle", "Pay attention");
```

Unlike regular functions, macros analyze input code token by token, allowing special syntax like `target: value`. In contrast, functions cannot pass arguments this way. If you don't specify a `target`, the name of the module where the code resides is automatically used.

Additionally, log macros work like `println!()`, so you can use format strings and multiple arguments.

```rust
warn!("Pay attention, minion {}!", 352);
```

### Separate Library for Log Output

However, the code above alone won't output any logs.

The `log` crate defines a common interface through a Trait that all loggers must implement. This allows various loggers—those that write to files, output to the console, or send over the network—to be used in the same way. In other words, libraries simply log to the `log` module, and the actual output is handled by separate logger implementations. As a result, logs remain compatible even if different libraries and applications use different loggers.

Think of logging as plumbing that connects various libraries and the applications that use them. Just as plumbing isn't complete with just pipes on the wall—you need somewhere for the logs to go—the code above is missing that part.

Since libraries only use the Log module like plumbing, a separate logging library is needed for output. This concept is called a **unified logging system**.

There are various logger libraries that actually output logs, and you can choose one based on your needs: console, syslog, regular files, Splunk, cloud servers, etc.

As an example, you can try the simple `env_logger` crate. `env_logger` reads an environment variable (`RUST_LOG`) to determine the log level and outputs to stderr.

Add `env_logger` to Cargo.toml and call `env_logger::init();` in your code, and you're ready to use the logger (output logs). By default, `env_logger`'s log level is set to **error**, so only ERROR level messages are output when run. If you set the environment variable `RUST_LOG` to `info` or another level before running, you'll see log messages at that level and above.

Below is an example of console output when `RUST_LOG` is set to `info`:

```rust
[2025-02-14T06:03:21Z INFO  frogger] A Frog hopped! It has 1 of energy left
[2025-02-14T06:03:21Z INFO  frogger] A Frog hopped! It has 0 of energy left
[2025-02-14T06:03:21Z WARN  frogger] The frog will go to sleep since he ran out of energy
[2025-02-14T06:03:21Z ERROR frogger] The frog is already asleep
```

Looking inside the brackets of the output logs, you can see information added by the logger: Timestamp, Log Level (with color based on level), target, etc.

There are also advanced logging systems for system programmers. If you need advanced features like structured logging, contexts, spans, or tracing requests through asynchronous code or multi-threading, you'll need to dive deeper and study the Tracing framework.
