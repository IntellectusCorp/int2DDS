//! int2DDS Route Gateway
//!
//! Bridges DDS communication between a LAN (UDP) and a WAN (TCP) using
//! [`AutoRelay`]. Topics are discovered automatically via SEDP; for each
//! matching topic a [`TopicRelay`] is created that forwards data
//! bidirectionally between the LocalNode and RemoteNode participants.
//!
//! **Supported WAN transport: TCP only.** Set `remote.transport` to "tcp"
//! in the configuration; other transports are not validated for WAN use.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example route_gateway -- --config gateway.json
//! ```
//!
//! # Example configuration
//!
//! ```json
//! {
//!   "local":  { "domain_id": 0, "transport": "udp" },
//!   "remote": { "domain_id": 1, "transport": "tcp",
//!               "initial_peers": ["192.168.1.100:7400"] },
//!   "auto_relay": { "filter": "*" },
//!   "poll_period_ms": 50
//! }
//! ```

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::sleep,
    time::Duration,
};

use clap::Parser;
use std::sync::Arc as StdArc;

use int2dds::{
    common::{
        env::{set_console_log_level, set_log_type},
        log::{LogLevel, LogType},
    },
    domain::domain_participant_factory::DomainParticipantFactory,
    infrastructure::status::StatusMask,
    route_gateway::{AutoRelay, QosResolver, RouteGatewayConfig, TopicFilter},
};

#[derive(Parser, Debug)]
#[command(author, version, about = "int2DDS Route Gateway", long_about = None)]
struct Args {
    /// Path to the JSON configuration file.
    #[arg(short, long)]
    config: String,

    /// Verbose logging.
    #[arg(short, long, default_value = "false")]
    verbose: bool,
}

fn main() {
    let args = Args::parse();

    set_log_type(LogType::Console);
    set_console_log_level(if args.verbose { LogLevel::Debug } else { LogLevel::Info });

    let cfg = RouteGatewayConfig::from_file(&args.config)
        .unwrap_or_else(|e| panic!("Failed to load config '{}': {:?}", args.config, e));

    println!("[Route Gateway] starting");
    println!(
        "  local:  domain={}, transport={}",
        cfg.local.domain_id, cfg.local.transport
    );
    println!(
        "  remote: domain={}, transport={}, tls={}, initial_peers={:?}",
        cfg.remote.domain_id,
        cfg.remote.transport,
        if cfg.remote.tls.is_some() { "enabled" } else { "disabled" },
        cfg.remote.initial_peers
    );
    println!("  auto_relay filter: {}", cfg.auto_relay.filter);
    println!(
        "  qos rules: {} override(s), defaults L-R/L-W/R-R/R-W = [{:?}/{:?}/{:?}/{:?}]",
        cfg.topic_relays.len(),
        cfg.auto_relay.default_local_reader_qos,
        cfg.auto_relay.default_local_writer_qos,
        cfg.auto_relay.default_remote_reader_qos,
        cfg.auto_relay.default_remote_writer_qos,
    );
    println!("  poll period: {} ms", cfg.poll_period_ms);

    let qos_resolver = StdArc::new(
        QosResolver::from_config(&cfg.auto_relay, &cfg.topic_relays)
            .unwrap_or_else(|e| panic!("Invalid QoS configuration: {:?}", e)),
    );

    let factory = DomainParticipantFactory::get_instance();

    let local_node = Arc::new(
        factory
            .create_participant(
                cfg.local.domain_id,
                cfg.local.to_participant_qos(),
                None,
                StatusMask::default(),
            )
            .expect("Failed to create LocalNode participant"),
    );

    let remote_node = Arc::new(
        factory
            .create_participant(
                cfg.remote.domain_id,
                cfg.remote.to_participant_qos(),
                None,
                StatusMask::default(),
            )
            .expect("Failed to create RemoteNode participant"),
    );

    let auto = AutoRelay::with_qos(
        local_node,
        remote_node,
        TopicFilter::new(cfg.auto_relay.filter.clone()),
        qos_resolver,
    )
    .expect("Failed to create AutoRelay");

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        ctrlc::set_handler(move || {
            println!("\n[Route Gateway] shutdown signal received");
            stop.store(true, Ordering::SeqCst);
        })
        .expect("Failed to install Ctrl-C handler");
    }

    let period = Duration::from_millis(cfg.poll_period_ms);
    let mut last_topic_count = 0usize;

    // Forwarding heartbeat: every HEARTBEAT_EVERY, print a one-line summary
    // so operators can tell whether forwarding is flowing or silent.
    const HEARTBEAT_EVERY: Duration = Duration::from_secs(5);
    let mut last_heartbeat = std::time::Instant::now();
    let mut total_forwarded_since_last_hb: usize = 0;
    let mut total_forwarded_lifetime: usize = 0;
    let mut last_fwd_activity: Option<std::time::Instant> = None;

    println!("[Route Gateway] running. Press Ctrl-C to stop.\n");
    while !stop.load(Ordering::SeqCst) {
        if let Err(e) = auto.discover_once() {
            log::warn!("[Route Gateway] discover_once failed: {:?}", e);
        }
        match auto.forward_once() {
            Ok(n) => {
                if n > 0 {
                    total_forwarded_since_last_hb += n;
                    total_forwarded_lifetime += n;
                    last_fwd_activity = Some(std::time::Instant::now());
                }
            }
            Err(e) => log::warn!("[Route Gateway] forward_once failed: {:?}", e),
        }

        let count = auto.relay_count();
        if count != last_topic_count {
            println!("[Route Gateway] active topics ({}): {:?}", count, auto.active_topics());
            last_topic_count = count;
        }

        if last_heartbeat.elapsed() >= HEARTBEAT_EVERY {
            let idle_str = match last_fwd_activity {
                Some(t) => format!("{:?} ago", t.elapsed()),
                None => "never".to_string(),
            };
            log::info!(
                "[Route Gateway] heartbeat: topics={} fwd_5s={} fwd_total={} last_activity={}",
                count,
                total_forwarded_since_last_hb,
                total_forwarded_lifetime,
                idle_str,
            );
            total_forwarded_since_last_hb = 0;
            last_heartbeat = std::time::Instant::now();
        }

        sleep(period);
    }

    println!("[Route Gateway] stopped");
}
